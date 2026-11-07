//! Tauri commands — the whole surface the UI can call.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::db::{self, Db};
use crate::error::{AppError, Result};
use crate::launch;
use crate::media;
use crate::models::*;
use crate::platforms;
use crate::scan;
use crate::scrape;
use crate::AppState;

// ------------------------------------------------------------- library

#[tauri::command]
pub fn list_games(db: State<Db>, filter: GameFilter) -> Result<Vec<Game>> {
    let conn = db.0.lock().unwrap();
    db::list_games(&conn, &filter)
}

#[tauri::command]
pub fn get_game(db: State<Db>, id: i64) -> Result<Option<Game>> {
    let conn = db.0.lock().unwrap();
    db::get_game(&conn, id)
}

#[tauri::command]
pub fn list_platforms(db: State<Db>) -> Result<Vec<PlatformInfo>> {
    let conn = db.0.lock().unwrap();
    db::platform_counts(&conn)
}

/// Every platform we know about, for dropdowns — not just ones with games.
#[tauri::command]
pub fn known_platforms() -> Vec<PlatformInfo> {
    platforms::PLATFORMS
        .iter()
        .map(|p| PlatformInfo {
            slug: p.slug.to_string(),
            name: p.name.to_string(),
            extensions: p.exts.iter().map(|s| s.to_string()).collect(),
            cores: p.cores.iter().map(|s| s.to_string()).collect(),
            game_count: 0,
        })
        .collect()
}

#[tauri::command]
pub fn library_stats(db: State<Db>) -> Result<LibraryStats> {
    let conn = db.0.lock().unwrap();
    db::stats(&conn)
}

#[tauri::command]
pub fn set_favorite(db: State<Db>, id: i64, value: bool) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::set_favorite(&conn, id, value)
}

#[tauri::command]
pub fn set_hidden(db: State<Db>, id: i64, value: bool) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::set_hidden(&conn, id, value)
}

#[tauri::command]
pub fn set_game_platform(db: State<Db>, id: i64, platform: String) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::set_platform(&conn, id, &platform)
}

/// Remove from the library only. The ROM file is left alone.
#[tauri::command]
pub fn remove_game(state: State<AppState>, db: State<Db>, id: i64) -> Result<()> {
    media::clear_game_media(&state.media_root, id);
    let conn = db.0.lock().unwrap();
    db::remove_game(&conn, id)
}

// ------------------------------------------------------------- folders

#[tauri::command]
pub fn list_library_folders(db: State<Db>) -> Result<Vec<LibraryFolder>> {
    let conn = db.0.lock().unwrap();
    db::list_folders(&conn)
}

#[tauri::command]
pub fn add_library_folder(db: State<Db>, path: String, platform: Option<String>) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::add_folder(&conn, &path, platform.as_deref().filter(|p| !p.is_empty()))
}

#[tauri::command]
pub fn remove_library_folder(db: State<Db>, id: i64) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::remove_folder(&conn, id)
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |picked| {
        let _ = tx.send(picked);
    });
    rx.await
        .ok()
        .flatten()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn pick_file(app: AppHandle) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_file(move |picked| {
        let _ = tx.send(picked);
    });
    rx.await
        .ok()
        .flatten()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().to_string())
}

// ------------------------------------------------------- custom artwork

/// Extensions the webview can actually display, which is the real constraint.
///
/// Accepting a TIFF would look like it worked and then show nothing, so the
/// picker offers only formats that will render once they are in place.
const COVER_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "avif"];

/// Choose a picture for a game by hand.
///
/// For the games the providers have never heard of — a hack, an obscure dump,
/// a homebrew — there is otherwise nothing to look at but a placeholder. The
/// chosen file is copied into the game's own media folder rather than
/// referenced where it sits, for the same reason ROMs are not copied but
/// artwork is: the library owns its artwork, and pointing at a file in
/// somebody's Downloads folder means the cover disappears when they tidy up.
///
/// Returns the new cover path, or `None` if the dialog was dismissed.
#[tauri::command]
pub async fn choose_custom_cover(app: AppHandle, id: i64) -> Result<Option<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Images", COVER_EXTENSIONS)
        .pick_file(move |picked| {
            let _ = tx.send(picked);
        });

    let Some(source) = rx
        .await
        .ok()
        .flatten()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if !COVER_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AppError::Other(format!(
            "{ext} is not an image Arcadex can show. Use a PNG, JPEG, WebP, GIF, BMP or AVIF."
        )));
    }

    let media_root = app.state::<AppState>().media_root.clone();
    let dir = media::game_media_dir(&media_root, id);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Other(e.to_string()))?;

    // Named by extension, and any previous choice in another format is removed,
    // so a game cannot end up with custom.png and custom.jpg both on disk and
    // only one of them referenced.
    for old in COVER_EXTENSIONS {
        let stale = dir.join(format!("custom.{old}"));
        if stale.exists() && *old != ext {
            let _ = std::fs::remove_file(stale);
        }
    }

    let dest = dir.join(format!("custom.{ext}"));
    std::fs::copy(&source, &dest).map_err(|e| AppError::Other(e.to_string()))?;

    let dest_str = dest.to_string_lossy().to_string();
    {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        db::set_custom_cover(&conn, id, &dest_str)?;
    }
    Ok(Some(dest_str))
}

/// Rename a game. Empty names are refused rather than saved as a blank tile.
#[tauri::command]
pub fn rename_game(db: State<Db>, id: i64, title: String) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::Other("A game needs a name.".into()));
    }
    let conn = db.0.lock().unwrap();
    db::rename_game(&conn, id, title)
}

/// Give a game the details and artwork of another game, keeping its name.
///
/// The artwork files are copied into the receiving game's own media folder.
/// Pointing at the source's files would look identical until the source game
/// was removed, at which point its media folder is deleted and this game's art
/// goes with it.
#[tauri::command]
pub fn borrow_metadata(app: AppHandle, from: i64, to: i64) -> Result<()> {
    if from == to {
        return Err(AppError::Other("Pick a different game to copy from.".into()));
    }

    let (cover, shot, logo) = {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        db::artwork_paths(&conn, from)?
    };

    let media_root = app.state::<AppState>().media_root.clone();
    let dir = media::game_media_dir(&media_root, to);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Other(e.to_string()))?;

    // A source file that has gone missing is skipped rather than failing the
    // whole copy: the text is still worth having.
    let copy = |src: &Option<String>, stem: &str| -> Option<String> {
        let src = std::path::PathBuf::from(src.as_ref()?);
        if !src.is_file() {
            return None;
        }
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("img");
        let dest = dir.join(format!("{stem}.{ext}"));
        std::fs::copy(&src, &dest).ok()?;
        Some(dest.to_string_lossy().to_string())
    };

    let cover = copy(&cover, "borrowed-cover");
    let shot = copy(&shot, "borrowed-screenshot");
    let logo = copy(&logo, "borrowed-logo");

    let db = app.state::<Db>();
    let conn = db.0.lock().unwrap();
    db::borrow_metadata(
        &conn,
        from,
        to,
        cover.as_deref(),
        shot.as_deref(),
        logo.as_deref(),
    )
}

/// Drop a hand-picked cover, letting metadata supply one again.
#[tauri::command]
pub fn clear_custom_cover(db: State<Db>, id: i64) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::clear_custom_cover(&conn, id)
}

// ---------------------------------------------------------------- scan

#[tauri::command]
pub async fn scan_library(app: AppHandle) -> Result<ScanProgress> {
    let handle = app.clone();
    let tally = tauri::async_runtime::spawn_blocking(move || -> Result<scan::ScanTally> {
        let db_handle = handle.state::<Db>().0.clone();
        scan::scan_all(&handle, &db_handle)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    Ok(ScanProgress {
        phase: "done".into(),
        current: 0,
        total: 0,
        message: tally.message(),
        added: tally.added,
        skipped: tally.skipped,
        corrected: tally.reindexed,
        dropped: tally.dropped,
        ignored: tally.ignored,
        ignored_summary: tally.summary(),
        done: true,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingReport {
    /// Entries whose file is gone.
    pub missing: usize,
    /// Entries on a drive or share that is not there, which are kept.
    pub unreachable: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub folders: usize,
    pub games: i64,
    /// Systems you have games for that nothing is set up to run yet.
    pub systems_without_emulator: Vec<String>,
    /// Whether anything beyond artwork - descriptions, release dates - can
    /// be fetched.
    pub descriptions: bool,
}

/// How far setting up has got, for the first-run checklist.
///
/// The first thing most people did was add a folder, scan, press Play, and
/// be told RetroArch was not set. This is what lets the app say which
/// systems cannot start yet before anyone presses anything.
#[tauri::command]
pub async fn setup_status(app: AppHandle) -> Result<SetupStatus> {
    off_main(move || {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        let retroarch = db::get_setting(&conn, "retroarch_path")?
            .map(|p| crate::detect::clean_path(&p))
            .is_some_and(|p| !p.is_empty() && std::path::Path::new(&p).exists());
        let mut without = Vec::new();
        for p in db::platform_counts(&conn)? {
            let cfg = launch::resolve_config(&conn, &p.slug);
            let ready = match cfg.mode.as_str() {
                "custom" => cfg.custom_command.as_deref().is_some_and(|c| !c.trim().is_empty()),
                _ => retroarch && cfg.core.as_deref().is_some_and(|c| !c.is_empty()),
            };
            if !ready {
                without.push(platforms::display_name(&p.slug));
            }
        }
        let settings = crate::secrets::all_settings(&conn, &app.state::<AppState>().vault)?;
        let creds = scrape::Credentials::from_settings(&settings);
        Ok(SetupStatus {
            folders: db::list_folders(&conn)?.len(),
            games: db::stats(&conn)?.total_games,
            systems_without_emulator: without,
            descriptions: creds.has_screenscraper() || creds.has_tgdb(),
        })
    })
    .await
}

/// Where sync stands: the folder, whether it is there, and who else syncs.
#[tauri::command]
pub async fn sync_status(app: AppHandle) -> Result<crate::sync::Status> {
    off_main(move || crate::sync::status(&app.state::<Db>().0)).await
}

/// Read the other machines' play time now, and write this one's.
#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<crate::sync::Imported> {
    off_main(move || {
        crate::sync::sync_play(&app.state::<Db>().0)?
            .ok_or_else(|| AppError::Other("Choose a sync folder first".into()))
    })
    .await
}

/// What cleaning up would remove, so it can be said before it is done.
#[tauri::command]
pub async fn missing_report(app: AppHandle) -> Result<MissingReport> {
    off_main(move || {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        let (missing, unreachable) = db::missing_entries(&conn)?;
        Ok(MissingReport {
            missing: missing.len(),
            unreachable,
        })
    })
    .await
}

#[tauri::command]
pub async fn clean_missing(app: AppHandle) -> Result<usize> {
    off_main(move || {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        scan::remove_missing(&conn)
    })
    .await
}

// -------------------------------------------------------------- scrape

#[tauri::command]
pub fn cancel_scrape(state: State<AppState>) {
    state.scrape_cancel.store(true, Ordering::SeqCst);
}

/// Fetch metadata across the library.
///
/// `refetch` is the difference between the pass that runs after a scan, which
/// only picks up what has none, and the explicit "fetch everything again",
/// which goes back over games that already have art. Absent means the former,
/// so the automatic path cannot start spending quota by accident.
#[tauri::command]
pub async fn scrape_library(
    app: AppHandle,
    platform: Option<String>,
    refetch: Option<bool>,
) -> Result<String> {
    let state = app.state::<AppState>();
    if state
        .scrape_running
        .swap(true, Ordering::SeqCst)
    {
        return Err(AppError::Other("A scrape is already running".into()));
    }
    state.scrape_cancel.store(false, Ordering::SeqCst);

    let client = state.client.clone();
    let media_root = state.media_root.clone();

    let outcome = run_scrape(&app, &client, &media_root, platform, refetch.unwrap_or(false)).await;

    let state = app.state::<AppState>();
    state.scrape_running.store(false, Ordering::SeqCst);
    outcome
}

async fn run_scrape(
    app: &AppHandle,
    client: &reqwest::Client,
    media_root: &std::path::Path,
    platform: Option<String>,
    refetch: bool,
) -> Result<String> {
    let db = app.state::<Db>();

    let (games, creds) = {
        let conn = db.0.lock().unwrap();
        let games = if refetch {
            db::games_for_rescrape(&conn, platform.as_deref())?
        } else {
            db::games_needing_scrape(&conn, platform.as_deref())?
        };
        let settings = crate::secrets::all_settings(&conn, &app.state::<AppState>().vault)?;
        (games, scrape::Credentials::from_settings(&settings))
    };

    if !creds.has_any() {
        return Err(AppError::Other(
            "Every metadata source is switched off. Turn libretro artwork back on, or add ScreenScraper or TheGamesDB credentials, in Settings."
                .into(),
        ));
    }

    let total = games.len();
    let mut ok = 0usize;
    let mut missed = 0usize;
    let mut halted: Option<String> = None;
    let mut spent = scrape::Spent::default();

    for (i, game) in games.iter().enumerate() {
        let cancel = app.state::<AppState>().scrape_cancel.load(Ordering::SeqCst);
        if cancel {
            halted = Some("Cancelled".into());
            break;
        }

        // One game going wrong - artwork that will not save, a database
        // hiccup - is that game's error, not the run's. Letting it end the run
        // also skipped the closing event, which left the progress card up for
        // good.
        let res = match scrape::scrape_game(client, &db.0, &creds, media_root, game, &mut spent).await {
            Ok(res) => res,
            Err(e) => {
                if let Ok(guard) = db.0.lock() {
                    let _ = db::set_scrape_status(&guard, game.id, "error");
                }
                eprintln!("fetching {} failed: {e}", game.title);
                scrape::ScrapeResult {
                    status: "error".into(),
                    source: None,
                    halt: None,
                }
            }
        };
        if res.status == "ok" {
            ok += 1;
        } else {
            missed += 1;
        }

        let _ = app.emit(
            "scrape-progress",
            ScrapeProgress {
                current: i + 1,
                total,
                game_id: game.id,
                title: game.title.clone(),
                status: res.status.clone(),
                source: res.source.clone(),
                done: false,
                halted_reason: res.halt.clone(),
            },
        );

        if let Some(reason) = res.halt {
            halted = Some(reason);
            break;
        }

        // ScreenScraper rejects bursts; a small gap keeps a big library within
        // the per-minute allowance.
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
    }

    let summary = match &halted {
        Some(reason) => format!("Stopped after {ok} matched, {missed} unmatched — {reason}"),
        None if !spent.notes.is_empty() => format!(
            "{ok} matched, {missed} unmatched. Rested partway: {}",
            spent.notes.join("; ")
        ),
        None => format!("{ok} matched, {missed} unmatched"),
    };

    let _ = app.emit(
        "scrape-progress",
        ScrapeProgress {
            current: total,
            total,
            game_id: 0,
            title: String::new(),
            status: "done".into(),
            source: None,
            done: true,
            halted_reason: halted,
        },
    );

    Ok(summary)
}

/// Re-fetch metadata for one game, ignoring its current status.
#[tauri::command]
pub async fn scrape_one(app: AppHandle, id: i64) -> Result<String> {
    let state = app.state::<AppState>();
    let client = state.client.clone();
    let media_root = state.media_root.clone();
    let db = app.state::<Db>();

    let (game, creds) = {
        let conn = db.0.lock().unwrap();
        let game = db::get_game(&conn, id)?
            .ok_or_else(|| AppError::Other("Game not found".into()))?;
        let settings = crate::secrets::all_settings(&conn, &app.state::<AppState>().vault)?;
        (game, scrape::Credentials::from_settings(&settings))
    };

    if !creds.has_any() {
        return Err(AppError::Other(
            "Every metadata source is switched off. Turn libretro artwork back on in Settings."
                .into(),
        ));
    }

    let res = scrape::scrape_game(&client, &db.0, &creds, &media_root, &game, &mut scrape::Spent::default()).await?;
    Ok(match res.source {
        Some(src) => format!("Matched via {src}"),
        None => res
            .halt
            .unwrap_or_else(|| "No match found".to_string()),
    })
}

/// Run a command's work off the main thread.
///
/// Tauri runs a command that is not `async` on the main thread, and the whole
/// window stands still until it returns. Anything that reads a ROM, a DAT or
/// a folder of saves, or waits on a drive, goes through here.
async fn off_main<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| AppError::Other(format!("background work failed: {e}")))?
}

// -------------------------------------------------------------- launch

/// Off the main thread: a game kept in an archive is unpacked before it
/// starts, which for a disc or a console package is minutes of work, and a
/// command run on the main thread holds the whole window still until it ends.
#[tauri::command]
pub async fn launch_game(app: AppHandle, id: i64) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        let game = {
            let conn = db.0.lock().unwrap();
            db::get_game(&conn, id)?.ok_or_else(|| AppError::Other("Game not found".into()))?
        };
        let cache_root = app.state::<AppState>().media_root.clone();
        launch::launch(&app, &db.0, &game, &cache_root)
    })
    .await
    .map_err(|e| AppError::Other(format!("launch failed: {e}")))?
}

/// Show what would run, so a misconfigured emulator is obvious before launch.
#[tauri::command]
pub fn preview_launch(app: AppHandle, db: State<Db>, id: i64) -> Result<String> {
    let conn = db.0.lock().unwrap();
    let game = db::get_game(&conn, id)?.ok_or_else(|| AppError::Other("Game not found".into()))?;

    // Show the path the emulator will really be given, without unpacking
    // anything just to draw a line of text.
    let cache_root = app.state::<AppState>().media_root.clone();
    let accepts = db::get_setting(&conn, "retroarch_path")?
        .map(|p| crate::detect::clean_path(&p))
        .filter(|p| !p.is_empty())
        .and_then(|p| {
            let core = launch::resolve_for_game(&conn, &game).core?;
            launch::core_extensions(std::path::Path::new(&p), &core)
        });
    let rom = launch::preview_rom_path(&game, &cache_root, accepts.as_deref());

    let (exe, args) = launch::build_command(&conn, &game, &rom)?;
    let quoted: Vec<String> = args
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect();
    Ok(format!("\"{exe}\" {}", quoted.join(" ")))
}

#[tauri::command]
pub fn reveal_game(db: State<Db>, id: i64) -> Result<()> {
    let conn = db.0.lock().unwrap();
    let game = db::get_game(&conn, id)?.ok_or_else(|| AppError::Other("Game not found".into()))?;
    let path = std::path::PathBuf::from(&game.path);
    let dir = path.parent().unwrap_or(&path);
    open_in_file_manager(dir)
}

fn open_in_file_manager(dir: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";

    std::process::Command::new(program)
        .arg(dir)
        .spawn()
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

// ------------------------------------------------------------ settings

#[tauri::command]
pub fn get_settings(db: State<Db>, state: State<AppState>) -> Result<HashMap<String, String>> {
    let conn = db.0.lock().unwrap();
    crate::secrets::all_settings(&conn, &state.vault)
}

/// Passwords and keys go to the system's credential store; everything else
/// to the library database.
#[tauri::command]
pub fn save_settings(
    db: State<Db>,
    state: State<AppState>,
    values: HashMap<String, String>,
) -> Result<()> {
    let conn = db.0.lock().unwrap();
    for (k, v) in values {
        // Paths arrive with the quotes Explorer's "Copy as path" wraps them in.
        let value = if k.ends_with("_path") || k.ends_with("_dir") {
            crate::detect::clean_path(&v)
        } else {
            v
        };
        crate::secrets::save(&conn, &state.vault, &k, &value)?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_emulators(db: State<Db>) -> Result<Vec<EmulatorConfig>> {
    let conn = db.0.lock().unwrap();
    db::list_emulators(&conn)
}

#[tauri::command]
pub fn save_emulator(db: State<Db>, config: EmulatorConfig) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::set_emulator(&conn, &config)
}

/// The emulator that would actually be used for a platform right now.
#[tauri::command]
pub fn effective_emulator(db: State<Db>, platform: String) -> Result<EmulatorConfig> {
    let conn = db.0.lock().unwrap();
    Ok(launch::resolve_config(&conn, &platform))
}

/// The discs a multi-disc entry stands for, in playing order. Empty for an
/// ordinary game, which is nearly all of them.
#[tauri::command]
pub fn disc_members(db: State<Db>, id: i64) -> Result<Vec<Game>> {
    let conn = db.0.lock().unwrap();
    db::disc_members(&conn, id)
}

/// One game's own emulator, or `None` when it just uses its system's.
#[tauri::command]
pub fn game_emulator(db: State<Db>, id: i64) -> Result<Option<EmulatorConfig>> {
    let conn = db.0.lock().unwrap();
    db::game_emulator(&conn, id)
}

/// Give one game its own emulator, or pass `null` to hand it back to its
/// system's. Everything that runs or previews a launch reads this, and so do
/// cheats and saves, which are filed under whichever core actually runs.
#[tauri::command]
pub fn save_game_emulator(db: State<Db>, id: i64, config: Option<EmulatorConfig>) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::set_game_emulator(&conn, id, config.as_ref())
}

// --------------------------------------------------------------- hacks

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "hack".into()
    } else {
        trimmed
    }
}

/// Read a patch and report whether it matches the chosen base ROM, without
/// writing anything.
#[tauri::command]
pub fn inspect_patch(
    db: State<Db>,
    base_game_id: i64,
    patch_path: String,
) -> Result<HackPreview> {
    let bytes = std::fs::read(&patch_path)?;
    let info = crate::patch::inspect(&bytes)?;

    let base = {
        let conn = db.0.lock().unwrap();
        db::get_game(&conn, base_game_id)?
            .ok_or_else(|| AppError::Other("Base game not found".into()))?
    };

    let suggested_title = std::path::Path::new(&patch_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("ROM hack")
        .to_string();

    let expected = info.source_crc.map(|c| format!("{c:08X}"));
    let base_crc = base.crc32.clone().map(|c| c.to_uppercase());

    let (compatible, message) = match (&expected, &base_crc) {
        (Some(want), Some(have)) if want.eq_ignore_ascii_case(have) => (
            true,
            format!("Matches your copy of {} exactly.", base.title),
        ),
        (Some(want), Some(have)) => (
            false,
            format!(
                "This patch was built for a ROM with CRC32 {want}, but your {} is {have}. \
                 You most likely need a different revision or region.",
                base.title
            ),
        ),
        (Some(_), None) => (
            true,
            "Your ROM has not been hashed, so the match cannot be verified up front.".into(),
        ),
        (None, _) => (
            true,
            format!(
                "{} patches carry no checksum, so this cannot be verified until it is applied.",
                info.format.as_str()
            ),
        ),
    };

    Ok(HackPreview {
        format: info.format.as_str().to_string(),
        suggested_title,
        expected_crc: expected,
        base_crc,
        compatible,
        message,
    })
}

/// Apply a patch and add the result to the library as its own game.
/// The base ROM is read but never modified.
fn apply_patch_file(
    state: &AppState,
    db: &Db,
    base_game_id: i64,
    patch_path: &str,
    title: Option<String>,
) -> Result<i64> {
    let base = {
        let conn = db.0.lock().unwrap();
        db::get_game(&conn, base_game_id)?
            .ok_or_else(|| AppError::Other("Base game not found".into()))?
    };

    let (rom_bytes, rom_name) = crate::hashing::read_rom_bytes(std::path::Path::new(&base.path))?;
    let patch_bytes = std::fs::read(patch_path)?;
    let info = crate::patch::inspect(&patch_bytes)?;
    let patched = crate::patch::apply(&rom_bytes, &patch_bytes)?;

    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            std::path::Path::new(patch_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("ROM hack")
                .to_string()
        });

    let ext = std::path::Path::new(&rom_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("rom")
        .to_string();

    let dir = state.media_root.join("hacks").join(base_game_id.to_string());
    std::fs::create_dir_all(&dir)?;
    let dest = unique_hack_path(&dir, &title, &ext);
    let filename = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();

    // Written aside and put in place only once the library has the entry, so
    // a failure in between leaves neither a stray ROM nor an entry for one.
    let partial = dir.join(format!("{filename}.partial"));
    std::fs::write(&partial, &patched)?;
    let conn = db.0.lock().unwrap();
    let inserted = db::insert_hack(
        &conn,
        &dest.to_string_lossy(),
        &filename,
        &base.platform,
        &title,
        patched.len() as i64,
        Some(&format!("{:08x}", crate::patch::crc32(&patched))),
        base_game_id,
        patch_path,
        info.format.as_str(),
    );
    match inserted {
        Ok(id) => {
            std::fs::rename(&partial, &dest)?;
            Ok(id)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

/// A file name for a new hack that nothing in its folder has yet, and that
/// Windows will take: "Kaizo.sfc", then "Kaizo (2).sfc".
///
/// Two hacks of the same game given the same title used to share a file, so
/// the second patch quietly replaced the first one's ROM.
fn unique_hack_path(dir: &std::path::Path, title: &str, ext: &str) -> std::path::PathBuf {
    let mut stem = sanitize_filename(title);
    // Device names Windows reserves, whatever follows them.
    let upper = stem.to_ascii_uppercase();
    let device = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit());
    if device {
        stem.push('_');
    }
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(format!("{stem}.{ext}"))
            } else {
                dir.join(format!("{stem} ({n}).{ext}"))
            }
        })
        .find(|p| !p.exists())
        .expect("an unused name")
}

/// Apply a patch chosen from the file system.
#[tauri::command]
pub async fn add_hack(
    app: AppHandle,
    base_game_id: i64,
    patch_path: String,
    title: Option<String>,
) -> Result<i64> {
    off_main(move || {
        apply_patch_file(&app.state::<AppState>(), &app.state::<Db>(), base_game_id, &patch_path, title)
    })
    .await
}

/// Apply a patch already in the catalog.
#[tauri::command]
pub async fn apply_catalog_patch(
    app: AppHandle,
    base_game_id: i64,
    patch_id: i64,
    title: Option<String>,
) -> Result<i64> {
    off_main(move || {
        let db = app.state::<Db>();
        let entry = {
            let conn = db.0.lock().unwrap();
            db::get_patch(&conn, patch_id)?
                .ok_or_else(|| AppError::Other("Patch not found in the catalog".into()))?
        };
        let title = title.or(Some(entry.name.clone()));
        apply_patch_file(&app.state::<AppState>(), &db, base_game_id, &entry.path, title)
    })
    .await
}

// ------------------------------------------------------- patch catalog

/// Index a folder of patches, or extract and index the patches inside a .7z
/// bundle. Nothing else in the archive is written to disk.
#[tauri::command]
pub async fn import_patches(app: AppHandle, path: String) -> Result<String> {
    let handle = app.clone();
    let (imported, skipped) =
        tauri::async_runtime::spawn_blocking(move || -> Result<(usize, usize)> {
            let db_handle = handle.state::<Db>().0.clone();
            let root = handle.state::<AppState>().media_root.join("patches");
            std::fs::create_dir_all(&root)?;
            let emitter = handle.clone();
            let mut on_progress = move |p: ImportProgress| {
                let _ = emitter.emit("import-progress", p);
            };
            crate::hacks::import(
                &mut on_progress,
                &db_handle,
                &root,
                std::path::Path::new(&path),
            )
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;

    Ok(format!("Catalogued {imported} patches, skipped {skipped}"))
}

/// Catalogued patches built for exactly this ROM, matched on CRC32.
#[tauri::command]
pub fn patches_for_game(db: State<Db>, game_id: i64) -> Result<Vec<PatchEntry>> {
    let conn = db.0.lock().unwrap();
    let game = db::get_game(&conn, game_id)?
        .ok_or_else(|| AppError::Other("Game not found".into()))?;
    match game.crc32 {
        Some(crc) => db::patches_for_crc(&conn, &crc),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub fn list_patches(db: State<Db>, search: Option<String>) -> Result<Vec<PatchEntry>> {
    let conn = db.0.lock().unwrap();
    db::list_patches(&conn, search.as_deref(), 500)
}

#[tauri::command]
pub fn patch_catalog_size(db: State<Db>) -> Result<i64> {
    let conn = db.0.lock().unwrap();
    db::patch_catalog_size(&conn)
}

#[tauri::command]
pub fn clear_patch_catalog(db: State<Db>) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::clear_patch_catalog(&conn)
}

// ---------------------------------------------------------- diagnostics

/// Look for an installed RetroArch so the path does not have to be typed.
#[tauri::command]
pub fn detect_retroarch() -> Option<crate::detect::DetectedEmulator> {
    crate::detect::find_retroarch()
}

/// The command to run games with a standalone emulator, given its
/// executable. Filled in when one is browsed to, so RPCS3 gets its
/// `--no-gui` without anyone having to know it wants one.
#[tauri::command]
pub fn suggest_emulator_command(exe: String) -> crate::emulators::Suggestion {
    crate::emulators::suggest(&exe)
}

/// Look for standalone emulators in the places people keep them. Off the main
/// thread, since it lists a good few folders.
#[tauri::command]
pub async fn detect_emulators() -> Result<Vec<crate::emulators::Found>> {
    tauri::async_runtime::spawn_blocking(crate::emulators::detect)
        .await
        .map_err(|e| AppError::Other(format!("emulator search failed: {e}")))
}

/// Ask both metadata providers whether the stored credentials work.
#[tauri::command]
pub async fn test_credentials(app: AppHandle) -> Result<Vec<scrape::ProviderStatus>> {
    let client = app.state::<AppState>().client.clone();
    let creds = {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        scrape::Credentials::from_settings(&crate::secrets::all_settings(
            &conn,
            &app.state::<AppState>().vault,
        )?)
    };
    Ok(scrape::check_credentials(&client, &creds).await)
}

// -------------------------------------------------------------- cheats

/// The ROM's own name, which is how both the cheat and artwork databases
/// index it — not the cleaned-up display title.
pub(crate) fn rom_name_of(game: &Game) -> String {
    let raw = game.inner_name.as_deref().unwrap_or(&game.filename);
    std::path::Path::new(raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(raw)
        .to_string()
}

/// Look this game up in libretro's cheat database and remember what it finds.
#[tauri::command]
pub async fn find_cheats(app: AppHandle, game_id: i64) -> Result<Vec<Cheat>> {
    let client = app.state::<AppState>().client.clone();
    let db = app.state::<Db>();

    let game = {
        let conn = db.0.lock().unwrap();
        db::get_game(&conn, game_id)?
            .ok_or_else(|| AppError::Other("Game not found".into()))?
    };

    let found = crate::cheats::fetch(&client, &game.platform, &rom_name_of(&game)).await?;

    let conn = db.0.lock().unwrap();
    if !found.is_empty() {
        db::replace_cheats(&conn, game_id, &found)?;
    }
    db::list_cheats(&conn, game_id)
}

#[tauri::command]
pub fn list_cheats(db: State<Db>, game_id: i64) -> Result<Vec<Cheat>> {
    let conn = db.0.lock().unwrap();
    db::list_cheats(&conn, game_id)
}

#[tauri::command]
pub fn set_cheat(db: State<Db>, game_id: i64, index: i64, enabled: bool) -> Result<()> {
    let conn = db.0.lock().unwrap();
    db::set_cheat_enabled(&conn, game_id, index, enabled)
}

/// Switch every cheat for a game on or off in one go.
#[tauri::command]
pub fn set_all_cheats(db: State<Db>, game_id: i64, enabled: bool) -> Result<Vec<Cheat>> {
    let conn = db.0.lock().unwrap();
    db::set_all_cheats_enabled(&conn, game_id, enabled)?;
    db::list_cheats(&conn, game_id)
}

/// Write the switched-on cheats to RetroArch by hand.
///
/// Playing a game does this on its own; this is for prepping a ROM you intend
/// to launch from RetroArch directly.
#[tauri::command]
pub fn save_cheats(db: State<Db>, game_id: i64) -> Result<String> {
    let conn = db.0.lock().unwrap();
    let game = db::get_game(&conn, game_id)?
        .ok_or_else(|| AppError::Other("Game not found".into()))?;

    crate::cheats::sync_to_retroarch(&conn, &game)?.ok_or_else(|| {
        AppError::Other("No cheats found for this game yet - press Find cheats first".into())
    })
}

/// What RetroArch's own config says, so the UI can explain what will happen.
#[tauri::command]
pub fn retroarch_cheat_status(db: State<Db>) -> Result<crate::cheats::RetroArchCheats> {
    let conn = db.0.lock().unwrap();
    let retroarch = db::get_setting(&conn, "retroarch_path")?
        .map(|p| crate::detect::clean_path(&p))
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::Other("Set the RetroArch path in Settings first".into()))?;
    Ok(crate::cheats::read_retroarch_config(std::path::Path::new(&retroarch)))
}

/// Turn on RetroArch's "Auto-Apply Cheats During Game Load".
#[tauri::command]
pub fn enable_auto_apply_cheats(db: State<Db>) -> Result<String> {
    let conn = db.0.lock().unwrap();
    let retroarch = db::get_setting(&conn, "retroarch_path")?
        .map(|p| crate::detect::clean_path(&p))
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::Other("Set the RetroArch path in Settings first".into()))?;
    crate::cheats::enable_auto_apply(std::path::Path::new(&retroarch))
}

/// Files and folders dropped onto the window.
///
/// Folders become library folders, so a full scan follows to pick up what is
/// inside them; dropped files are already indexed by the time that runs.
#[tauri::command]
pub async fn add_dropped(app: AppHandle, paths: Vec<String>) -> Result<scan::DropTally> {
    let handle = app.clone();
    let tally = tauri::async_runtime::spawn_blocking(move || -> Result<scan::DropTally> {
        let db_handle = handle.state::<Db>().0.clone();
        let paths: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();
        let tally = scan::add_paths(&db_handle, &paths)?;
        if tally.folders > 0 {
            scan::scan_all(&handle, &db_handle)?;
        }
        Ok(tally)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    Ok(tally)
}

// --------------------------------------------------------------- DATs

/// Import a No-Intro or Redump DAT so dumps can be checked against it.
#[tauri::command]
pub async fn import_dat(app: AppHandle, path: String) -> Result<crate::dats::ImportSummary> {
    off_main(move || {
        let (name, roms) = crate::dats::parse(std::path::Path::new(&path))?;
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        crate::dats::store(&conn, &name, &roms)
    })
    .await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatStatus {
    pub total: i64,
    /// Each loaded DAT and how many dumps it brought.
    pub loaded: Vec<(String, i64)>,
    /// How the library scores against them, by verdict.
    pub library: std::collections::HashMap<String, usize>,
}

#[tauri::command]
pub async fn dat_status(app: AppHandle) -> Result<DatStatus> {
    off_main(move || {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        Ok(DatStatus {
            total: crate::dats::count(&conn)?,
            loaded: crate::dats::loaded(&conn)?,
            library: crate::dats::verify_library(&conn)?,
        })
    })
    .await
}

#[tauri::command]
pub fn clear_dats(db: State<Db>) -> Result<()> {
    let conn = db.0.lock().unwrap();
    crate::dats::clear(&conn)
}

/// Work out the checksums of one file a scan skipped as too large, then say
/// what the catalogue makes of them.
///
/// A scan leaves anything past the hashing limit alone, because reading every
/// disc image in a library would turn a scan into an afternoon. One disc read
/// once, when asked, is a short wait instead - and gives DAT verification
/// and ScreenScraper's checksum lookup something to work with.
#[tauri::command]
pub async fn checksum_game(app: AppHandle, id: i64) -> Result<crate::dats::Verdict> {
    let game = {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        db::get_game(&conn, id)?.ok_or_else(|| AppError::Other("Game not found".into()))?
    };
    let path = std::path::PathBuf::from(&game.path);
    if crate::consoles::for_boot_file(&path).is_some() {
        return Err(AppError::Other(
            "This game is a whole folder rather than one file, so there is no single checksum to take."
                .into(),
        ));
    }
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("m3u")) {
        return Err(AppError::Other(
            "This is a set of discs. Each disc is checked on its own.".into(),
        ));
    }

    let hashes = tauri::async_runtime::spawn_blocking(move || crate::hashing::hash_rom_whole(&path))
        .await
        .map_err(|e| AppError::Other(format!("checksum failed: {e}")))??
        .ok_or_else(|| AppError::Other("There was nothing in the archive to check.".into()))?;

    let db = app.state::<Db>();
    let conn = db.0.lock().unwrap();
    db::set_hashes(
        &conn,
        id,
        &hashes.crc32,
        &hashes.md5,
        &hashes.sha1,
        hashes.inner_name.as_deref(),
    )?;
    crate::dats::verify(
        &conn,
        Some(&hashes.crc32),
        Some(&hashes.md5),
        Some(&hashes.sha1),
    )
}

/// What the catalogue says about one game.
#[tauri::command]
pub fn verify_game(db: State<Db>, id: i64) -> Result<crate::dats::Verdict> {
    let conn = db.0.lock().unwrap();
    let game = db::get_game(&conn, id)?
        .ok_or_else(|| AppError::Other("Game not found".into()))?;
    crate::dats::verify(
        &conn,
        game.crc32.as_deref(),
        game.md5.as_deref(),
        game.sha1.as_deref(),
    )
}

// ------------------------------------------------------ unpacked cache

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheUsage {
    pub bytes: u64,
    pub entries: usize,
    pub limit_bytes: u64,
}

/// What the unpacked-ROM cache is costing right now.
#[tauri::command]
pub async fn cache_usage(app: AppHandle) -> Result<CacheUsage> {
    off_main(move || {
        let root = app.state::<AppState>().media_root.clone();
        let (bytes, entries) = launch::cache_usage(&root);
        let limit = {
            let db = app.state::<Db>();
            let conn = db.0.lock().unwrap();
            launch::cache_limit(&conn)
        };
        Ok(CacheUsage {
            bytes,
            entries,
            limit_bytes: limit,
        })
    })
    .await
}

/// Swap a game's archive for what is inside it, deleting the archive.
///
/// Off the main thread and with the library unlocked while it unpacks, which
/// for a console package is gigabytes; the library is only touched at the end,
/// to point the entry at the unpacked game.
#[tauri::command]
pub async fn unpack_in_place(app: AppHandle, id: i64) -> Result<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        let game = {
            let conn = db.0.lock().unwrap();
            db::get_game(&conn, id)?.ok_or_else(|| AppError::Other("Game not found".into()))?
        };
        let cache_root = app.state::<AppState>().media_root.clone();

        let mut last = std::time::Instant::now();
        let mut report = |done: u64, total: u64| {
            if done < total && last.elapsed() < std::time::Duration::from_millis(200) {
                return;
            }
            last = std::time::Instant::now();
            let _ = app.emit(
                "unpack-progress",
                serde_json::json!({
                    "gameId": game.id, "title": game.title,
                    "done": done, "total": total, "finished": false,
                }),
            );
        };
        let result = launch::unpack_in_place(&game, &cache_root, &mut report);
        let _ = app.emit(
            "unpack-progress",
            serde_json::json!({ "gameId": game.id, "title": game.title, "finished": true }),
        );
        let unpacked = result?;

        let conn = db.0.lock().unwrap();
        conn.execute(
            "UPDATE games SET path = ?2, filename = ?3, inner_name = NULL, size = ?4
             WHERE id = ?1",
            rusqlite::params![game.id, unpacked.path, unpacked.filename, unpacked.size as i64],
        )?;
        Ok(unpacked.message)
    })
    .await
    .map_err(|e| AppError::Other(format!("unpack failed: {e}")))?
}

/// Empty it. Nothing on disk outside the cache is touched, and every entry is
/// rebuilt on the next launch of that game.
#[tauri::command]
pub async fn clear_cache(app: AppHandle) -> Result<u64> {
    off_main(move || launch::clear_cache(&app.state::<AppState>().media_root)).await
}

// ------------------------------------------------------- hack bundles

const HACK_ARCHIVE: &str = "rom-hack-patch-archive";

/// The downloadable patch bundles in the Internet Archive item.
#[tauri::command]
pub async fn list_hack_bundles(app: AppHandle) -> Result<Vec<HackBundle>> {
    let client = app.state::<AppState>().client.clone();
    let url = format!("https://archive.org/metadata/{HACK_ARCHIVE}/files");

    let json: serde_json::Value = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Could not reach the Internet Archive: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Other(format!("Unreadable file list: {e}")))?;

    let files = json
        .get("result")
        .and_then(|r| r.as_array())
        .or_else(|| json.as_array())
        .cloned()
        .unwrap_or_default();

    let mut out: Vec<HackBundle> = files
        .iter()
        .filter_map(|f| {
            let name = f.get("name")?.as_str()?.to_string();
            if !name.to_ascii_lowercase().ends_with(".7z") {
                return None;
            }
            let size = f
                .get("size")
                .and_then(|s| s.as_str())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            Some(HackBundle {
                url: format!(
                    "https://archive.org/download/{HACK_ARCHIVE}/{}",
                    urlencoding::encode(&name)
                ),
                name,
                size,
            })
        })
        .collect();

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// Bundles being downloaded, so the same one is not fetched twice at once
/// into the same file.
static DOWNLOADING: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(Default::default);

struct Downloading(String);

impl Downloading {
    fn claim(name: &str) -> Result<Self> {
        if !DOWNLOADING.lock().unwrap().insert(name.to_string()) {
            return Err(AppError::Other(format!("{name} is already downloading.")));
        }
        Ok(Downloading(name.to_string()))
    }
}

impl Drop for Downloading {
    fn drop(&mut self) {
        DOWNLOADING.lock().unwrap().remove(&self.0);
    }
}

/// Download one bundle and index every patch inside it. These run to about a
/// gigabyte, so progress is reported as it streams.
#[tauri::command]
pub async fn download_hack_bundle(app: AppHandle, bundle: HackBundle) -> Result<String> {
    use futures_util::StreamExt;
    use std::io::Write;

    // Only the archive this window lists bundles from. The URL comes from the
    // page, and a command that downloads whatever it is handed is one any
    // script in the page could point anywhere.
    let allowed = format!("https://archive.org/download/{HACK_ARCHIVE}/");
    if !bundle.url.starts_with(&allowed) {
        return Err(AppError::Other(
            "Only bundles from the ROM Hack Patch Archive can be downloaded here.".into(),
        ));
    }
    let _downloading = Downloading::claim(&bundle.name)?;

    let media_root = app.state::<AppState>().media_root.clone();
    let dir = media_root.join("bundles");
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(crate::hacks::safe_file_name(&bundle.name));
    let partial = dir.join(format!("{}.part", crate::hacks::safe_file_name(&bundle.name)));

    // Not the shared client: its thirty-second limit covers the whole
    // request, body and all, and ended a gigabyte download on any connection
    // slower than about thirty megabytes a second. This one only gives up
    // when the connection goes quiet.
    let client = reqwest::Client::builder()
        .user_agent(concat!("arcadex/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::Other(format!("Download failed: {e}")))?;

    let resp = client
        .get(&bundle.url)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "Download failed with {}",
            resp.status()
        )));
    }

    let total = resp.content_length().unwrap_or(bundle.size.max(0) as u64);
    // Downloaded beside its name and renamed once whole, so an interrupted
    // download is never taken for a bundle.
    let mut file = std::fs::File::create(&partial)?;
    let mut stream = resp.bytes_stream();
    let mut done: u64 = 0;
    let mut last_pct = u64::MAX;

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                drop(file);
                let _ = std::fs::remove_file(&partial);
                return Err(AppError::Other(format!("Download interrupted: {e}")));
            }
        };
        file.write_all(&chunk)?;
        done += chunk.len() as u64;

        let pct = if total > 0 { done * 100 / total } else { 0 };
        if pct != last_pct {
            last_pct = pct;
            let _ = app.emit(
                "import-progress",
                ImportProgress {
                    scanned: 0,
                    imported: 0,
                    skipped: 0,
                    message: format!(
                        "Downloading {} — {pct}% of {:.2} GB",
                        bundle.name,
                        total as f64 / 1e9
                    ),
                    done: false,
                },
            );
        }
    }
    file.flush()?;
    drop(file);
    std::fs::rename(&partial, &dest)?;

    // Hand the finished archive to the importer already built for local
    // files. The patches come out into the catalog's own folder; the bundle,
    // a gigabyte of everything else, is deleted afterwards rather than left
    // to pile up.
    let handle = app.clone();
    let bundle_file = dest.clone();
    let imported =
        tauri::async_runtime::spawn_blocking(move || -> Result<(usize, usize)> {
            let db_handle = handle.state::<Db>().0.clone();
            let root = handle.state::<AppState>().media_root.join("patches");
            std::fs::create_dir_all(&root)?;
            let emitter = handle.clone();
            let mut on_progress = move |p: ImportProgress| {
                let _ = emitter.emit("import-progress", p);
            };
            crate::hacks::import(&mut on_progress, &db_handle, &root, &bundle_file)
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()));
    let _ = std::fs::remove_file(&dest);
    let (imported, skipped) = imported??;

    Ok(format!(
        "{} — catalogued {imported} patches, skipped {skipped}",
        bundle.name
    ))
}

// --------------------------------------------------------------- saves

pub(crate) fn retroarch_exe(conn: &rusqlite::Connection) -> Result<std::path::PathBuf> {
    let path = db::get_setting(conn, "retroarch_path")?
        .map(|p| crate::detect::clean_path(&p))
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::Other("Set the RetroArch path in Settings first".into()))?;
    Ok(std::path::PathBuf::from(path))
}

/// One game's saves and states, and its title. The library is only held
/// while the game is looked up; the folders are listed after.
fn saves_of(app: &AppHandle, game_id: i64) -> Result<(Vec<SaveEntry>, String)> {
    let (exe, game, folder) = {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        let game = db::get_game(&conn, game_id)?
            .ok_or_else(|| AppError::Other("Game not found".into()))?;
        let exe = retroarch_exe(&conn)?;
        let core = launch::resolve_for_game(&conn, &game).core.unwrap_or_default();
        (exe, game, crate::cheats::core_folder_name(&core))
    };
    let found = crate::saves::find(&exe, std::path::Path::new(&game.path), &rom_name_of(&game), &folder);
    Ok((found, game.title))
}

/// The save files and save states belonging to one game.
#[tauri::command]
pub async fn list_saves(app: AppHandle, game_id: i64) -> Result<Vec<SaveEntry>> {
    off_main(move || Ok(saves_of(&app, game_id)?.0)).await
}

/// Copy this game's saves somewhere safe before you overwrite them.
#[tauri::command]
pub async fn back_up_saves(app: AppHandle, game_id: i64) -> Result<String> {
    off_main(move || {
        let (found, title) = saves_of(&app, game_id)?;
        if found.is_empty() {
            return Err(AppError::Other("Nothing to back up yet".into()));
        }
        let dest = crate::saves::back_up(&found, &app.state::<AppState>().media_root, &title)?;
        Ok(format!(
            "Copied {} file{} to {}",
            found.len(),
            if found.len() == 1 { "" } else { "s" },
            dest.to_string_lossy()
        ))
    })
    .await
}

/// Delete one of a game's save states. Battery saves are refused — losing
/// real progress to a stray click is not worth the convenience.
///
/// Only a state this game's own listing found can go. The command used to
/// delete any path whose name had `.state` in it, wherever it was.
#[tauri::command]
pub async fn delete_save_state(app: AppHandle, game_id: i64, path: String) -> Result<()> {
    off_main(move || {
        let (found, _) = saves_of(&app, game_id)?;
        let entry = found.iter().find(|e| e.path == path).ok_or_else(|| {
            AppError::Other("That is not one of this game's save states.".into())
        })?;
        if entry.kind != "state" {
            return Err(AppError::Other(
                "Only save states can be deleted here — battery saves hold your actual progress"
                    .into(),
            ));
        }
        std::fs::remove_file(&entry.path)?;
        Ok(())
    })
    .await
}

// ------------------------------------------------------------ insights

/// Everything the stats view shows, in one round trip.
#[tauri::command]
pub fn library_insights(db: State<Db>) -> Result<LibraryInsights> {
    let conn = db.0.lock().unwrap();
    let stats = db::stats(&conn)?;
    Ok(LibraryInsights {
        total_games: stats.total_games,
        games_played: conn.query_row(
            "SELECT COUNT(*) FROM games WHERE play_seconds > 0",
            [],
            |r| r.get(0),
        )?,
        total_play_seconds: stats.total_play_seconds,
        session_count: db::session_count(&conn)?,
        longest_session: db::longest_session(&conn)?,
        recent: db::recently_played(&conn, 6)?,
        most_played: db::most_played(&conn, 6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second hack with the same title gets a name of its own instead of
    /// writing over the first one's ROM, and Windows' device names are kept
    /// clear of.
    #[test]
    fn a_new_hack_never_takes_another_hacks_file() {
        let dir = std::env::temp_dir().join(format!("arcadex-hackname-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let first = unique_hack_path(&dir, "Kaizo", "sfc");
        assert_eq!(first, dir.join("Kaizo.sfc"));
        std::fs::write(&first, b"one").unwrap();
        assert_eq!(unique_hack_path(&dir, "Kaizo", "sfc"), dir.join("Kaizo (2).sfc"));

        assert_eq!(unique_hack_path(&dir, "con", "nes"), dir.join("con_.nes"));
        assert_eq!(unique_hack_path(&dir, "COM1", "nes"), dir.join("COM1_.nes"));
        assert_eq!(unique_hack_path(&dir, "Console", "nes"), dir.join("Console.nes"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
