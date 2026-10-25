//! Launching games and recording how long they were played.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::db;
use crate::error::{AppError, Result};
use crate::models::{EmulatorConfig, Game};
use crate::platforms;

/// Split a command template into argv, respecting double quotes.
fn tokenize(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in input.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Where RetroArch keeps its cores, if the user did not say explicitly.
fn default_cores_dir(retroarch_path: &Path) -> Option<PathBuf> {
    let dir = retroarch_path.parent()?;
    let candidate = dir.join("cores");
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

fn core_filename(core: &str) -> String {
    #[cfg(target_os = "windows")]
    let ext = "dll";
    #[cfg(target_os = "macos")]
    let ext = "dylib";
    #[cfg(all(unix, not(target_os = "macos")))]
    let ext = "so";

    if core.ends_with(ext) {
        core.to_string()
    } else {
        format!("{core}.{ext}")
    }
}

/// Whether a stored config says enough to run anything: a core for RetroArch,
/// a command for a standalone emulator. A half-filled one is not an answer,
/// and falling through to the next source beats failing at launch.
fn is_usable(cfg: &EmulatorConfig) -> bool {
    let has_core = cfg.core.as_deref().map(|c| !c.is_empty()).unwrap_or(false);
    let has_cmd = cfg
        .custom_command
        .as_deref()
        .map(|c| !c.is_empty())
        .unwrap_or(false);
    (cfg.mode == "custom" && has_cmd) || (cfg.mode == "retroarch" && has_core)
}

/// The emulator to run one game with: its own if it has been given one, then
/// its system's, then RetroArch with the system's preferred core.
///
/// Everything that launches or previews a launch goes through here, so a
/// per-game override applies to the command shown as well as the one run.
pub fn resolve_for_game(conn: &rusqlite::Connection, game: &Game) -> EmulatorConfig {
    if let Ok(Some(cfg)) = db::game_emulator(conn, game.id) {
        if is_usable(&cfg) {
            return cfg;
        }
    }
    resolve_config(conn, &game.platform)
}

/// The emulator configured for a platform, falling back to RetroArch with the
/// platform's preferred core.
pub fn resolve_config(conn: &rusqlite::Connection, platform: &str) -> EmulatorConfig {
    if let Ok(Some(cfg)) = db::get_emulator(conn, platform) {
        if is_usable(&cfg) {
            return cfg;
        }
    }
    EmulatorConfig {
        platform: platform.to_string(),
        mode: "retroarch".into(),
        core: platforms::by_slug(platform)
            .and_then(|p| p.cores.first())
            .map(|c| c.to_string()),
        custom_command: None,
    }
}

/// The extensions a core will actually accept, from the `.info` file
/// RetroArch ships beside it.
///
/// Worth reading rather than guessing: Dolphin, for instance, lists
/// `gcm|iso|wbfs|ciso|gcz|elf|dol|dff|tgc|wad|rvz|m3u|wia` and no archive
/// format at all, so handing it a `.7z` fails with nothing useful said.
pub fn core_extensions(ra_path: &Path, core: &str) -> Option<Vec<String>> {
    let stem = core.trim_end_matches(".dll").trim_end_matches(".so");
    let info = ra_path.parent()?.join("info").join(format!("{stem}.info"));
    let text = std::fs::read_to_string(info).ok()?;

    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "supported_extensions" {
            continue;
        }
        return Some(
            value
                .trim()
                .trim_matches('"')
                .split('|')
                .map(|e| e.trim().to_ascii_lowercase())
                .filter(|e| !e.is_empty())
                .collect(),
        );
    }
    None
}

/// Where an archived ROM gets unpacked to, keyed by the archive's own hash so
/// two dumps of the same name cannot collide.
fn cache_dir_for(game: &Game, cache_root: &Path) -> PathBuf {
    let key = game
        .crc32
        .clone()
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| game.id.to_string());
    cache_root.join("extracted").join(key)
}

/// Whether this ROM has to be unpacked before the emulator can open it.
///
/// `accepts` is the core's own extension list where we have one. Without it
/// the rule is just `.7z`: RetroArch unpacks a `.zip` itself for the cores
/// that want one, but a 7z is no use to anything.
/// Disc image formats. A core that accepts one of these loads whole discs,
/// and those cores are the ones RetroArch hands the file path to directly
/// rather than unpacking for. `bin` is deliberately absent: Mupen64Plus lists
/// it for N64 dumps and is nothing to do with discs.
const DISC_FORMATS: &[&str] = &[
    "iso", "cue", "chd", "gcm", "wbfs", "ciso", "gcz", "rvz", "wia", "gdi", "cdi", "pbp",
    "cso", "nrg", "m3u",
];

/// Whether this ROM has to be unpacked before the emulator can open it.
///
/// The obvious rule - "unpack when the archive's extension is not on the
/// core's list" - is wrong, and quietly unpacked every archived game in the
/// library. No core lists `zip` or `7z`, because RetroArch handles archives
/// itself in the frontend and never passes one to a core. So the core's list
/// says nothing at all about archives.
///
/// What it does say is whether the core loads discs, and disc cores are
/// precisely the ones RetroArch will not unpack for: Dolphin wants a path to
/// a real `.wbfs`, which is why a Wii game in a `.7z` did nothing at all.
/// Everything else gets its archive opened by RetroArch, so unpacking it here
/// would only duplicate the ROM on disk for no gain.
fn needs_extracting(game: &Game, accepts: Option<&[String]>) -> bool {
    let ext = Path::new(&game.path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if !crate::platforms::ARCHIVE_EXTS.contains(&ext.as_str()) {
        return false;
    }
    match accepts {
        Some(list) => list.iter().any(|e| DISC_FORMATS.contains(&e.as_str())),
        // No core to ask: a standalone emulator, or a RetroArch we cannot
        // find. Standalone emulators generally cannot read an archive at all,
        // and handing over a real file always works, so unpack. The cache is
        // capped, so guessing this way costs disk rather than correctness.
        None => true,
    }
}

/// Where the ROM *will* be once unpacked, without doing the unpacking — for
/// showing the command before anything runs.
pub fn preview_rom_path(game: &Game, cache_root: &Path, accepts: Option<&[String]>) -> String {
    if !needs_extracting(game, accepts) {
        return game.path.clone();
    }
    let inner = game
        .inner_name
        .as_deref()
        .and_then(|n| Path::new(n).file_name().and_then(|f| f.to_str()))
        .unwrap_or("<rom>");
    cache_dir_for(game, cache_root)
        .join(inner)
        .to_string_lossy()
        .to_string()
}

/// How much unpacked ROM to keep before throwing the oldest away, when nothing
/// else has been chosen. One Switch or Wii U game can be most of this on its
/// own, so the default is generous; the point of a limit is that a forgotten
/// folder in AppData cannot swallow a disk, not that it should be tight.
pub const DEFAULT_CACHE_LIMIT_BYTES: u64 = 64 * 1024 * 1024 * 1024;

/// The limit in force, which is a setting.
pub fn cache_limit(conn: &rusqlite::Connection) -> u64 {
    db::get_setting(conn, "cache_limit_gb")
        .ok()
        .flatten()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|gb| *gb > 0)
        .map(|gb| gb * 1024 * 1024 * 1024)
        .unwrap_or(DEFAULT_CACHE_LIMIT_BYTES)
}

/// Marks an entry as used just now. Ordinary file times move only when a file
/// is written, so a game launched nightly for a year would still look as old
/// as the day it was unpacked and be evicted first.
fn touch(dir: &Path) {
    let _ = std::fs::write(dir.join(".used"), b"");
}

/// The path to hand the emulator, unpacking the ROM first if it has to be.
pub fn playable_path(game: &Game, cache_root: &Path, accepts: Option<&[String]>) -> Result<String> {
    if !needs_extracting(game, accepts) {
        return Ok(game.path.clone());
    }
    let dir = cache_dir_for(game, cache_root);
    let file = crate::hashing::extract_rom(Path::new(&game.path), &dir)?;
    touch(&dir);
    Ok(file.to_string_lossy().to_string())
}

/// Replace a game's archive with the unpacked ROM inside it.
///
/// Keeping both is the cost of storing a disc game compressed: a 3.5 GB `.7z`
/// plus the 3.2 GB copy the emulator actually reads. This collapses that to
/// one file. The unpacked ROM is moved out of the cache and into the folder
/// the archive lived in, the library entry is pointed at it, and only then is
/// the archive deleted.
///
/// That order matters and is the whole design. The replacement is put in
/// place and checked before anything is removed, so an interruption at any
/// point leaves you with at least one working copy. Moving it out of the
/// cache also takes it out of reach of eviction, which would otherwise be
/// free to delete the only remaining copy of the game.
///
/// This deletes something of yours, which nothing else in Playdex does. It
/// only ever happens when asked for directly.
pub fn unpack_in_place(
    conn: &rusqlite::Connection,
    game: &Game,
    cache_root: &Path,
) -> Result<String> {
    let archive = PathBuf::from(&game.path);
    let ext = archive
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if !crate::platforms::ARCHIVE_EXTS.contains(&ext.as_str()) {
        return Err(AppError::Other(
            "This game is not in an archive, so there is nothing to unpack.".into(),
        ));
    }
    if !archive.exists() {
        return Err(AppError::Other(format!("Missing: {}", game.path)));
    }

    // Unpack now if it has not been already.
    let dir = cache_dir_for(game, cache_root);
    let unpacked = crate::hashing::extract_rom(&archive, &dir)?;
    let size = std::fs::metadata(&unpacked)?.len();

    let leaf = unpacked
        .file_name()
        .ok_or_else(|| AppError::Other("The unpacked ROM has no name".into()))?;
    let parent = archive
        .parent()
        .ok_or_else(|| AppError::Other("The archive has no folder".into()))?;
    let destination = parent.join(leaf);

    if destination.exists() {
        return Err(AppError::Other(format!(
            "{} is already there. Nothing was changed.",
            destination.display()
        )));
    }

    // Rename where we can; a cache on another drive needs a real copy.
    if std::fs::rename(&unpacked, &destination).is_err() {
        std::fs::copy(&unpacked, &destination)?;
    }

    // Refuse to go further unless the replacement really arrived intact.
    match std::fs::metadata(&destination) {
        Ok(m) if m.len() == size => {}
        _ => {
            let _ = std::fs::remove_file(&destination);
            return Err(AppError::Other(
                "The unpacked ROM did not copy across cleanly, so the archive was left alone."
                    .into(),
            ));
        }
    }

    // Only now is there a spare copy to lose.
    std::fs::remove_file(&archive)?;
    let _ = std::fs::remove_dir_all(&dir);

    let new_path = destination.to_string_lossy().to_string();
    let new_name = leaf.to_string_lossy().to_string();
    conn.execute(
        "UPDATE games SET path = ?2, filename = ?3, inner_name = NULL, size = ?4
         WHERE id = ?1",
        rusqlite::params![game.id, new_path, new_name, size as i64],
    )?;

    Ok(format!(
        "Unpacked to {new_name} and deleted the archive. The hashes are unchanged, since they were always of the ROM inside."
    ))
}

/// What the unpacked-ROM cache is costing, for somewhere to show it.
pub fn cache_usage(cache_root: &Path) -> (u64, usize) {
    let mut bytes = 0;
    let mut entries = 0;
    if let Ok(dirs) = std::fs::read_dir(cache_root.join("extracted")) {
        for dir in dirs.flatten() {
            entries += 1;
            bytes += dir_size(&dir.path());
        }
    }
    (bytes, entries)
}

pub(crate) fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Throw away everything unpacked. The originals are untouched; anything
/// deleted here is rebuilt the next time that game is launched.
pub fn clear_cache(cache_root: &Path) -> Result<u64> {
    let (bytes, _) = cache_usage(cache_root);
    let root = cache_root.join("extracted");
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    Ok(bytes)
}

/// Delete least-recently-used entries until the cache fits within `limit`.
pub fn prune_cache(cache_root: &Path, limit: u64) -> Result<u64> {
    let root = cache_root.join("extracted");
    let Ok(dirs) = std::fs::read_dir(&root) else {
        return Ok(0);
    };

    let mut entries: Vec<(std::time::SystemTime, u64, PathBuf)> = dirs
        .flatten()
        .map(|d| {
            let path = d.path();
            let used = std::fs::metadata(path.join(".used"))
                .or_else(|_| std::fs::metadata(&path))
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (used, dir_size(&path), path)
        })
        .collect();

    let mut total: u64 = entries.iter().map(|(_, size, _)| size).sum();
    if total <= limit {
        return Ok(0);
    }

    // Oldest first, so the one you have not played in longest goes.
    entries.sort_by_key(|(used, _, _)| *used);

    let mut freed = 0;
    for (_, size, path) in entries {
        if total <= limit {
            break;
        }
        if std::fs::remove_dir_all(&path).is_ok() {
            total = total.saturating_sub(size);
            freed += size;
        }
    }
    Ok(freed)
}

/// Build the command for a game without running it — also used by the UI to
/// show what will be executed. `rom_path` is what the emulator is handed,
/// which is not the library path when the ROM had to be unpacked.
pub fn build_command(
    conn: &rusqlite::Connection,
    game: &Game,
    rom_path: &str,
) -> Result<(String, Vec<String>)> {
    let cfg = resolve_for_game(conn, game);

    if cfg.mode == "custom" {
        let template = cfg
            .custom_command
            .filter(|c| !c.is_empty())
            .ok_or_else(|| AppError::Other(format!("No emulator set for {}", game.platform)))?;
        // Split first, then fill in. Filling first would split a ROM path
        // with a space in it into two arguments wherever the template left
        // {rom} unquoted, and a path is one argument however it is spelt.
        let mut argv: Vec<String> = tokenize(&template)
            .into_iter()
            .map(|arg| arg.replace("{rom}", rom_path))
            .collect();
        if argv.is_empty() {
            return Err(AppError::Other("Emulator command is empty".into()));
        }
        let exe = argv.remove(0);
        // A template with no {rom} still needs the ROM passed somewhere.
        if !template.contains("{rom}") {
            argv.push(rom_path.to_string());
        }
        return Ok((exe, argv));
    }

    let retroarch = db::get_setting(conn, "retroarch_path")?
        .map(|p| crate::detect::clean_path(&p))
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            AppError::Other("RetroArch path is not set — add it in Settings".into())
        })?;
    let ra_path = PathBuf::from(&retroarch);
    if !ra_path.exists() {
        return Err(AppError::Other(format!(
            "RetroArch not found at: {retroarch} — check the path in Settings, \
             or press Detect RetroArch"
        )));
    }

    let core = cfg.core.filter(|c| !c.is_empty()).ok_or_else(|| {
        let name = platforms::display_name(&game.platform);
        // Some systems have no libretro core in existence, so pointing at one
        // is not the fix — a standalone emulator is.
        if platforms::by_slug(&game.platform).map_or(false, |p| p.cores.is_empty()) {
            AppError::Other(format!(
                "{name} has no libretro core. Point it at a standalone emulator \
                 in Settings -> Emulators."
            ))
        } else {
            AppError::Other(format!("No libretro core set for {name}"))
        }
    })?;

    let cores_dir = db::get_setting(conn, "retroarch_cores_dir")?
        .map(|p| crate::detect::clean_path(&p))
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| default_cores_dir(&ra_path))
        .ok_or_else(|| {
            AppError::Other("RetroArch cores folder is not set — add it in Settings".into())
        })?;

    let core_path = cores_dir.join(core_filename(&core));
    if !core_path.exists() {
        return Err(AppError::Other(format!(
            "Core not found: {}",
            core_path.to_string_lossy()
        )));
    }

    Ok((
        retroarch,
        vec![
            "-L".into(),
            core_path.to_string_lossy().to_string(),
            rom_path.to_string(),
        ],
    ))
}

/// Start a game and record the session when it exits.
pub fn launch(
    app: &AppHandle,
    conn: &Mutex<rusqlite::Connection>,
    game: &Game,
    cache_root: &Path,
) -> Result<()> {
    if !Path::new(&game.path).exists() {
        return Err(AppError::Other(format!("ROM is missing: {}", game.path)));
    }

    let (exe, args) = {
        let guard = conn.lock().unwrap();

        // Check the emulator is actually set up before unpacking anything.
        // Unpacking a Switch game is several gigabytes, and discovering
        // afterwards that the system has no emulator configured would have
        // spent all of it to reach an error we could have raised first. The
        // path handed in here is not the one that finally runs; this call is
        // only asked whether a command can be built at all.
        build_command(&guard, game, &game.path)?;

        // Unpack the ROM if the emulator cannot read the archive it lives in.
        // Asking the core what it accepts beats guessing: Dolphin takes a
        // .wbfs but not the .7z holding it.
        let accepts = db::get_setting(&guard, "retroarch_path")?
            .map(|p| crate::detect::clean_path(&p))
            .filter(|p| !p.is_empty())
            .and_then(|p| {
                let core = resolve_for_game(&guard, game).core?;
                core_extensions(Path::new(&p), &core)
            });
        let rom = playable_path(game, cache_root, accepts.as_deref())?;
        // Trim afterwards, so a game is never evicted to make room for itself.
        // Untidy is not a reason to refuse to start something, so best effort.
        let _ = prune_cache(cache_root, cache_limit(&guard));

        let command = build_command(&guard, game, &rom)?;

        // Put any switched-on cheats where RetroArch will find them, now,
        // while it is definitely not running. Best effort: a game with no
        // cheats, a system with no core, or a RetroArch we cannot find are
        // all ordinary, and none of them is a reason to refuse to play.
        let _ = crate::cheats::sync_to_retroarch(&guard, game);

        command
    };

    let mut cmd = Command::new(&exe);
    cmd.args(&args);
    if let Some(dir) = Path::new(&exe).parent() {
        if dir.is_dir() {
            cmd.current_dir(dir);
        }
    }

    let child = cmd
        .spawn()
        .map_err(|e| AppError::Other(format!("Could not start {exe}: {e}")))?;

    let started_at = db::now();
    let game_id = game.id;
    let title = game.title.clone();
    let app_handle = app.clone();
    // An owned handle, so the waiting thread does not borrow the app.
    let db_handle = app.state::<crate::db::Db>().0.clone();

    let _ = app.emit(
        "game-launched",
        serde_json::json!({ "gameId": game_id, "title": title }),
    );

    // Wait off-thread so the UI stays responsive for the whole session.
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
        let seconds = (db::now() - started_at).max(0);

        if let Ok(guard) = db_handle.lock() {
            let _ = db::record_play(&guard, game_id, started_at, seconds);
        }

        let _ = app_handle.emit(
            "game-exited",
            serde_json::json!({
                "gameId": game_id,
                "title": title,
                "seconds": seconds,
            }),
        );
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "playdex-launch-{}-{}-{:?}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A game with its own emulator uses it; one without falls back to its
    /// system's; and a half-filled override - RetroArch mode with no core
    /// named - is not an answer, so it falls back too rather than failing at
    /// launch.
    #[test]
    fn a_games_own_emulator_beats_its_systems() {
        let dir = tmp("override");
        let conn = crate::db::open(&dir.join("test.db")).unwrap();
        conn.execute(
            "INSERT INTO games (id, path, filename, platform, title, added_at)
             VALUES (1, 'C:/roms/a.iso', 'a.iso', 'ps2', 'A', 0)",
            [],
        )
        .unwrap();
        let game = crate::db::get_game(&conn, 1).unwrap().unwrap();

        crate::db::set_emulator(
            &conn,
            &EmulatorConfig {
                platform: "ps2".into(),
                mode: "custom".into(),
                core: None,
                custom_command: Some("\"C:/pcsx2.exe\" \"{rom}\"".into()),
            },
        )
        .unwrap();
        assert_eq!(
            resolve_for_game(&conn, &game).custom_command.as_deref(),
            Some("\"C:/pcsx2.exe\" \"{rom}\""),
            "with no override, the system's emulator stands"
        );

        crate::db::set_game_emulator(
            &conn,
            1,
            Some(&EmulatorConfig {
                platform: "ps2".into(),
                mode: "retroarch".into(),
                core: Some("pcsx_rearmed".into()),
                custom_command: None,
            }),
        )
        .unwrap();
        let cfg = resolve_for_game(&conn, &game);
        assert_eq!(cfg.mode, "retroarch");
        assert_eq!(cfg.core.as_deref(), Some("pcsx_rearmed"));

        // Named RetroArch but no core: says nothing, so the system answers.
        crate::db::set_game_emulator(
            &conn,
            1,
            Some(&EmulatorConfig {
                platform: "ps2".into(),
                mode: "retroarch".into(),
                core: None,
                custom_command: None,
            }),
        )
        .unwrap();
        assert_eq!(resolve_for_game(&conn, &game).mode, "custom");

        // And clearing it puts the game back on its system's emulator.
        crate::db::set_game_emulator(&conn, 1, None).unwrap();
        assert!(crate::db::game_emulator(&conn, 1).unwrap().is_none());
        assert_eq!(resolve_for_game(&conn, &game).mode, "custom");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A ROM path with spaces in it is one argument, whether or not the
    /// template remembered to quote {rom}.
    #[test]
    fn a_rom_path_with_spaces_is_one_argument() {
        let dir = tmp("argv");
        let conn = crate::db::open(&dir.join("test.db")).unwrap();
        conn.execute(
            "INSERT INTO games (id, path, filename, platform, title, added_at)
             VALUES (1, 'C:/PS3/Demon''s Souls (USA)/PS3_GAME/USRDIR/EBOOT.BIN', 'EBOOT.BIN', 'ps3', 'A', 0)",
            [],
        )
        .unwrap();
        let game = crate::db::get_game(&conn, 1).unwrap().unwrap();
        let rom = "C:/PS3/Demon's Souls (USA)/PS3_GAME/USRDIR/EBOOT.BIN";

        for template in [
            r#""C:/Emulators/RPCS3/rpcs3.exe" --no-gui "{rom}""#,
            r#""C:/Emulators/RPCS3/rpcs3.exe" --no-gui {rom}"#,
        ] {
            crate::db::set_emulator(
                &conn,
                &EmulatorConfig {
                    platform: "ps3".into(),
                    mode: "custom".into(),
                    core: None,
                    custom_command: Some(template.into()),
                },
            )
            .unwrap();
            let (exe, args) = build_command(&conn, &game, rom).unwrap();
            assert_eq!(exe, "C:/Emulators/RPCS3/rpcs3.exe");
            assert_eq!(args, vec!["--no-gui".to_string(), rom.to_string()], "{template}");
        }

        // {rom} inside a longer argument is filled in where it stands.
        crate::db::set_emulator(
            &conn,
            &EmulatorConfig {
                platform: "ps3".into(),
                mode: "custom".into(),
                core: None,
                custom_command: Some(r#"xemu -dvd_path={rom}"#.into()),
            },
        )
        .unwrap();
        let (_, args) = build_command(&conn, &game, rom).unwrap();
        assert_eq!(args, vec![format!("-dvd_path={rom}")]);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn game_at(path: &Path, inner: Option<&str>) -> Game {
        Game {
            id: 1,
            path: path.to_string_lossy().to_string(),
            filename: path.file_name().unwrap().to_string_lossy().to_string(),
            platform: "wii".into(),
            size: 0,
            crc32: Some("8c246e48".into()),
            md5: None,
            sha1: None,
            inner_name: inner.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    /// RetroArch ships the answer next to every core; read it rather than
    /// guessing what the core will take.
    #[test]
    fn reads_a_cores_own_extension_list() {
        let dir = tmp("info");
        let exe = dir.join("retroarch.exe");
        std::fs::write(&exe, b"stub").unwrap();
        std::fs::create_dir_all(dir.join("info")).unwrap();
        std::fs::write(
            dir.join("info").join("dolphin_libretro.info"),
            "display_name = \"Nintendo - GameCube / Wii (Dolphin)\"
             supported_extensions = \"gcm|iso|wbfs|ciso|gcz|elf|dol|dff|tgc|wad|rvz|m3u|wia\"
",
        )
        .unwrap();

        let exts = core_extensions(&exe, "dolphin_libretro").expect("info file should parse");
        assert!(exts.contains(&"wbfs".to_string()));
        assert!(exts.contains(&"rvz".to_string()));
        // The point of the whole exercise: no archive format is on that list.
        assert!(!exts.contains(&"7z".to_string()));
        assert!(!exts.contains(&"zip".to_string()));

        assert!(core_extensions(&exe, "not_a_core").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Disc cores get a real file; everything else lets RetroArch open the
    /// archive itself.
    ///
    /// These lists are copied from real `.info` files. Note that not one of
    /// them mentions `zip` or `7z`: RetroArch handles archives in the
    /// frontend and never passes one to a core, which is exactly why "is the
    /// archive on the core's list" was the wrong question to ask.
    #[test]
    fn only_unpacks_for_cores_that_cannot_take_an_archive() {
        let dolphin: Vec<String> =
            "gcm|iso|wbfs|ciso|gcz|elf|dol|dff|tgc|wad|rvz|m3u|wia"
                .split('|')
                .map(|s| s.to_string())
                .collect();
        let mesen: Vec<String> = "nes|fds|unf|unif"
            .split('|')
            .map(|s| s.to_string())
            .collect();
        let mupen: Vec<String> = "n64|v64|z64|ndd|bin|u1"
            .split('|')
            .map(|s| s.to_string())
            .collect();

        let wii = game_at(Path::new("C:/roms/New Super Mario Bros Wii.7z"), None);
        let nes = game_at(Path::new("C:/roms/Super Mario Bros.zip"), None);
        let n64 = game_at(Path::new("C:/roms/Super Mario 64.zip"), None);

        // Dolphin loads discs, so it needs the file on disk.
        assert!(needs_extracting(&wii, Some(&dolphin)));

        // These do not, so unpacking would just duplicate the ROM. This is
        // the bug: both of these were being unpacked.
        assert!(!needs_extracting(&nes, Some(&mesen)));
        assert!(!needs_extracting(&n64, Some(&mupen)));

        // Mupen listing "bin" must not read as a disc core.
        assert!(!needs_extracting(&n64, Some(&mupen)));

        // A plain ROM is never unpacked, whatever the core.
        let plain = game_at(Path::new("C:/roms/Super Mario Bros.nes"), None);
        assert!(!needs_extracting(&plain, Some(&mesen)));
        assert!(!needs_extracting(&plain, Some(&dolphin)));

        // No core to ask: unpack, because a standalone emulator usually
        // cannot read an archive and a real file always works.
        assert!(needs_extracting(&wii, None));
        assert!(needs_extracting(&nes, None));
    }

    /// The cache is capped, and the least recently used entry goes first.
    #[test]
    fn prunes_the_oldest_entries_when_over_the_limit() {
        let dir = tmp("prune");
        let root = dir.join("extracted");

        // Three entries of 4 KB each, touched in a known order.
        for name in ["oldest", "middle", "newest"] {
            let entry = root.join(name);
            std::fs::create_dir_all(&entry).unwrap();
            std::fs::write(entry.join("rom.bin"), vec![0u8; 4096]).unwrap();
            touch(&entry);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        let (before, entries) = cache_usage(&dir);
        assert_eq!(entries, 3);
        assert!(before >= 12 * 1024);

        // Room for two of them.
        let freed = prune_cache(&dir, 9 * 1024).unwrap();
        assert!(freed > 0);

        assert!(!root.join("oldest").exists(), "the oldest should have gone");
        assert!(root.join("newest").exists(), "the newest should have stayed");

        // Under the limit already: nothing is touched.
        assert_eq!(prune_cache(&dir, 100 * 1024).unwrap(), 0);

        // And clearing takes the lot.
        clear_cache(&dir).unwrap();
        assert_eq!(cache_usage(&dir), (0, 0));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole path: an archived ROM is unpacked once and the emulator is
    /// handed the real file, not the archive.
    #[test]
    fn hands_the_emulator_the_unpacked_rom() {
        let dir = tmp("extract");
        let cache = dir.join("cache");

        let archive = dir.join("New Super Mario Bros Wii [SMNE01].7z");
        let payload: Vec<u8> = (0..8192).map(|i| (i * 31 % 251) as u8).collect();
        {
            let mut w = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
            w.push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("New Super Mario Bros Wii [SMNE01].wbfs"),
                Some(payload.as_slice()),
            )
            .unwrap();
            w.finish().unwrap();
        }

        let game = game_at(&archive, Some("New Super Mario Bros Wii [SMNE01].wbfs"));
        let dolphin: Vec<String> = ["wbfs", "iso"].iter().map(|s| s.to_string()).collect();

        let out = playable_path(&game, &cache, Some(&dolphin)).unwrap();
        assert!(out.ends_with(".wbfs"), "got {out}");
        assert_eq!(std::fs::read(&out).unwrap(), payload);

        // The preview says the same thing without unpacking anything.
        assert_eq!(preview_rom_path(&game, &cache, Some(&dolphin)), out);

        // Unpacking again is free: the file is already the right size.
        let again = playable_path(&game, &cache, Some(&dolphin)).unwrap();
        assert_eq!(again, out);

        // Nothing is left behind half-written.
        let leftovers: Vec<String> = std::fs::read_dir(Path::new(&out).parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".partial"))
            .collect();
        assert!(leftovers.is_empty(), "partial files left: {leftovers:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

