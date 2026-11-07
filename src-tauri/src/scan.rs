//! Filesystem scanning: find ROMs, work out what platform they belong to,
//! hash them, and record them in the library.

use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

use crate::consoles::{self, ConsoleGame};
use crate::db;
use crate::error::Result;
use crate::hashing;
use crate::models::{LibraryFolder, ScanProgress};
use crate::romcheck;
use crate::signature;
use crate::platforms;

/// Strip the extension and the usual No-Intro / TOSEC bracket tags to get
/// something presentable before a scraper has run.
pub fn clean_title(filename: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);
    clean_name(stem)
}

/// The same tidying for a name with no extension to strip, such as a folder.
/// `Vol. 2` is not a file called `Vol` with a `. 2` extension.
pub fn clean_name(stem: &str) -> String {
    let mut out = String::new();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    for ch in stem.chars() {
        match ch {
            '(' => depth_paren += 1,
            ')' => depth_paren = (depth_paren - 1).max(0),
            '[' => depth_brack += 1,
            ']' => depth_brack = (depth_brack - 1).max(0),
            _ if depth_paren == 0 && depth_brack == 0 => out.push(ch),
            _ => {}
        }
    }

    let out = out.replace(['_', '.'], " ");
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let out = out.trim_matches(|c: char| c == '-' || c.is_whitespace()).to_string();

    // "Legend of Zelda, The" -> "The Legend of Zelda"
    if let Some((head, tail)) = out.rsplit_once(", ") {
        let t = tail.trim();
        if matches!(
            t.to_ascii_lowercase().as_str(),
            "the" | "a" | "an" | "le" | "la" | "les" | "der" | "die" | "das"
        ) {
            return format!("{t} {head}");
        }
    }

    if out.is_empty() {
        stem.to_string()
    } else {
        out
    }
}

/// Pull a region out of the filename tags, e.g. "Sonic (USA, Europe).md".
pub fn guess_region(filename: &str) -> Option<String> {
    const REGIONS: &[&str] = &[
        "USA", "Europe", "Japan", "World", "Australia", "Germany", "France", "Spain", "Italy",
        "Korea", "Brazil", "Canada", "China", "Netherlands", "Sweden", "Asia", "Taiwan",
    ];
    let lower = filename.to_ascii_lowercase();
    let mut found: Vec<&str> = Vec::new();
    for r in REGIONS {
        if lower.contains(&r.to_ascii_lowercase()) {
            found.push(r);
        }
    }
    if found.is_empty() {
        None
    } else {
        Some(found.join(", "))
    }
}

/// Decide which platform a ROM belongs to.
///
/// Extensions alone are not enough — `.bin`, `.cue`, `.iso` and `.zip` are
/// shared across a dozen systems — so this falls back to the folder's assigned
/// platform, then to what is inside the archive, then to directory names, and
/// finally to the ROM's own name.
pub fn detect_platform(path: &Path, root: &Path, folder_override: Option<&str>) -> String {
    if let Some(p) = folder_override.filter(|p| !p.is_empty()) {
        return p.to_string();
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // 1. An extension only one system uses settles it.
    let candidates = platforms::candidates_for_ext(&ext);
    if candidates.len() == 1 {
        return candidates[0].slug.to_string();
    }

    // 2. For an archive, look at what is inside.
    let mut inner_names: Vec<String> = Vec::new();
    if platforms::ARCHIVE_EXTS.contains(&ext.as_str()) {
        if let Ok(names) = hashing::archive_entry_names(path) {
            for name in &names {
                let inner_ext = Path::new(name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let inner = platforms::candidates_for_ext(&inner_ext);
                if inner.len() == 1 {
                    return inner[0].slug.to_string();
                }
            }
            inner_names = names;
        }
        // A disc image zipped up: `.iso` and `.bin` say nothing, but its first
        // few megabytes do, and those are a short read even from inside an
        // archive. Without this a zipped PlayStation 2 or Xbox 360 disc was
        // Unidentified unless its name happened to say.
        if let Some(found) = archived_disc_signature(path) {
            return found.to_string();
        }
    }

    // 3. What the file says about itself. Only reached when the extension did
    //    not settle it, which is exactly the ambiguous cases: .iso, .bin and
    //    .chd belong to a dozen systems, and a disc image announces which one
    //    in its first few hundred bytes. Bytes beat both the folder someone
    //    filed it under and the name they gave it, so this comes first of the
    //    guesses. Archives are skipped - reaching a header inside one means
    //    decompressing it, and their inner extension already spoke at step 2.
    if !platforms::ARCHIVE_EXTS.contains(&ext.as_str()) {
        if let Some(found) = signature::identify(path) {
            return found.to_string();
        }
        // A playlist is a few lines of text and says nothing about itself, but
        // the tracks it names are the disc. Saturn, Dreamcast and Mega CD all
        // write their system into the start of the data track.
        if romcheck::is_playlist(&ext) {
            if let Some(dir) = path.parent() {
                for track in referenced_tracks(path).iter().take(4) {
                    if let Some(found) = signature::identify(&dir.join(track)) {
                        return found.to_string();
                    }
                }
            }
        }
    }

    // 4. Directory names between the library root and the file, nearest first.
    let mut dirs: Vec<String> = Vec::new();
    let mut cur = path.parent();
    while let Some(d) = cur {
        if d == root {
            if let Some(name) = d.file_name().and_then(|n| n.to_str()) {
                dirs.push(name.to_string());
            }
            break;
        }
        if let Some(name) = d.file_name().and_then(|n| n.to_str()) {
            dirs.push(name.to_string());
        }
        cur = d.parent();
    }
    for d in &dirs {
        if let Some(p) = platforms::match_alias(d) {
            return p.slug.to_string();
        }
    }

    // 5. The ROM's own name, and the names inside the archive. Dumps are
    //    routinely called "Mario Kart Wii" or "Sonic (Mega Drive)", which is
    //    the last real evidence available for a container extension like .7z
    //    that belongs to no system at all. Matched on whole words only, so a
    //    title is not mistaken for a system it merely contains the letters of.
    let own_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    for name in std::iter::once(own_name).chain(inner_names.iter().map(|s| s.as_str())) {
        if let Some(p) = platforms::match_alias_word(name) {
            return p.slug.to_string();
        }
    }

    // 6. Extension shared by a few systems and nothing else to go on: take the
    //    first candidate rather than dropping the ROM entirely.
    if let Some(first) = candidates.first() {
        return first.slug.to_string();
    }

    "unknown".to_string()
}

#[derive(Default)]
pub struct ScanTally {
    pub added: usize,
    pub skipped: usize,
    /// Entries already in the library that were corrected on this pass.
    pub reindexed: usize,
    /// Entries dropped because they are no longer recognised as games. The
    /// file on disk is never touched.
    pub dropped: usize,
    /// Files that were not games at all.
    pub ignored: usize,
    /// Multi-disc games gathered into a single entry.
    pub disc_sets: usize,
    reasons: Vec<String>,
}

/// Whether an entry already in the library was indexed under a rule we have
/// since fixed, and so deserves another look.
///
/// Two cases, both cheap to spot: a platform we never worked out, and an
/// archive we recorded without ever opening — its stored hashes are the
/// container's, which match nothing in a scraper or a DAT.
fn needs_reindex(state: &db::IndexState, ext: &str) -> bool {
    state.platform == "unknown"
        || (platforms::ARCHIVE_EXTS.contains(&ext) && state.inner_name.is_none())
}

impl ScanTally {
    /// The one-line summary of a finished scan, so the progress event and the
    /// command's return value can never drift apart.
    pub fn message(&self) -> String {
        let mut out = format!(
            "Added {}, skipped {}, ignored {}",
            self.added, self.skipped, self.ignored
        );
        if self.reindexed > 0 {
            out.push_str(&format!(", corrected {}", self.reindexed));
        }
        if self.dropped > 0 {
            out.push_str(&format!(", dropped {}", self.dropped));
        }
        if self.disc_sets > 0 {
            out.push_str(&format!(
                ", grouped {} multi-disc game{}",
                self.disc_sets,
                if self.disc_sets == 1 { "" } else { "s" }
            ));
        }
        out
    }

    pub fn summary(&self) -> String {
        if self.reasons.is_empty() {
            String::new()
        } else {
            romcheck::summarise(&self.reasons)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit(
    app: &AppHandle,
    phase: &str,
    current: usize,
    total: usize,
    message: &str,
    tally: &ScanTally,
    done: bool,
) {
    let _ = app.emit(
        "scan-progress",
        ScanProgress {
            phase: phase.to_string(),
            current,
            total,
            message: message.to_string(),
            added: tally.added,
            skipped: tally.skipped,
            corrected: tally.reindexed,
            dropped: tally.dropped,
            ignored: tally.ignored,
            ignored_summary: tally.summary(),
            done,
        },
    );
}

/// How much of a zipped disc image is read to identify it.
const DISC_HEAD_BYTES: u64 = 4 * 1024 * 1024;

/// The system the disc image inside an archive says it is, if the ROM inside
/// is a disc image at all.
fn archived_disc_signature(path: &Path) -> Option<&'static str> {
    let entry = hashing::archive_rom_entry(path)?;
    let inner_ext = Path::new(&entry)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !platforms::AMBIGUOUS_EXTS.contains(&inner_ext.as_str()) {
        return None;
    }
    let head = hashing::read_archive_entry(path, &entry, DISC_HEAD_BYTES).ok()?;
    signature::identify_head(&head)
}

/// What happened to one file.
pub enum Indexed {
    Added(i64),
    AlreadyKnown,
    NotRom(String),
}

/// Index a single file into the library.
///
/// Shared by the folder scan and by dropping a ROM onto the window, so a game
/// added either way ends up described identically - same platform detection,
/// same rejection rules, same hashing, same cleaned-up title. A second path
/// for "quick add" would be a second place for those rules to drift.
///
/// `root` is the folder platform detection is allowed to reason about. For a
/// scan that is the library folder; for a dropped file it is whatever folder
/// the file came out of, which is still worth reading since people keep ROMs
/// in folders named after the system.
pub fn index_file(
    conn: &rusqlite::Connection,
    path: &Path,
    root: &Path,
    override_platform: Option<&str>,
) -> Result<Indexed> {
    let path_str = path.to_string_lossy().to_string();
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    // A game that is a folder is described by the folder. `EBOOT.BIN` is not
    // a title, and the console wrote the real one down.
    if let Some(game) = consoles::for_boot_file(path) {
        return index_console_game(conn, &path_str, &game);
    }
    // The same game zipped up whole is still that game. Nothing in a Games
    // on Demand package has an extension, so without this a zipped one was
    // Unidentified.
    let archived = is_archive(path)
        .then(|| consoles::identify_archive(path))
        .flatten();
    if let Some(game) = archived {
        return insert_console_record(conn, &path_str, &archived_record(&game, &filename));
    }

    let platform = detect_platform(path, root, override_platform);

    // Reject anything that is plainly not a game before spending the time to
    // hash it.
    match romcheck::inspect(path, &platform) {
        romcheck::Verdict::NotRom(reason) => return Ok(Indexed::NotRom(reason)),
        // Odd, but indexed anyway - losing a real game is worse.
        romcheck::Verdict::Suspect(_) | romcheck::Verdict::Rom => {}
    }

    let hashes = hashing::hash_rom(path).ok().flatten();
    let size = hashes
        .as_ref()
        .map(|h| h.size as i64)
        .or_else(|| std::fs::metadata(path).ok().map(|m| m.len() as i64))
        .unwrap_or(0);

    // A zipped ROM's real name is the entry inside, which is what the
    // scrapers index - recorded even when the archive is too big to hash.
    let inner_name = inner_entry(path, hashes.as_ref());
    let name_for_title = inner_name.clone().unwrap_or_else(|| filename.clone());

    let inserted = db::insert_game(
        conn,
        &path_str,
        &filename,
        &platform,
        &clean_title(&name_for_title),
        guess_region(&name_for_title).as_deref(),
        size,
        hashes.as_ref().map(|h| h.crc32.as_str()),
        hashes.as_ref().map(|h| h.md5.as_str()),
        hashes.as_ref().map(|h| h.sha1.as_str()),
        inner_name.as_deref(),
    )?;

    if inserted {
        Ok(Indexed::Added(
            conn.query_row(
                "SELECT id FROM games WHERE path = ?1",
                rusqlite::params![path_str],
                |r| r.get(0),
            )
            .unwrap_or(0),
        ))
    } else {
        Ok(Indexed::AlreadyKnown)
    }
}

/// What a console game is recorded as: its folder's name where a file's name
/// would go, the title the console wrote, and the size of the whole folder.
///
/// Nothing is hashed. The file the emulator is handed is a few megabytes of
/// executable, and a checksum of that matches no catalogue and no scraper;
/// the game itself runs to tens of gigabytes. A folder's platform is not
/// open to the folder assignment in Settings either - `PS3_GAME/PARAM.SFO`
/// is not a guess, and an assignment made for the discs in a folder should
/// not relabel a game that merely sits there too.
struct ConsoleRecord {
    platform: &'static str,
    filename: String,
    title: String,
    region: Option<String>,
    size: i64,
    serial: Option<String>,
}

fn console_record(game: &ConsoleGame) -> ConsoleRecord {
    let name = game.name();
    ConsoleRecord {
        platform: game.platform,
        title: game.title.clone().unwrap_or_else(|| clean_name(&name)),
        region: guess_region(&name),
        size: consoles::size_on_disk(game) as i64,
        serial: game.serial.clone(),
        filename: name,
    }
}

fn needs_console_identity(state: &db::IndexState, game: &ConsoleGame, boot_name: &str) -> bool {
    state.platform != game.platform && state.filename == boot_name
}

/// What an archived console game is recorded as. Its name is the archive's,
/// since that is the file in the library, and its size is everything inside.
fn archived_record(game: &consoles::ArchivedGame, filename: &str) -> ConsoleRecord {
    ConsoleRecord {
        platform: game.platform,
        title: game.title.clone().unwrap_or_else(|| clean_title(filename)),
        region: guess_region(filename),
        size: game.size as i64,
        serial: game.serial.clone(),
        filename: filename.to_string(),
    }
}

fn is_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| platforms::ARCHIVE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The ROM's name inside an archive: the one hashed, or for an archive too
/// large to hash, the one that would have been.
fn inner_entry(path: &Path, hashes: Option<&hashing::Hashes>) -> Option<String> {
    hashes
        .and_then(|h| h.inner_name.clone())
        .or_else(|| is_archive(path).then(|| hashing::archive_rom_entry(path)).flatten())
}

fn index_console_game(
    conn: &rusqlite::Connection,
    path_str: &str,
    game: &ConsoleGame,
) -> Result<Indexed> {
    insert_console_record(conn, path_str, &console_record(game))
}

fn insert_console_record(
    conn: &rusqlite::Connection,
    path_str: &str,
    r: &ConsoleRecord,
) -> Result<Indexed> {
    let inserted = db::insert_game(
        conn,
        path_str,
        &r.filename,
        r.platform,
        &r.title,
        r.region.as_deref(),
        r.size,
        None,
        None,
        None,
        None,
    )?;
    if !inserted {
        return Ok(Indexed::AlreadyKnown);
    }
    let id: i64 = conn
        .query_row(
            "SELECT id FROM games WHERE path = ?1",
            rusqlite::params![path_str],
            |r| r.get(0),
        )
        .unwrap_or(0);
    db::set_serial(conn, id, r.serial.as_deref())?;
    Ok(Indexed::Added(id))
}

/// Library entries that a scan made from inside a console game's folder
/// before Arcadex knew the folder was one game: its data files, each listed
/// as a game of its own. The boot file is kept, since that is the game, and
/// so is anything you have played or favourited, as everywhere else.
fn stale_console_files(
    known: &std::collections::HashMap<String, db::IndexState>,
    games: &[ConsoleGame],
) -> Vec<(i64, String)> {
    use std::collections::{HashMap, HashSet};

    let roots: HashMap<&Path, &'static str> =
        games.iter().map(|g| (g.root.as_path(), g.platform)).collect();
    let boots: HashSet<&Path> = games.iter().filter_map(|g| g.boot.as_deref()).collect();

    known
        .iter()
        .filter(|(_, state)| !state.precious)
        .filter_map(|(path, state)| {
            let p = Path::new(path);
            if boots.contains(p) {
                return None;
            }
            let platform = p.ancestors().skip(1).find_map(|a| roots.get(a))?;
            Some((
                state.id,
                format!("part of a {} game's folder", platforms::display_name(platform)),
            ))
        })
        .collect()
}

/// Files a disc is actually made of, which a playlist speaks for.
const TRACK_EXTS: &[&str] = &["bin", "raw", "img", "wav", "iso", "mdf", "sub", "ccd"];

/// The file names a playlist points at.
///
/// A `.cue` names its tracks in `FILE "..." BINARY` lines, a `.gdi` lists them
/// as bare tokens, an `.m3u` is one path per line. Rather than write three
/// parsers, this pulls out anything that looks like a filename with a track
/// extension, which is all any of them contain that we care about.
fn referenced_tracks(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for raw in text.split(|c: char| c == '"' || c.is_whitespace()) {
        let token = raw.trim().trim_matches('\'');
        let ext = Path::new(token)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if TRACK_EXTS.contains(&ext.as_str()) {
            if let Some(leaf) = Path::new(token).file_name().and_then(|n| n.to_str()) {
                out.push(leaf.to_ascii_lowercase());
            }
        }
    }
    out
}

/// Drop the pieces of a disc that a playlist in the same folder speaks for.
///
/// A PS1 game kept unzipped is a cue sheet and three or four `.bin` tracks. All
/// of them look like candidates, so the library filled up with "Track 1",
/// "Track 2" and "Track 3" as separate games, none of which an emulator can
/// open on its own. The cue is the game; the tracks belong to it.
///
/// Names referenced by the playlist go first. Any remaining track-shaped file
/// sharing a folder with a playlist goes too, because dumps get renamed and a
/// cue that no longer matches its tracks is common - and a stray `.bin` beside
/// a cue sheet was never a second game.
fn drop_disc_tracks(candidates: &mut Vec<(PathBuf, PathBuf, Option<String>)>) {
    use std::collections::{HashMap, HashSet};

    let mut playlists: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    for (path, _, _) in candidates.iter() {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !romcheck::is_playlist(&ext) {
            continue;
        }
        let Some(dir) = path.parent() else { continue };
        playlists
            .entry(dir.to_path_buf())
            .or_default()
            .extend(referenced_tracks(path));
    }

    if playlists.is_empty() {
        return;
    }

    candidates.retain(|(path, _, _)| {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if romcheck::is_playlist(&ext) || !TRACK_EXTS.contains(&ext.as_str()) {
            return true;
        }
        let Some(dir) = path.parent() else { return true };
        // A track only loses to a playlist standing beside it.
        playlists.get(dir).is_none()
    });
}

/// What came of dropping things onto the window.
#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DropTally {
    pub added: usize,
    /// Already in the library.
    pub skipped: usize,
    /// Inspected and found not to be a game.
    pub ignored: usize,
    /// Folders added to the library, which then get scanned.
    pub folders: usize,
    /// The last game added, so the UI can select it.
    pub last_id: Option<i64>,
    pub reasons: Vec<String>,
}

/// Take files and folders dropped onto the window and put them in the library.
///
/// A dropped file is indexed where it lies rather than copied anywhere. It is
/// someone's own ROM in their own place, and quietly duplicating gigabytes
/// into the app data folder to make the bookkeeping tidier is not a trade
/// worth making. The consequence is that the file has to stay where it is,
/// which is the same rule the rest of the library already follows.
///
/// A dropped folder becomes a library folder, because that is plainly what
/// someone means by dropping one, and it is the same thing Settings does.
pub fn add_paths(
    db_handle: &std::sync::Mutex<rusqlite::Connection>,
    paths: &[PathBuf],
) -> Result<DropTally> {
    let mut tally = DropTally::default();

    for path in paths {
        if path.is_dir() {
            let conn = db_handle.lock().unwrap();
            db::add_folder(&conn, &path.to_string_lossy(), None)?;
            tally.folders += 1;
            continue;
        }
        if !path.is_file() {
            continue;
        }

        // Skipped before inspection so a dropped folder of mixed junk does not
        // report a wall of reasons for files that were never candidates.
        // Something with no extension is only worth a look if it is an Xbox
        // 360 package, which is the one kind of game that has none.
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let wanted = if ext.is_empty() {
            consoles::for_boot_file(path).is_some()
        } else {
            platforms::is_indexable_ext(&ext)
        };
        if !wanted {
            tally.ignored += 1;
            tally.reasons.push(format!("not a ROM format (.{ext})"));
            continue;
        }

        let root = path.parent().unwrap_or(path);
        let conn = db_handle.lock().unwrap();
        match index_file(&conn, path, root, None)? {
            Indexed::Added(id) => {
                tally.added += 1;
                tally.last_id = Some(id);
            }
            Indexed::AlreadyKnown => tally.skipped += 1,
            Indexed::NotRom(reason) => {
                tally.ignored += 1;
                tally.reasons.push(reason);
            }
        }
    }

    Ok(tally)
}

/// Walk every library folder and add anything new.
///
/// Takes the mutex rather than a connection so the lock can be released
/// between inserts — a scan of a large library would otherwise block every
/// query the UI makes for its whole duration.
pub fn scan_all(
    app: &AppHandle,
    db_handle: &std::sync::Mutex<rusqlite::Connection>,
) -> Result<ScanTally> {
    let (folders, known) = {
        let conn = db_handle.lock().unwrap();
        (db::list_folders(&conn)?, db::index_state(&conn)?)
    };

    let mut tally = ScanTally::default();
    emit(app, "discovering", 0, 0, "Looking for ROMs…", &tally, false);

    // Pass 1 — discover candidate files.
    let Discovery {
        mut candidates,
        console_games,
    } = discover(&folders);

    // A file dropped onto the window lives outside every library folder, so
    // walking the folders never reaches it again. It is looked at here too,
    // so a rule fixed since it was added applies to it as it does to the rest.
    let app_data = app.try_state::<crate::AppState>().map(|s| s.media_root.clone());
    candidates.extend(dropped_files(&known, &folders, app_data.as_deref()));

    // A scan from before Arcadex knew a console's folder layout listed the
    // folder's data files one by one. They are part of one game, now found.
    let stale = stale_console_files(&known, &console_games);
    if !stale.is_empty() {
        let conn = db_handle.lock().unwrap();
        for (id, reason) in stale {
            db::remove_game(&conn, id)?;
            tally.dropped += 1;
            tally.reasons.push(reason);
        }
    }

    // Pass 2 — check, hash and record.
    let total = candidates.len();

    for (i, (path, root, override_platform)) in candidates.iter().enumerate() {
        let path_str = path.to_string_lossy().to_string();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let console = consoles::for_boot_file(path);

        if let (Some(state), Some(game)) = (known.get(&path_str), &console) {
            // Already listed. One still recorded under its boot file's own
            // name was indexed before its folder could be read - an EBOOT.BIN
            // filed as an unidentified .bin called "EBOOT" - and is put right.
            // Once it has been, its name is the folder's, so a system you
            // pick by hand afterwards is not undone by the next scan.
            let conn = db_handle.lock().unwrap();
            if needs_console_identity(state, game, &filename) {
                let r = console_record(game);
                db::set_console_identity(&conn, state.id, r.platform, &r.filename, &r.title, r.size)?;
                tally.reindexed += 1;
            } else {
                tally.skipped += 1;
            }
            // A serial is read from the folder every time, so a game listed
            // before serials were kept picks one up.
            db::set_serial(&conn, state.id, game.serial.as_deref())?;
        } else if let Some(state) = known.get(&path_str) {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            // An entry the scanner itself added and now recognises as a
            // mistake — an emulator's firmware, an application archive — is
            // dropped from the library. The file on disk is untouched, and
            // anything you have played, favourited or built a hack on is left
            // alone however it now reads: a heuristic is not allowed to throw
            // away something you have shown you care about.
            if !state.precious {
                if let romcheck::Verdict::NotRom(reason) = romcheck::inspect(path, &state.platform)
                {
                    let conn = db_handle.lock().unwrap();
                    db::remove_game(&conn, state.id)?;
                    drop(conn);
                    tally.dropped += 1;
                    tally.reasons.push(reason);
                    if i % 5 == 0 || i + 1 == total {
                        emit(app, "hashing", i + 1, total, &filename, &tally, false);
                    }
                    continue;
                }
            }

            let archived_console = needs_reindex(state, &ext)
                && is_archive(path)
                && consoles::archived_boot_entry(path).is_some();

            if archived_console {
                // A console game kept zipped. Only an entry nobody could
                // identify before is described again, so a system picked by
                // hand stays picked; the names alone say it is one, so the
                // header inside is read only then.
                if state.platform == "unknown" {
                    if let Some(game) = consoles::identify_archive(path) {
                        let r = archived_record(&game, &filename);
                        let conn = db_handle.lock().unwrap();
                        db::set_console_identity(&conn, state.id, r.platform, &r.filename, &r.title, r.size)?;
                        db::set_serial(&conn, state.id, r.serial.as_deref())?;
                        tally.reindexed += 1;
                    } else {
                        tally.skipped += 1;
                    }
                } else {
                    tally.skipped += 1;
                }
            } else if needs_reindex(state, &ext) {
                let platform = detect_platform(path, root, override_platform.as_deref());
                let hashes = hashing::hash_rom(path).ok().flatten();
                let size = hashes
                    .as_ref()
                    .map(|h| h.size as i64)
                    .or_else(|| std::fs::metadata(path).ok().map(|m| m.len() as i64))
                    .unwrap_or(0);
                let inner_name = inner_entry(path, hashes.as_ref());

                let conn = db_handle.lock().unwrap();
                db::reindex_game(
                    &conn,
                    state.id,
                    &platform,
                    size,
                    hashes.as_ref().map(|h| h.crc32.as_str()),
                    hashes.as_ref().map(|h| h.md5.as_str()),
                    hashes.as_ref().map(|h| h.sha1.as_str()),
                    inner_name.as_deref(),
                )?;
                tally.reindexed += 1;
            } else {
                tally.skipped += 1;
            }
        } else {
            let conn = db_handle.lock().unwrap();
            match index_file(&conn, path, root, override_platform.as_deref())? {
                Indexed::Added(_) => tally.added += 1,
                Indexed::AlreadyKnown => tally.skipped += 1,
                Indexed::NotRom(reason) => {
                    drop(conn);
                    tally.ignored += 1;
                    tally.reasons.push(reason);
                    if i % 5 == 0 || i + 1 == total {
                        emit(app, "hashing", i + 1, total, &filename, &tally, false);
                    }
                    continue;
                }
            }
        }

        if i % 5 == 0 || i + 1 == total {
            emit(app, "hashing", i + 1, total, &filename, &tally, false);
        }
    }

    // Now that the library is settled, gather any multi-disc games into one
    // entry each. This runs over the whole library rather than only what was
    // just added, because a set is only a set once its second disc arrives.
    if let Some(state) = app.try_state::<crate::AppState>() {
        let root = state.media_root.join("playlists");
        let conn = db_handle.lock().unwrap();
        match crate::discs::regroup(&conn, &root) {
            Ok(grouped) => tally.disc_sets = grouped.sets,
            // Grouping is a convenience laid over a library that is already
            // correct, so a failure here is not a failure of the scan.
            Err(e) => eprintln!("disc grouping failed: {e}"),
        }
    }

    let message = tally.message();
    emit(app, "done", total, total, &message, &tally, true);

    Ok(tally)
}

/// Library entries outside every library folder that are still there:
/// files that were dropped onto the window. Arcadex's own files - patched
/// hacks, disc playlists - live in the app data folder and are left out.
fn dropped_files(
    known: &std::collections::HashMap<String, db::IndexState>,
    folders: &[LibraryFolder],
    app_data: Option<&Path>,
) -> Vec<(PathBuf, PathBuf, Option<String>)> {
    let roots: Vec<PathBuf> = folders.iter().map(|f| PathBuf::from(&f.path)).collect();
    let mut out: Vec<(PathBuf, PathBuf, Option<String>)> = known
        .keys()
        .map(PathBuf::from)
        .filter(|p| !roots.iter().any(|r| p.starts_with(r)))
        .filter(|p| !app_data.is_some_and(|d| p.starts_with(d)))
        .filter(|p| p.is_file())
        .map(|p| {
            let parent = p.parent().unwrap_or(&p).to_path_buf();
            (p, parent, None)
        })
        .collect();
    out.sort();
    out
}

/// What walking the library folders turned up.
struct Discovery {
    /// Files worth indexing, each with the library folder it was found under
    /// and that folder's assigned system.
    candidates: Vec<(PathBuf, PathBuf, Option<String>)>,
    /// Folders that are one console game each. Their boot files are among
    /// the candidates; nothing else beneath them is.
    console_games: Vec<ConsoleGame>,
}

/// Walk every library folder for things to index.
fn discover(folders: &[LibraryFolder]) -> Discovery {
    let mut candidates: Vec<(PathBuf, PathBuf, Option<String>)> = Vec::new();
    let mut console_games: Vec<ConsoleGame> = Vec::new();

    for folder in folders {
        let root = PathBuf::from(&folder.path);
        if !root.exists() {
            continue;
        }
        let mut found: Vec<ConsoleGame> = Vec::new();
        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let hidden = e
                    .file_name()
                    .to_str()
                    .map(|s| s.starts_with('.') || s.eq_ignore_ascii_case("$RECYCLE.BIN"))
                    .unwrap_or(false);
                if hidden {
                    return false;
                }
                // A console game's own folder is one game, and nothing under
                // it is a second: a PlayStation 3 game's USRDIR is thousands
                // of data files, plenty of them called something.bin. This
                // applies to the library root too, so adding a single game's
                // folder works.
                if e.file_type().is_dir() {
                    if let Some(game) = consoles::identify_dir(e.path()) {
                        found.push(game);
                        return false;
                    }
                }
                // An emulator unpacked into the library is not a ROM folder.
                // Skipping it at its root drops its Sys and data directories
                // with it, which is where the firmware and fonts that read as
                // ROMs actually live. Never applied to the library root
                // itself, which the user chose deliberately.
                if e.depth() > 0 && e.file_type().is_dir() && romcheck::holds_a_program(e.path()) {
                    return false;
                }
                true
            })
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            // An Xbox 360 package has no extension at all, so it is known by
            // the folder it is filed in and then by its header.
            let wanted = if ext.is_empty() {
                consoles::could_be_package(path)
            } else {
                platforms::is_indexable_ext(&ext)
            };
            if !wanted {
                continue;
            }
            candidates.push((
                path.to_path_buf(),
                root.clone(),
                folder.platform_override.clone(),
            ));
        }

        for game in &found {
            if let Some(boot) = &game.boot {
                candidates.push((boot.clone(), root.clone(), folder.platform_override.clone()));
            }
        }
        console_games.extend(found);
    }

    // A disc kept unzipped is a playlist plus its tracks. Index the playlist,
    // not the pieces.
    drop_disc_tracks(&mut candidates);

    Discovery {
        candidates,
        console_games,
    }
}

/// Drop library entries whose file is gone. Does not delete anything on disk.
///
/// Only what `db::missing_entries` finds gone: a game on a drive that is not
/// plugged in is kept. Each goes the way removing it by hand would, so hacks
/// made from it stay.
pub fn remove_missing(conn: &rusqlite::Connection) -> Result<usize> {
    let (missing, _) = db::missing_entries(conn)?;
    for id in &missing {
        db::remove_game(conn, *id)?;
    }
    Ok(missing.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dropping a ROM on the window indexes it where it lies, and dropping a
    /// folder makes it a library folder.
    #[test]
    fn drops_index_files_and_adopt_folders() {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-drop-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let conn = db::open(&dir.join("library.db")).unwrap();
        let db_handle = std::sync::Mutex::new(conn);

        // A real-enough NES ROM: iNES header, past the size floor.
        let rom = dir.join("Super Mario Bros. (World).nes");
        let mut bytes = b"NES".to_vec();
        bytes.push(0x1a);
        bytes.resize(40_976, 0);
        std::fs::write(&rom, &bytes).unwrap();

        // Something that is plainly not a game.
        let junk = dir.join("box-scan.png");
        let mut png = vec![0x89u8, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.resize(9000, 0);
        std::fs::write(&junk, &png).unwrap();

        // And a folder to adopt.
        let folder = dir.join("More ROMs");
        std::fs::create_dir_all(&folder).unwrap();

        let tally = add_paths(
            &db_handle,
            &[rom.clone(), junk.clone(), folder.clone()],
        )
        .unwrap();

        assert_eq!(tally.added, 1, "the ROM should be added");
        assert_eq!(tally.folders, 1, "the folder should be adopted");
        assert_eq!(tally.ignored, 1, "the PNG should be rejected");
        assert!(tally.last_id.is_some());

        {
            let conn = db_handle.lock().unwrap();
            let game = db::get_game(&conn, tally.last_id.unwrap()).unwrap().unwrap();
            // Indexed in place: the path is where the user left it.
            assert_eq!(game.path, rom.to_string_lossy());
            assert_eq!(game.platform, "nes");
            // clean_title drops the region and the trailing full stop.
            assert_eq!(game.title, "Super Mario Bros");
            assert!(game.crc32.is_some(), "it should have been hashed");

            // The folder really joined the library.
            let folders = db::list_folders(&conn).unwrap();
            assert!(folders.iter().any(|f| f.path == folder.to_string_lossy()));
        }

        // Dropping the same ROM again is recognised rather than duplicated.
        let again = add_paths(&db_handle, &[rom]).unwrap();
        assert_eq!(again.added, 0);
        assert_eq!(again.skipped, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-{}-{}-{:?}",
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

    fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// A disc kept unzipped is a playlist and its tracks. That is one game.
    #[test]
    fn a_disc_folder_is_one_game_not_four() {
        let root = scratch("discfolder");
        let game = root.join("Final Fantasy VII");
        let big = vec![0u8; 60_000];

        put(
            &game.join("Final Fantasy VII.cue"),
            b"FILE \"Final Fantasy VII (Track 1).bin\" BINARY
  TRACK 01 MODE2/2352
",
        );
        for n in 1..=3 {
            put(&game.join(format!("Final Fantasy VII (Track {n}).bin")), &big);
        }

        let mut candidates: Vec<(PathBuf, PathBuf, Option<String>)> = std::fs::read_dir(&game)
            .unwrap()
            .flatten()
            .map(|e| (e.path(), root.clone(), None))
            .collect();
        assert_eq!(candidates.len(), 4);

        drop_disc_tracks(&mut candidates);

        assert_eq!(candidates.len(), 1, "the cue speaks for its tracks");
        assert_eq!(
            candidates[0].0.extension().unwrap(),
            "cue",
            "and the cue is what survives, since it is what an emulator opens"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A cue sheet is a couple of hundred bytes. The size floor used to throw
    /// it out and keep its tracks, which is exactly backwards.
    #[test]
    fn a_playlist_is_not_too_small_to_be_a_game() {
        let root = scratch("playlist");
        let cue = root.join("Some Game.cue");
        put(&cue, b"FILE \"Some Game.bin\" BINARY
");
        assert!(std::fs::metadata(&cue).unwrap().len() < 100);
        assert_eq!(romcheck::inspect(&cue, "ps1"), romcheck::Verdict::Rom);

        // Empty is still nothing.
        let empty = root.join("Empty.cue");
        put(&empty, b"");
        assert!(matches!(
            romcheck::inspect(&empty, "ps1"),
            romcheck::Verdict::NotRom(_)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A playlist says nothing about itself, so the system comes from the
    /// track it points at.
    #[test]
    fn a_playlist_takes_its_system_from_its_track() {
        let root = scratch("gditrack");
        let game = root.join("Sonic Adventure");

        // A Dreamcast data track names the console at 0x10.
        let mut track = vec![0u8; 0x300];
        track[0x10..0x10 + 15].copy_from_slice(b"SEGA SEGAKATANA");
        put(&game.join("track03.bin"), &track);

        let cue = game.join("Sonic Adventure.cue");
        put(&cue, b"FILE \"track03.bin\" BINARY
");

        assert_eq!(detect_platform(&cue, &root, None), "dreamcast");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A bare `.bin` with no cue sheet beside it. Nothing in its name or its
    /// folder says PlayStation, and `.bin` belongs to half a dozen systems,
    /// so before the raw-track reader this landed in Unidentified.
    #[test]
    fn a_bare_bin_track_is_read_as_the_disc_it_is() {
        let root = scratch("barebin");

        // A minimal ISO 9660 filesystem, wrapped as a raw Mode 1 CD track.
        const SEC: usize = 2048;
        let mut iso = vec![0u8; SEC * 20];
        iso[16 * SEC] = 1;
        iso[16 * SEC + 1..16 * SEC + 6].copy_from_slice(b"CD001");
        let r = 16 * SEC + 156;
        iso[r] = 34;
        iso[r + 2..r + 6].copy_from_slice(&17u32.to_le_bytes());
        iso[r + 10..r + 14].copy_from_slice(&(SEC as u32).to_le_bytes());
        let name = b"SYSTEM.CNF;1";
        let rec_len = 33 + name.len() + 1;
        let mut rec = vec![0u8; rec_len];
        rec[0] = rec_len as u8;
        rec[2..6].copy_from_slice(&18u32.to_le_bytes());
        rec[10..14].copy_from_slice(&37u32.to_le_bytes());
        rec[32] = name.len() as u8;
        rec[33..33 + name.len()].copy_from_slice(name);
        iso[17 * SEC..17 * SEC + rec.len()].copy_from_slice(&rec);
        iso[18 * SEC..18 * SEC + 37]
            .copy_from_slice(b"BOOT2 = cdrom0:SLUS_203.12;1 VER=1.00");

        let mut raw = Vec::new();
        for chunk in iso.chunks(SEC) {
            let mut sector = vec![0u8; 2352];
            sector[16..16 + chunk.len()].copy_from_slice(chunk);
            raw.extend_from_slice(&sector);
        }

        let bin = root.join("Sonic Riders.bin");
        put(&bin, &raw);
        assert_eq!(detect_platform(&bin, &root, None), "ps2");
        let _ = std::fs::remove_dir_all(&root);
    }

    fn library(root: &Path) -> Vec<LibraryFolder> {
        vec![LibraryFolder {
            id: 1,
            path: root.to_string_lossy().to_string(),
            platform_override: None,
            added_at: 0,
        }]
    }

    fn ps3_folder(game: &Path, serial: &str) -> PathBuf {
        put(
            &game.join("PS3_GAME").join("PARAM.SFO"),
            &crate::consoles::tests::sfo(&[
                ("CATEGORY", "DG"),
                ("TITLE", "Demon's Souls"),
                ("TITLE_ID", serial),
            ]),
        );
        let usrdir = game.join("PS3_GAME").join("USRDIR");
        let eboot = usrdir.join("EBOOT.BIN");
        put(&eboot, &vec![0u8; 5000]);
        for n in 0..5 {
            put(&usrdir.join(format!("data{n}.bin")), &vec![1u8; 5000]);
        }
        eboot
    }

    /// A PlayStation 3 game kept as a folder is one game, named the way the
    /// console names it, and none of its data files are games of their own.
    #[test]
    fn a_console_game_folder_is_one_game() {
        let root = scratch("consolefolder");
        let eboot = ps3_folder(&root.join("PS3").join("Demon's Souls (USA)"), "BLUS30443");
        // An ordinary ROM beside it is found as it always was.
        let rom = root.join("PS3").join("Tetris.gb");
        put(&rom, &vec![0u8; 5000]);

        let found = discover(&library(&root));
        let paths: Vec<&PathBuf> = found.candidates.iter().map(|c| &c.0).collect();
        assert!(paths.contains(&&eboot), "the boot file is the game");
        assert!(paths.contains(&&rom));
        assert_eq!(paths.len(), 2, "and nothing in USRDIR is a second game: {paths:?}");
        assert_eq!(found.console_games.len(), 1);

        let conn = db::open(&root.join("library.db")).unwrap();
        let Indexed::Added(id) = index_file(&conn, &eboot, &root, None).unwrap() else {
            panic!("should be added");
        };
        let game = db::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(game.platform, "ps3");
        assert_eq!(game.title, "Demon's Souls", "named from PARAM.SFO, not EBOOT");
        assert_eq!(game.filename, "Demon's Souls (USA)");
        assert_eq!(game.region.as_deref(), Some("USA"));
        assert!(game.size >= 6 * 5000, "the whole folder, not the executable");
        assert!(game.crc32.is_none(), "an executable's checksum matches nothing");
        assert_eq!(game.serial.as_deref(), Some("BLUS30443"), "the serial is what there is instead");

        drop(conn);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// What a scan from before Arcadex knew the PlayStation 3 left behind:
    /// the EBOOT as an unidentified game called "EBOOT", and every data file
    /// beside it as another. The data files go, unless you have played one,
    /// and the EBOOT is put right.
    #[test]
    fn an_older_scans_console_leftovers_are_cleaned_up() {
        let root = scratch("consolestale");
        let eboot = ps3_folder(&root.join("BLUS30443"), "BLUS30443");
        let usrdir = eboot.parent().unwrap();

        let conn = db::open(&root.join("library.db")).unwrap();
        for (path, seconds) in [
            (eboot.clone(), 0),
            (usrdir.join("data0.bin"), 0),
            (usrdir.join("data1.bin"), 600),
        ] {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            conn.execute(
                "INSERT INTO games (path, filename, platform, title, size, play_seconds, added_at)
                 VALUES (?1, ?2, 'unknown', ?3, 5000, ?4, 0)",
                rusqlite::params![path.to_string_lossy(), name, clean_title(&name), seconds],
            )
            .unwrap();
        }

        let known = db::index_state(&conn).unwrap();
        let found = discover(&library(&root));
        let stale = stale_console_files(&known, &found.console_games);
        assert_eq!(stale.len(), 1, "one data file; the played one and the EBOOT stay");
        let data0 = &known[&usrdir.join("data0.bin").to_string_lossy().to_string()];
        assert_eq!(stale[0].0, data0.id);
        assert_eq!(stale[0].1, "part of a PlayStation 3 game's folder");

        // The EBOOT is described again from its folder.
        let state = &known[&eboot.to_string_lossy().to_string()];
        let game = consoles::for_boot_file(&eboot).unwrap();
        assert!(needs_console_identity(state, &game, "EBOOT.BIN"));
        let r = console_record(&game);
        db::set_console_identity(&conn, state.id, r.platform, &r.filename, &r.title, r.size).unwrap();
        let after = db::get_game(&conn, state.id).unwrap().unwrap();
        assert_eq!(after.platform, "ps3");
        assert_eq!(after.title, "Demon's Souls");
        // The folder is only the serial, so the title stands in for its name.
        assert_eq!(after.filename, "Demon's Souls");

        // Moved to another system by hand afterwards, it stays there: the
        // row no longer looks like something an older scan left behind.
        db::set_platform(&conn, state.id, "ps4").unwrap();
        let known = db::index_state(&conn).unwrap();
        let state = &known[&eboot.to_string_lossy().to_string()];
        assert!(!needs_console_identity(state, &game, "EBOOT.BIN"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An Xbox 360 Games on Demand package has no extension. It is found by
    /// the folder it is filed in and identified by its header.
    #[test]
    fn finds_a_games_on_demand_package() {
        let root = scratch("godscan");
        let content = root.join("4D5307E6").join("00007000");
        let package = content.join("7D0B6B8C2C8A1E3F4B2D");
        put(&package, &crate::consoles::tests::package(0x7000, "Halo 3"));
        put(&content.join("7D0B6B8C2C8A1E3F4B2D.data").join("Data0000"), &[0u8; 64]);

        let found = discover(&library(&root));
        let paths: Vec<&PathBuf> = found.candidates.iter().map(|c| &c.0).collect();
        assert_eq!(paths, vec![&package]);

        let conn = db::open(&root.join("library.db")).unwrap();
        let Indexed::Added(id) = index_file(&conn, &package, &root, None).unwrap() else {
            panic!("should be added");
        };
        let game = db::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(game.platform, "xbox360");
        assert_eq!(game.title, "Halo 3");

        drop(conn);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A Games on Demand package zipped up whole: one game, named from the
    /// header inside, sized by everything in the archive.
    #[test]
    fn indexes_a_zipped_package_as_the_game_inside() {
        let root = scratch("zippedgod");
        let zip = root.join("Skate 3.zip");
        crate::consoles::tests::zipped_package(&zip);

        let conn = db::open(&root.join("library.db")).unwrap();
        let Indexed::Added(id) = index_file(&conn, &zip, &root, None).unwrap() else {
            panic!("should be added");
        };
        let game = db::get_game(&conn, id).unwrap().unwrap();
        assert_eq!(game.platform, "xbox360");
        assert_eq!(game.title, "Skate 3");
        assert_eq!(game.serial.as_deref(), Some("4D5307E6"));
        assert_eq!(game.size, 0xB000 + 3000);
        assert_eq!(game.filename, "Skate 3.zip", "the file in the library is the zip");
        drop(conn);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The case this was written for: a zipped package added before Arcadex
    /// could read one, sitting in the library as Unidentified.
    #[test]
    fn puts_right_a_zipped_package_an_older_scan_left_unidentified() {
        let root = scratch("zippedold");
        let zip = root.join("Skate 3.zip");
        crate::consoles::tests::zipped_package(&zip);
        let conn = db::open(&root.join("library.db")).unwrap();
        conn.execute(
            "INSERT INTO games (path, filename, platform, title, size, added_at)
             VALUES (?1, 'Skate 3.zip', 'unknown', 'Skate 3', 5870520658, 0)",
            [zip.to_string_lossy()],
        )
        .unwrap();
        let known = db::index_state(&conn).unwrap();
        let state = &known[&zip.to_string_lossy().to_string()];

        // What the scan checks, in the order it checks it.
        assert!(needs_reindex(state, "zip"));
        assert!(consoles::archived_boot_entry(&zip).is_some());
        let game = consoles::identify_archive(&zip).unwrap();
        let r = archived_record(&game, "Skate 3.zip");
        db::set_console_identity(&conn, state.id, r.platform, &r.filename, &r.title, r.size).unwrap();

        let after = db::get_game(&conn, state.id).unwrap().unwrap();
        assert_eq!(after.platform, "xbox360");
        assert_eq!(after.filename, "Skate 3.zip");
        drop(conn);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file dropped onto the window, outside every library folder, is
    /// looked at again by a scan. Arcadex's own files are not.
    #[test]
    fn a_scan_looks_again_at_files_dropped_on_the_window() {
        let root = scratch("dropped");
        let library_dir = root.join("Library");
        let in_library = library_dir.join("a.nes");
        let dropped = root.join("Desktop").join("Skate 3.zip");
        let ours = root.join("AppData").join("hacks").join("1").join("hack.sfc");
        let gone = root.join("Desktop").join("deleted.zip");
        for p in [&in_library, &dropped, &ours] {
            put(p, b"x");
        }

        let state = |id| db::IndexState {
            id,
            platform: "unknown".into(),
            inner_name: None,
            filename: String::new(),
            precious: false,
        };
        let known: std::collections::HashMap<String, db::IndexState> = [&in_library, &dropped, &ours, &gone]
            .into_iter()
            .enumerate()
            .map(|(i, p)| (p.to_string_lossy().to_string(), state(i as i64)))
            .collect();

        let found = dropped_files(&known, &library(&library_dir), Some(&root.join("AppData")));
        let paths: Vec<&PathBuf> = found.iter().map(|c| &c.0).collect();
        assert_eq!(paths, vec![&dropped]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A disc image zipped up names its system in its first sectors, the same
    /// as an unzipped one; nothing in the zip's or the image's name says it.
    #[test]
    fn a_zipped_disc_image_is_read_like_an_unzipped_one() {
        let root = scratch("zippediso");
        const SEC: usize = 2048;
        let mut iso = vec![0u8; SEC * 20];
        iso[16 * SEC] = 1;
        iso[16 * SEC + 1..16 * SEC + 6].copy_from_slice(b"CD001");
        let r = 16 * SEC + 156;
        iso[r] = 34;
        iso[r + 2..r + 6].copy_from_slice(&17u32.to_le_bytes());
        iso[r + 10..r + 14].copy_from_slice(&(SEC as u32).to_le_bytes());
        let name = b"SYSTEM.CNF;1";
        let mut rec = vec![0u8; 33 + name.len() + 1];
        rec[0] = rec.len() as u8;
        rec[2..6].copy_from_slice(&18u32.to_le_bytes());
        rec[10..14].copy_from_slice(&37u32.to_le_bytes());
        rec[32] = name.len() as u8;
        rec[33..33 + name.len()].copy_from_slice(name);
        iso[17 * SEC..17 * SEC + rec.len()].copy_from_slice(&rec);
        iso[18 * SEC..18 * SEC + 37].copy_from_slice(b"BOOT2 = cdrom0:SLUS_203.12;1 VER=1.00");

        let zip = root.join("Some Game.zip");
        crate::consoles::tests::zip_of(&zip, &[("Some Game.iso", iso)]);
        assert_eq!(detect_platform(&zip, &root, None), "ps2");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn folder_assignment_beats_everything() {
        let root = Path::new("C:/roms");
        let p = Path::new("C:/roms/Wii/Mario Kart Wii.iso");
        assert_eq!(detect_platform(p, root, Some("gamecube")), "gamecube");
    }

    #[test]
    fn a_unique_extension_settles_it() {
        let root = Path::new("C:/roms");
        let p = Path::new("C:/roms/anything/Super Metroid.sfc");
        assert_eq!(detect_platform(p, root, None), "snes");
    }

    #[test]
    fn a_directory_named_after_a_system_wins_over_the_title() {
        let root = Path::new("C:/roms");
        // The file says Wii; the folder the user made says GameCube. Their
        // own filing is the better evidence.
        let p = Path::new("C:/roms/GameCube/Wii Play.iso");
        assert_eq!(detect_platform(p, root, None), "gamecube");
    }

    /// The case this was written for: a 7z belongs to no system by extension,
    /// sits in a folder named nothing in particular, and the only thing left
    /// saying "Wii" is the filename.
    #[test]
    fn falls_back_to_the_roms_own_name() {
        let root = Path::new("C:/roms");
        let p = Path::new("C:/roms/Retro Folders/New Super Mario Bros Wii [SMNE01].7z");
        assert_eq!(detect_platform(p, root, None), "wii");
    }

    /// An ambiguous extension that used to be guessed at from folder names
    /// is now settled by the file's own header.
    #[test]
    fn bytes_beat_the_folder_and_the_filename() {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-sig-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // A folder named after the wrong system, on purpose.
        let wrong = dir.join("PlayStation");
        std::fs::create_dir_all(&wrong).unwrap();

        // A disc image with the Wii magic where a Wii disc keeps it.
        let iso = wrong.join("Some Disc.iso");
        let mut bytes = vec![0u8; 0x300];
        bytes[0x18..0x1c].copy_from_slice(&[0x5D, 0x1C, 0x9E, 0xA3]);
        std::fs::write(&iso, &bytes).unwrap();

        assert_eq!(
            detect_platform(&iso, &dir, None),
            "wii",
            "the header should win over a folder called PlayStation"
        );

        // The same file with nothing recognisable in it falls back to the
        // folder name, which is the behaviour that used to be all there was.
        let blank = wrong.join("Mystery.iso");
        std::fs::write(&blank, vec![0u8; 0x300]).unwrap();
        assert_eq!(detect_platform(&blank, &dir, None), "ps1");

        // And a folder assignment still outranks everything.
        assert_eq!(detect_platform(&iso, &dir, Some("gamecube")), "gamecube");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_nameless_container_is_still_unidentified() {
        let root = Path::new("C:/roms");
        let p = Path::new("C:/roms/Retro Folders/Some Game.7z");
        assert_eq!(detect_platform(p, root, None), "unknown");
    }

    #[test]
    fn an_ambiguous_extension_takes_its_first_candidate() {
        let root = Path::new("C:/roms");
        // rvz is shared by GameCube and Wii; with nothing else to go on it
        // lands on the first rather than being dropped.
        let p = Path::new("C:/roms/discs/Some Game.rvz");
        assert_eq!(detect_platform(p, root, None), "gamecube");
        // ...but the name is enough to tell them apart.
        let p = Path::new("C:/roms/discs/Wii Sports Resort.rvz");
        assert_eq!(detect_platform(p, root, None), "wii");
    }
}



