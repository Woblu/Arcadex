mod cheats;
mod commands;
mod consoles;
mod dats;
mod db;
mod discs;
mod detect;
mod emulators;
mod error;
mod hacks;
mod hashing;
mod launch;
mod media;
mod models;
mod patch;
mod platforms;
mod romcheck;
mod saves;
mod scan;
mod secrets;
mod signature;
mod scrape;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::{Manager, State};

pub struct AppState {
    pub client: reqwest::Client,
    pub media_root: PathBuf,
    pub scrape_running: AtomicBool,
    pub scrape_cancel: AtomicBool,
    /// Where passwords and keys are kept, instead of the library database.
    pub vault: secrets::Keychain,
}

/// The content type of an image the media protocol is allowed to hand the
/// webview, or `None` for anything else.
///
/// Two things qualify, and both must be images. Artwork in the `media`
/// folder, which is everything Arcadex downloaded or was given. And the
/// thumbnail RetroArch writes beside a save state, `<rom>.state1.png` next to
/// `<rom>.state1`, which lives in RetroArch's own folders.
///
/// The app data folder as a whole does not qualify. It holds the library
/// database, and a protocol that would read that out to any page in the
/// webview is a way for one bad description to leak everything in it.
fn servable(path: &std::path::Path, data_dir: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => return None,
    };
    let real = path.canonicalize().ok()?;

    let in_media = data_dir
        .join("media")
        .canonicalize()
        .map(|media| real.starts_with(media))
        .unwrap_or(false);

    // `.state`, `.state3`, `.state.auto`: whatever the slot, the name of the
    // state itself carries `.state`, and it has to be really there.
    let state = real.with_extension("");
    let state_thumbnail = ext == "png"
        && state
            .file_name()
            .and_then(|n| n.to_str())
            .map_or(false, |n| n.to_ascii_lowercase().contains(".state"))
        && state.is_file();

    (in_media || state_thumbnail).then_some(mime)
}

/// Turn a cached artwork path into a URL the webview can load.
/// The frontend calls `convertFileSrc(path, 'media')` to build the other half.
fn media_protocol_response(
    media_root: &std::path::Path,
    uri_path: &str,
) -> tauri::http::Response<Vec<u8>> {
    let not_found = || {
        tauri::http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap()
    };

    let decoded = match urlencoding::decode(uri_path.trim_start_matches('/')) {
        Ok(d) => d.into_owned(),
        Err(_) => return not_found(),
    };

    let path = PathBuf::from(decoded);
    let Some(mime) = servable(&path, media_root) else {
        return not_found();
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return not_found(),
    };

    tauri::http::Response::builder()
        .header("Content-Type", mime)
        .header("Cache-Control", "max-age=86400")
        .body(bytes)
        .unwrap()
}

#[tauri::command]
fn app_paths(state: State<AppState>) -> serde_json::Value {
    serde_json::json!({
        "mediaRoot": state.media_root.to_string_lossy(),
    })
}

/// Bundle identifiers this app has shipped under before, newest first.
///
/// The app data directory is named after the identifier, so every rename
/// moves it, and everything anyone has — the library database, their playtime,
/// the artwork cache, imported patches and the generated disc playlists —
/// lives in that folder. A launch after a rename has to go and find it.
///
/// The app was Playdex, became Romcade in 0.9.0, went back to Playdex in
/// 0.11.0 and became Arcadex in 0.16.0, so a library can be sitting under
/// either old name depending on which versions a person happened to run.
/// Newest first matters: someone who ran every version has a stale Romcade
/// library as well as their real Playdex one, and the first library found is
/// the one adopted. Add to the front of this list on the next rename rather
/// than replacing it, because the old directories do not disappear.
const LEGACY_IDENTIFIERS: &[&str] = &["com.boazv.playdex", "com.boazv.romcade"];

/// Whether a directory actually holds a library, as opposed to merely
/// existing.
///
/// This distinction is the whole point. An empty data directory gets created
/// by any build that starts even once, so "the destination exists" says
/// nothing about whether there is anything in it.
fn has_library(dir: &std::path::Path) -> bool {
    dir.join("library.db").is_file()
}

/// Moves the *contents* of `old` into `new`, if and only if `old` holds a
/// library and `new` does not.
///
/// Contents rather than the directory itself, and keyed on the library file
/// rather than on the directory existing, because of how this actually fails:
/// run an older build once after updating and it recreates its own empty data
/// directory. A version of this that renamed the folder and refused whenever
/// the destination existed would then find that empty directory in the way,
/// decline, and quietly leave the real library stranded under the old name —
/// which is the exact failure it was written to prevent.
///
/// Nothing is ever overwritten: an entry already present at the destination is
/// left alone and the old copy stays where it is.
///
/// Split out from the identifier lookup so it can be tested without standing
/// up a Tauri app handle.
fn adopt_data_dir(old: &std::path::Path, new: &std::path::Path) -> std::io::Result<bool> {
    if !has_library(old) || has_library(new) {
        return Ok(false);
    }
    std::fs::create_dir_all(new)?;
    for entry in std::fs::read_dir(old)? {
        let entry = entry?;
        let dest = new.join(entry.file_name());
        if dest.exists() {
            continue;
        }
        std::fs::rename(entry.path(), &dest)?;
    }
    Ok(true)
}

/// Carries a library across from whichever name it was last saved under.
///
/// A failure here is reported and then dropped. Being unable to move the old
/// folder is a bad first launch; refusing to start at all is a worse one, and
/// the old directory is left untouched either way, so nothing is lost that a
/// later attempt could not still recover.
fn migrate_legacy_data_dir(new_dir: &std::path::Path) {
    let Some(parent) = new_dir.parent() else {
        return;
    };
    for legacy in LEGACY_IDENTIFIERS {
        let old_dir = parent.join(legacy);
        if old_dir == new_dir {
            continue;
        }
        match adopt_data_dir(&old_dir, new_dir) {
            Ok(true) => eprintln!(
                "carried the library over from {} to {}",
                old_dir.display(),
                new_dir.display()
            ),
            Ok(false) => {}
            Err(e) => eprintln!("could not carry the library across from {legacy}: {e}"),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol("media", |ctx, request| {
            let state = ctx.app_handle().state::<AppState>();
            media_protocol_response(&state.media_root, request.uri().path())
        })
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("no app data directory available");
            // Before anything creates the new directory, since the move only
            // happens when there is nothing at the destination.
            migrate_legacy_data_dir(&data_dir);
            std::fs::create_dir_all(&data_dir)?;

            let conn = db::open(&data_dir.join("library.db"))?;

            // Moving the folder is only half of it. Artwork and patched ROMs
            // are recorded as absolute paths into that folder, so they have to
            // be repointed as well or the library comes up with every cover
            // broken. This runs for anyone who took 0.9.0 before the paths
            // were fixed, not only at the moment the folder moves.
            if let Some(current) = data_dir.file_name().and_then(|name| name.to_str()) {
                for legacy in LEGACY_IDENTIFIERS {
                    match db::repoint_data_dir(&conn, legacy, current) {
                        Ok(0) => {}
                        Ok(n) => eprintln!("repointed {n} stored paths from {legacy}"),
                        Err(e) => eprintln!("could not repoint stored paths: {e}"),
                    }
                }
            }

            // Passwords an earlier version kept in the database go to the
            // system's credential store. Anything the store will not take
            // stays where it was, and is tried again next time.
            let vault = secrets::Keychain::new(&app.config().identifier);
            match secrets::migrate(&conn, &vault) {
                Ok(0) => {}
                Ok(n) => eprintln!("moved {n} saved passwords to the credential store"),
                Err(e) => eprintln!("could not move saved passwords: {e}"),
            }
            // And the ones already in the store, filed under an old name.
            if let Some(parent) = data_dir.parent() {
                for legacy in LEGACY_IDENTIFIERS {
                    let retire = !has_library(&parent.join(legacy));
                    let n = secrets::adopt(&secrets::Keychain::new(legacy), &vault, retire);
                    if n > 0 {
                        eprintln!("carried {n} saved passwords over from {legacy}");
                    }
                }
            }

            app.manage(db::Db(std::sync::Arc::new(Mutex::new(conn))));

            let media_root = data_dir.clone();
            std::fs::create_dir_all(media_root.join("media"))?;

            let client = reqwest::Client::builder()
                .user_agent(concat!("arcadex/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("could not build http client");

            app.manage(AppState {
                client,
                media_root,
                scrape_running: AtomicBool::new(false),
                scrape_cancel: AtomicBool::new(false),
                vault,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_paths,
            commands::add_dropped,
            commands::import_dat,
            commands::dat_status,
            commands::clear_dats,
            commands::verify_game,
            commands::cache_usage,
            commands::clear_cache,
            commands::unpack_in_place,
            commands::list_games,
            commands::get_game,
            commands::list_platforms,
            commands::known_platforms,
            commands::library_stats,
            commands::library_insights,
            commands::set_favorite,
            commands::set_hidden,
            commands::set_game_platform,
            commands::remove_game,
            commands::list_library_folders,
            commands::add_library_folder,
            commands::remove_library_folder,
            commands::pick_folder,
            commands::pick_file,
            commands::choose_custom_cover,
            commands::clear_custom_cover,
            commands::rename_game,
            commands::borrow_metadata,
            commands::scan_library,
            commands::clean_missing,
            commands::missing_report,
            commands::setup_status,
            commands::scrape_library,
            commands::scrape_one,
            commands::cancel_scrape,
            commands::launch_game,
            commands::preview_launch,
            commands::reveal_game,
            commands::get_settings,
            commands::save_settings,
            commands::list_emulators,
            commands::save_emulator,
            commands::effective_emulator,
            commands::disc_members,
            commands::game_emulator,
            commands::save_game_emulator,
            commands::inspect_patch,
            commands::add_hack,
            commands::apply_catalog_patch,
            commands::import_patches,
            commands::patches_for_game,
            commands::list_patches,
            commands::patch_catalog_size,
            commands::clear_patch_catalog,
            commands::find_cheats,
            commands::list_cheats,
            commands::set_cheat,
            commands::set_all_cheats,
            commands::save_cheats,
            commands::retroarch_cheat_status,
            commands::enable_auto_apply_cheats,
            commands::list_hack_bundles,
            commands::download_hack_bundle,
            commands::list_saves,
            commands::back_up_saves,
            commands::delete_save_state,
            commands::detect_retroarch,
            commands::suggest_emulator_command,
            commands::detect_emulators,
            commands::checksum_game,
            commands::test_credentials,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod media_tests {
    use super::servable;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-media-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn serves_artwork_and_nothing_else_in_the_data_folder() {
        let data = scratch("scope");
        let art = data.join("media").join("1").join("cover.webp");
        std::fs::create_dir_all(art.parent().unwrap()).unwrap();
        std::fs::write(&art, b"art").unwrap();
        assert_eq!(servable(&art, &data), Some("image/webp"));

        // The library itself sits one folder up, and is the thing worth
        // protecting: it holds every setting.
        let db = data.join("library.db");
        std::fs::write(&db, b"SQLite format 3").unwrap();
        assert_eq!(servable(&db, &data), None);

        // Renaming it to look like a picture does not get it out either.
        let disguised = data.join("library.png");
        std::fs::write(&disguised, b"SQLite format 3").unwrap();
        assert_eq!(servable(&disguised, &data), None);

        // Nor does climbing out of the media folder.
        let climb = data.join("media").join("..").join("library.db");
        assert_eq!(servable(&climb, &data), None);

        // And inside it, only images.
        let text = data.join("media").join("1").join("notes.txt");
        std::fs::write(&text, b"x").unwrap();
        assert_eq!(servable(&text, &data), None);

        let _ = std::fs::remove_dir_all(&data);
    }

    /// RetroArch writes a state's thumbnail beside it, in its own folders.
    #[test]
    fn serves_a_save_states_thumbnail_where_retroarch_left_it() {
        let data = scratch("states-data");
        let states = scratch("states");
        for name in ["Zelda.state1", "Zelda.state.auto"] {
            std::fs::write(states.join(name), b"state").unwrap();
            let shot = states.join(format!("{name}.png"));
            std::fs::write(&shot, b"png").unwrap();
            assert_eq!(servable(&shot, &data), Some("image/png"), "{name}");
        }

        // A picture with no state beside it is just a picture somewhere on
        // disk, and not ours to hand out.
        let orphan = states.join("Mario.state2.png");
        std::fs::write(&orphan, b"png").unwrap();
        assert_eq!(servable(&orphan, &data), None);
        let holiday = states.join("holiday.png");
        std::fs::write(&holiday, b"png").unwrap();
        assert_eq!(servable(&holiday, &data), None);

        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&states);
    }
}

#[cfg(test)]
mod migration_tests {
    use super::{adopt_data_dir, migrate_legacy_data_dir};

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-migrate-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn library_at(dir: &std::path::Path, marker: &[u8]) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("library.db"), marker).unwrap();
        std::fs::create_dir_all(dir.join("media")).unwrap();
        std::fs::write(dir.join("media").join("cover.jpg"), b"art").unwrap();
    }

    #[test]
    fn carries_a_library_across() {
        let base = scratch("move");
        let old = base.join("com.boazv.romcade");
        let new = base.join("com.boazv.arcadex");
        library_at(&old, b"library");

        assert!(adopt_data_dir(&old, &new).unwrap());
        assert_eq!(std::fs::read(new.join("library.db")).unwrap(), b"library");
        assert!(new.join("media").join("cover.jpg").exists());

        std::fs::remove_dir_all(&base).ok();
    }

    /// The one that matters. Running an older build even once recreates its
    /// own empty data directory, and a migration keyed on "does the
    /// destination exist" would decline here and strand the real library.
    #[test]
    fn adopts_into_a_destination_that_exists_but_is_empty() {
        let base = scratch("empty-dest");
        let old = base.join("com.boazv.romcade");
        let new = base.join("com.boazv.arcadex");
        library_at(&old, b"library");
        std::fs::create_dir_all(&new).unwrap();

        assert!(adopt_data_dir(&old, &new).unwrap());
        assert_eq!(std::fs::read(new.join("library.db")).unwrap(), b"library");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn refuses_to_overwrite_a_real_library() {
        let base = scratch("keep");
        let old = base.join("com.boazv.romcade");
        let new = base.join("com.boazv.arcadex");
        library_at(&old, b"old");
        library_at(&new, b"new");

        assert!(!adopt_data_dir(&old, &new).unwrap());
        assert_eq!(std::fs::read(new.join("library.db")).unwrap(), b"new");
        assert!(old.join("library.db").exists());

        std::fs::remove_dir_all(&base).ok();
    }

    /// Someone who ran every version has a stale library under Romcade as
    /// well as their real one under Playdex. The newer one is the one that
    /// comes across, and the stale one is left where it is.
    #[test]
    fn the_most_recent_name_wins_when_both_hold_a_library() {
        let base = scratch("newest");
        let romcade = base.join("com.boazv.romcade");
        let playdex = base.join("com.boazv.playdex");
        let arcadex = base.join("com.boazv.arcadex");
        library_at(&romcade, b"stale");
        library_at(&playdex, b"real");

        migrate_legacy_data_dir(&arcadex);

        assert_eq!(std::fs::read(arcadex.join("library.db")).unwrap(), b"real");
        assert!(!playdex.join("library.db").exists());
        assert_eq!(std::fs::read(romcade.join("library.db")).unwrap(), b"stale");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn does_nothing_on_a_fresh_install() {
        let base = scratch("fresh");
        let new = base.join("com.boazv.arcadex");
        assert!(!adopt_data_dir(&base.join("com.boazv.romcade"), &new).unwrap());
        assert!(!new.exists());
    }

    /// An empty directory under the old name is not a library, so it must not
    /// count as something to carry across.
    #[test]
    fn ignores_an_empty_legacy_directory() {
        let base = scratch("empty-src");
        let old = base.join("com.boazv.romcade");
        let new = base.join("com.boazv.arcadex");
        std::fs::create_dir_all(&old).unwrap();

        assert!(!adopt_data_dir(&old, &new).unwrap());
        assert!(!new.exists());

        std::fs::remove_dir_all(&base).ok();
    }
}
