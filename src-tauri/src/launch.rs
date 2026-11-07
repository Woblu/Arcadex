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
    let dir = cache_dir_for(game, cache_root);
    if let Some(boot) = crate::consoles::archived_boot_entry(Path::new(&game.path)) {
        return crate::hashing::safe_entry_path(&dir, &boot)
            .unwrap_or(dir)
            .to_string_lossy()
            .to_string();
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

/// Something told how far an unpacking has got: bytes so far, and in all.
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64);

/// The path to hand the emulator, unpacking the ROM first if it has to be.
///
/// A console game that is more than one file - an Xbox 360 package and its
/// data, a PlayStation 3 folder - is unpacked whole, and the emulator handed
/// the one file it starts from. Anything else is a single ROM, and only that
/// entry is unpacked.
pub fn playable_path(
    game: &Game,
    cache_root: &Path,
    accepts: Option<&[String]>,
    progress: Progress,
) -> Result<String> {
    if !needs_extracting(game, accepts) {
        return Ok(game.path.clone());
    }
    let dir = cache_dir_for(game, cache_root);
    if let Some(boot) = crate::consoles::archived_boot_entry(Path::new(&game.path)) {
        unpack_whole(Path::new(&game.path), &dir, progress)?;
        touch(&dir);
        let file = crate::hashing::safe_entry_path(&dir, &boot)
            .filter(|f| f.is_file())
            .ok_or_else(|| AppError::Other(format!("{boot} was not in the archive after all")))?;
        return Ok(file.to_string_lossy().to_string());
    }
    let file = crate::hashing::extract_rom(Path::new(&game.path), &dir)?;
    touch(&dir);
    Ok(file.to_string_lossy().to_string())
}

/// Marks a cache folder as holding a whole archive, unpacked to the end.
const COMPLETE: &str = ".complete";

/// `path` with `.suffix` added, without treating a dot already in the name -
/// `Vol. 2` - as the start of an extension to replace.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".");
    s.push(suffix);
    PathBuf::from(s)
}

/// Unpack a whole archive into `dir`, once.
///
/// It goes into a `.partial` folder that is renamed only when every file is
/// out, so an unpack cut short - Arcadex closed, the disk full - never leaves
/// half a game that the next launch takes for a whole one.
fn unpack_whole(archive: &Path, dir: &Path, progress: Progress) -> Result<()> {
    if dir.join(COMPLETE).is_file() {
        return Ok(());
    }
    let partial = with_suffix(dir, "partial");
    let _ = std::fs::remove_dir_all(&partial);
    crate::hashing::extract_all(archive, &partial, progress)?;
    std::fs::write(partial.join(COMPLETE), b"")?;
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&partial, dir)?;
    Ok(())
}

/// The one folder every file in an archive sits under, if there is one.
fn common_top_folder(entries: &[(String, u64)]) -> Option<String> {
    let mut top: Option<&str> = None;
    for (name, _) in entries {
        let normal = name.trim_start_matches(['/', '\\']);
        let (first, rest) = normal.split_once(['/', '\\'])?;
        if rest.is_empty() || top.is_some_and(|t| t != first) {
            return None;
        }
        top = Some(first);
    }
    top.map(|t| t.to_string())
}

/// What unpacking an archive in place left behind.
pub struct Unpacked {
    /// The file the library entry now points at.
    pub path: String,
    pub filename: String,
    pub size: u64,
    pub message: String,
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
/// This deletes something of yours, which nothing else in Arcadex does. It
/// only ever happens when asked for directly.
pub fn unpack_in_place(game: &Game, cache_root: &Path, progress: Progress) -> Result<Unpacked> {
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
    if let Some(boot) = crate::consoles::archived_boot_entry(&archive) {
        return unpack_folder_in_place(game, &archive, &boot, cache_root, progress);
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

    let new_name = leaf.to_string_lossy().to_string();
    Ok(Unpacked {
        path: destination.to_string_lossy().to_string(),
        message: format!(
            "Unpacked to {new_name} and deleted the archive. The hashes are unchanged, since they were always of the ROM inside."
        ),
        filename: new_name,
        size,
    })
}

/// Replace an archived console game with the folder inside it.
///
/// The same order as for a single ROM: everything is put in place and
/// checked, and the archive is deleted last. An archive that holds one
/// folder unpacks as that folder beside it; one holding loose files gets a
/// folder of its own, named after the archive, rather than spilling them
/// across the folder it sat in. A copy a launch already unpacked into the
/// cache is moved rather than unpacked a second time.
fn unpack_folder_in_place(
    game: &Game,
    archive: &Path,
    boot: &str,
    cache_root: &Path,
    progress: Progress,
) -> Result<Unpacked> {
    let entries = crate::hashing::archive_entries(archive)?;
    let total: u64 = entries.iter().map(|(_, size)| size).sum();
    let parent = archive
        .parent()
        .ok_or_else(|| AppError::Other("The archive has no folder".into()))?;
    let top = common_top_folder(&entries);
    let (base, folder) = match &top {
        Some(t) => (parent.to_path_buf(), parent.join(t)),
        None => {
            let stem = archive.file_stem().unwrap_or_default();
            (parent.join(stem), parent.join(stem))
        }
    };
    if folder.exists() {
        return Err(AppError::Other(format!(
            "{} is already there. Nothing was changed.",
            folder.display()
        )));
    }

    // The archive's own layout is laid out first, then moved into place: the
    // copy in the cache if a launch already unpacked it in full, or else a
    // working folder beside the archive, so the last step is a rename on the
    // same drive.
    let cached = cache_dir_for(game, cache_root);
    let pick = |root: &Path| match &top {
        Some(t) => root.join(t),
        None => root.to_path_buf(),
    };
    let moved = cached.join(COMPLETE).is_file() && std::fs::rename(pick(&cached), &folder).is_ok();
    let work = parent.join(format!(".arcadex-unpacking-{}", game.id));
    // An unpack cut short before the rename left its half under the old name.
    let _ = std::fs::remove_dir_all(parent.join(format!(".playdex-unpacking-{}", game.id)));
    if !moved {
        let _ = std::fs::remove_dir_all(&work);
        crate::hashing::extract_all(archive, &work, progress)?;
        if let Err(e) = std::fs::rename(pick(&work), &folder) {
            let _ = std::fs::remove_dir_all(&work);
            return Err(e.into());
        }
    }
    // The cache's own markers do not belong in a game's folder.
    let _ = std::fs::remove_file(folder.join(COMPLETE));
    let _ = std::fs::remove_file(folder.join(".used"));

    // Refuse to go further unless every byte arrived.
    let boot_file = crate::hashing::safe_entry_path(&base, boot).filter(|f| f.is_file());
    let arrived = dir_size(&folder);
    let Some(boot_file) = boot_file.filter(|_| arrived == total) else {
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_dir_all(&work);
        return Err(AppError::Other(
            "The unpacked game did not arrive whole, so the archive was left alone.".into(),
        ));
    };

    // Only now is there a spare copy to lose.
    std::fs::remove_file(archive)?;
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(&cached);

    let filename = crate::consoles::for_boot_file(&boot_file)
        .map(|g| g.name())
        .unwrap_or_else(|| folder.file_name().unwrap_or_default().to_string_lossy().to_string());
    Ok(Unpacked {
        path: boot_file.to_string_lossy().to_string(),
        message: format!("Unpacked to {} and deleted the archive.", folder.display()),
        filename,
        size: total,
    })
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
///
/// `keep` is never deleted, however far over the limit it alone puts the
/// cache: it is the game about to start, and evicting it would hand the
/// emulator a path to nothing.
pub fn prune_cache(cache_root: &Path, limit: u64, keep: Option<&Path>) -> Result<u64> {
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
        if keep.is_some_and(|k| k.starts_with(&path)) {
            continue;
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

    // Asked before anything about RetroArch: a PlayStation 3 game that says
    // "RetroArch path is not set" sends you off to fix something that was
    // never going to run it.
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

/// Games being got ready to play: unpacked, or about to be.
static PREPARING: std::sync::LazyLock<Mutex<std::collections::HashSet<i64>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

/// Holds a game's place while it is got ready, so asking twice does not
/// unpack the same several gigabytes twice over.
struct Preparing(i64);

impl Preparing {
    fn claim(game: &Game) -> Result<Self> {
        if !PREPARING.lock().unwrap().insert(game.id) {
            return Err(AppError::Other(format!(
                "{} is already being got ready to play.",
                game.title
            )));
        }
        Ok(Preparing(game.id))
    }
}

impl Drop for Preparing {
    fn drop(&mut self) {
        PREPARING.lock().unwrap().remove(&self.0);
    }
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

    let (accepts, limit) = {
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
        (accepts, cache_limit(&guard))
    };

    // Unpacking a disc or a package can take minutes, so it happens with the
    // library unlocked - everything else keeps working meanwhile - and says
    // how far it has got.
    let _preparing = Preparing::claim(game)?;
    let rom = {
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
        let rom = playable_path(game, cache_root, accepts.as_deref(), &mut report);
        let _ = app.emit(
            "unpack-progress",
            serde_json::json!({ "gameId": game.id, "title": game.title, "finished": true }),
        );
        rom?
    };
    // Trim afterwards, so a game is never evicted to make room for itself.
    // Untidy is not a reason to refuse to start something, so best effort.
    let _ = prune_cache(cache_root, limit, Some(Path::new(&rom)));

    let (exe, args) = {
        let guard = conn.lock().unwrap();
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
            "arcadex-launch-{}-{}-{:?}",
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

    /// A system no core exists for says so, rather than asking for a
    /// RetroArch that would not run it anyway.
    #[test]
    fn a_system_with_no_core_asks_for_a_standalone_emulator() {
        let dir = tmp("nocore");
        let conn = crate::db::open(&dir.join("test.db")).unwrap();
        conn.execute(
            "INSERT INTO games (id, path, filename, platform, title, added_at)
             VALUES (1, 'C:/PS3/Game/PS3_GAME/USRDIR/EBOOT.BIN', 'Game', 'ps3', 'Game', 0)",
            [],
        )
        .unwrap();
        let game = crate::db::get_game(&conn, 1).unwrap().unwrap();

        // No RetroArch set at all, which is the ordinary case for someone
        // who only plays newer consoles.
        let err = build_command(&conn, &game, &game.path).unwrap_err().to_string();
        assert!(err.contains("standalone emulator"), "{err}");
        assert!(!err.contains("RetroArch path"), "{err}");

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

    fn archived_package(dir: &Path) -> Game {
        let zip = dir.join("Skate 3.zip");
        crate::consoles::tests::zipped_package(&zip);
        Game {
            id: 7,
            path: zip.to_string_lossy().to_string(),
            filename: "Skate 3.zip".into(),
            platform: "xbox360".into(),
            title: "Skate 3".into(),
            ..Default::default()
        }
    }

    const HEADER: &str = "07A9631BB10EE556B1CE";

    /// A Games on Demand package is a header and a folder of data. Handing
    /// Xenia the header alone, which is all the single-file rule unpacked,
    /// is handing it half a game.
    #[test]
    fn unpacks_a_zipped_package_whole_and_starts_its_header() {
        let dir = tmp("wholezip");
        let cache = dir.join("cache");
        let game = archived_package(&dir);

        let mut reports = 0;
        let out = playable_path(&game, &cache, None, &mut |_, _| reports += 1).unwrap();
        let header = PathBuf::from(&out);
        assert_eq!(header.file_name().unwrap(), HEADER);
        assert!(header.is_file());
        let data = header.with_file_name(format!("{HEADER}.data"));
        assert_eq!(std::fs::read_dir(&data).unwrap().count(), 3, "all its data came too");
        assert!(reports > 0, "progress was reported");

        // The preview names the same file without unpacking anything.
        assert_eq!(preview_rom_path(&game, &cache, None), out);

        // Unpacked once: the second launch reads nothing from the archive.
        let mut again = 0;
        assert_eq!(playable_path(&game, &cache, None, &mut |_, _| again += 1).unwrap(), out);
        assert_eq!(again, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Half an unpack, left by Arcadex closing partway, is not taken for a
    /// whole one.
    #[test]
    fn an_unpack_cut_short_is_done_again() {
        let dir = tmp("partialzip");
        let cache = dir.join("cache");
        let game = archived_package(&dir);
        let target = cache_dir_for(&game, &cache);
        std::fs::create_dir_all(with_suffix(&target, "partial")).unwrap();

        let out = playable_path(&game, &cache, None, &mut |_, _| {}).unwrap();
        assert!(Path::new(&out).is_file());
        assert!(!with_suffix(&target, "partial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unpacks_a_zipped_package_in_place_and_deletes_the_zip() {
        let dir = tmp("inplacezip");
        let cache = dir.join("cache");
        let game = archived_package(&dir);

        let done = unpack_in_place(&game, &cache, &mut |_, _| {}).unwrap();
        let header = PathBuf::from(&done.path);
        // The zip held one folder, so that folder is what appears beside it.
        assert_eq!(header, dir.join("Skate 3").join("454108E6").join("00007000").join(HEADER));
        assert!(header.is_file());
        assert!(!dir.join("Skate 3.zip").exists(), "the zip goes last, once all is in place");
        assert_eq!(done.filename, "Skate 3", "named from the package's header");
        assert_eq!(done.size, 0xB000 + 3000);
        assert!(!dir.join(format!(".arcadex-unpacking-{}", game.id)).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Played once already, so the cache holds the whole game: it is moved,
    /// not unpacked a second time, and none of the cache's markers come too.
    #[test]
    fn unpacking_in_place_moves_what_a_launch_already_unpacked() {
        let dir = tmp("inplacecached");
        let cache = dir.join("cache");
        let game = archived_package(&dir);
        playable_path(&game, &cache, None, &mut |_, _| {}).unwrap();

        let mut read_again = false;
        let done = unpack_in_place(&game, &cache, &mut |_, _| read_again = true).unwrap();
        assert!(!read_again, "moved rather than unpacked again");
        assert!(Path::new(&done.path).is_file());
        assert!(!cache_dir_for(&game, &cache).exists());
        assert!(!dir.join("Skate 3").join(COMPLETE).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Loose files get a folder of their own rather than spilling out.
    #[test]
    fn an_archive_of_loose_files_unpacks_into_a_folder_named_after_it() {
        let dir = tmp("inplaceloose");
        let zip = dir.join("Bloodborne.zip");
        crate::consoles::tests::zip_of(
            &zip,
            &[
                (
                    "sce_sys/param.sfo",
                    crate::consoles::tests::sfo(&[("CATEGORY", "gd"), ("TITLE", "Bloodborne"), ("TITLE_ID", "CUSA00003")]),
                ),
                ("eboot.bin", vec![1; 64]),
            ],
        );
        let game = Game {
            id: 8,
            path: zip.to_string_lossy().to_string(),
            platform: "ps4".into(),
            ..Default::default()
        };
        let done = unpack_in_place(&game, &dir.join("cache"), &mut |_, _| {}).unwrap();
        assert_eq!(PathBuf::from(&done.path), dir.join("Bloodborne").join("eboot.bin"));
        assert_eq!(done.filename, "Bloodborne");

        // A second archive that would land on the same folder is refused,
        // and left where it is.
        let zip2 = dir.join("Bloodborne.zip");
        crate::consoles::tests::zip_of(&zip2, &[("eboot.bin", vec![1; 64]), ("sce_sys/param.sfo", vec![0; 8])]);
        let game2 = Game { path: zip2.to_string_lossy().to_string(), ..game };
        assert!(unpack_in_place(&game2, &dir.join("cache"), &mut |_, _| {}).is_err());
        assert!(zip2.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A game larger than the whole cache limit is still not evicted to make
    /// room for itself, moments before the emulator opens it.
    #[test]
    fn pruning_never_takes_the_game_about_to_start() {
        let dir = tmp("prunekeep");
        let root = dir.join("extracted");
        for name in ["older", "playing"] {
            let entry = root.join(name);
            std::fs::create_dir_all(&entry).unwrap();
            std::fs::write(entry.join("rom.bin"), vec![0u8; 8192]).unwrap();
            touch(&entry);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let rom = root.join("playing").join("rom.bin");
        prune_cache(&dir, 1024, Some(&rom)).unwrap();
        assert!(rom.exists());
        assert!(!root.join("older").exists());
        let _ = std::fs::remove_dir_all(&dir);
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
        let freed = prune_cache(&dir, 9 * 1024, None).unwrap();
        assert!(freed > 0);

        assert!(!root.join("oldest").exists(), "the oldest should have gone");
        assert!(root.join("newest").exists(), "the newest should have stayed");

        // Under the limit already: nothing is touched.
        assert_eq!(prune_cache(&dir, 100 * 1024, None).unwrap(), 0);

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

        let out = playable_path(&game, &cache, Some(&dolphin), &mut |_, _| {}).unwrap();
        assert!(out.ends_with(".wbfs"), "got {out}");
        assert_eq!(std::fs::read(&out).unwrap(), payload);

        // The preview says the same thing without unpacking anything.
        assert_eq!(preview_rom_path(&game, &cache, Some(&dolphin)), out);

        // Unpacking again is free: the file is already the right size.
        let again = playable_path(&game, &cache, Some(&dolphin), &mut |_, _| {}).unwrap();
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

