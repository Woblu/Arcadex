//! SQLite-backed library storage.

use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::Mutex;

use crate::error::Result;
use crate::models::*;
use crate::platforms;

pub struct Db(pub std::sync::Arc<Mutex<Connection>>);

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS folders (
    id                INTEGER PRIMARY KEY,
    path              TEXT    NOT NULL UNIQUE,
    platform_override TEXT,
    added_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS games (
    id              INTEGER PRIMARY KEY,
    path            TEXT    NOT NULL UNIQUE,
    filename        TEXT    NOT NULL,
    platform        TEXT    NOT NULL DEFAULT 'unknown',
    size            INTEGER NOT NULL DEFAULT 0,
    crc32           TEXT,
    md5             TEXT,
    sha1            TEXT,
    inner_name      TEXT,
    title           TEXT    NOT NULL,
    description     TEXT,
    developer       TEXT,
    publisher       TEXT,
    genre           TEXT,
    release_date    TEXT,
    players         TEXT,
    rating          REAL,
    region          TEXT,
    cover_path      TEXT,
    screenshot_path TEXT,
    logo_path       TEXT,
    scrape_status   TEXT    NOT NULL DEFAULT 'pending',
    scrape_source   TEXT,
    favorite        INTEGER NOT NULL DEFAULT 0,
    hidden          INTEGER NOT NULL DEFAULT 0,
    play_count      INTEGER NOT NULL DEFAULT 0,
    play_seconds    INTEGER NOT NULL DEFAULT 0,
    last_played     INTEGER,
    added_at        INTEGER NOT NULL,
    base_game_id    INTEGER REFERENCES games(id) ON DELETE CASCADE,
    patch_path      TEXT,
    patch_format    TEXT
);

CREATE INDEX IF NOT EXISTS idx_games_platform ON games(platform);
CREATE INDEX IF NOT EXISTS idx_games_title    ON games(title);
CREATE INDEX IF NOT EXISTS idx_games_status   ON games(scrape_status);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS emulators (
    platform       TEXT PRIMARY KEY,
    mode           TEXT NOT NULL DEFAULT 'retroarch',
    core           TEXT,
    custom_command TEXT
);

CREATE TABLE IF NOT EXISTS patches (
    id          INTEGER PRIMARY KEY,
    path        TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    format      TEXT    NOT NULL,
    source_crc  TEXT,
    system_hint TEXT,
    target_hint TEXT,
    origin      TEXT,
    added_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_patches_crc  ON patches(source_crc);
CREATE INDEX IF NOT EXISTS idx_patches_name ON patches(name);

CREATE TABLE IF NOT EXISTS cheats (
    id          INTEGER PRIMARY KEY,
    game_id     INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    description TEXT    NOT NULL,
    code        TEXT    NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 0,
    UNIQUE(game_id, idx)
);

-- Known-good dumps from No-Intro and Redump DATs. Keyed by checksum so
-- importing the same DAT twice catalogues nothing new.
CREATE TABLE IF NOT EXISTS dat_roms (
    id       INTEGER PRIMARY KEY,
    dat_name TEXT    NOT NULL,
    name     TEXT    NOT NULL,
    size     INTEGER NOT NULL DEFAULT 0,
    crc32    TEXT,
    md5      TEXT,
    sha1     TEXT
);

-- Not a plain UNIQUE(...) on the columns: SQLite treats NULLs as distinct in a
-- unique constraint, so a DAT entry with no MD5 would be inserted afresh on
-- every import. Folding NULL to an empty string makes "the same dump" mean the
-- same thing every time.
CREATE UNIQUE INDEX IF NOT EXISTS dat_roms_identity ON dat_roms(
    name,
    COALESCE(crc32, ''),
    COALESCE(md5, ''),
    COALESCE(sha1, '')
);

CREATE INDEX IF NOT EXISTS dat_roms_sha1 ON dat_roms(sha1);
CREATE INDEX IF NOT EXISTS dat_roms_md5  ON dat_roms(md5);
CREATE INDEX IF NOT EXISTS dat_roms_crc  ON dat_roms(crc32);

CREATE TABLE IF NOT EXISTS play_sessions (
    id         INTEGER PRIMARY KEY,
    game_id    INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    seconds    INTEGER NOT NULL DEFAULT 0
);

-- What each game's saves last synced as: the fingerprint of this machine's
-- files afterwards, and the version in the sync folder they matched.
CREATE TABLE IF NOT EXISTS save_sync (
    game_id   INTEGER PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    digest    TEXT NOT NULL,
    version   TEXT NOT NULL,
    synced_at INTEGER NOT NULL
);
"#;

const GAME_COLS: &str = "id, path, filename, platform, size, crc32, md5, sha1, inner_name, \
     title, description, developer, publisher, genre, release_date, players, rating, region, \
     cover_path, screenshot_path, logo_path, scrape_status, scrape_source, favorite, hidden, \
     play_count, play_seconds, last_played, added_at, base_game_id, patch_path, cover_custom, title_custom,      serial";

/// Repoint stored paths after the app data directory has moved.
///
/// Artwork, imported patches and the ROMs produced by applying one are all
/// recorded as absolute paths into the app data directory. That directory is
/// named after the bundle identifier, so renaming the app moves it and every
/// one of those paths goes stale at once: the files are all still there under
/// the new name, but nothing in the library points at them any more.
///
/// This rewrites the directory segment in place, wherever it appears as a
/// whole folder name: between two separators, or at the very end. A name that
/// merely starts the same - `com.boazv.playdex.verify` is not
/// `com.boazv.playdex` - is a different folder and is left alone. Matching is
/// case-sensitive, so what is counted is what was changed.
///
/// It sweeps every column of every table rather than a fixed list, because
/// the set of columns holding such a path has grown before, and the match
/// guard means a column that never held one is never written to. Values that
/// do not mention the old directory are left exactly as they were, which is
/// what makes it safe to run against a library with nothing to fix.
///
/// Idempotent: once it has run nothing matches, and it costs a single query.
/// Returns how many values were rewritten.
pub fn repoint_data_dir(conn: &Connection, old_dir: &str, new_dir: &str) -> Result<usize> {
    if old_dir == new_dir || old_dir.is_empty() {
        return Ok(0);
    }
    let segments: Vec<(String, String)> = ["\\", "/"]
        .iter()
        .map(|sep| (format!("{sep}{old_dir}"), format!("{sep}{new_dir}")))
        .collect();
    // Within a path, the folder name has a separator on both sides; at the
    // end of one, only before it.
    let within = |col: &str| format!("instr({col}, ?1 || substr(?1, 1, 1)) > 0");
    let at_end = |col: &str| format!("substr({col}, -length(?1)) = ?1");

    // The cheap check first, so an ordinary launch costs one query rather than
    // a scan of every column in the database. If it cannot be answered the
    // sweep runs anyway, since being slow once beats leaving a library broken.
    let stale = segments.iter().any(|(old, _)| {
        let any_of = ["cover_path", "screenshot_path", "logo_path", "path"]
            .iter()
            .map(|c| format!("{} OR {}", within(c), at_end(c)))
            .collect::<Vec<_>>()
            .join(" OR ");
        conn.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM games WHERE {any_of})"),
            params![old],
            |row| row.get(0),
        )
        .unwrap_or(true)
    });
    if !stale {
        return Ok(0);
    }

    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;

    let mut rewritten = 0usize;
    for table in tables {
        let columns: Vec<String> = conn
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<String>>>()?;

        for column in columns {
            let col = format!("\"{column}\"");
            for (old, new) in &segments {
                // ?1 is `\old` or `/old`; with its own separator appended it
                // is the folder in the middle of a path.
                let middle = format!(
                    "UPDATE \"{table}\" SET {col} = replace({col}, ?1 || substr(?1, 1, 1), \
                     ?2 || substr(?2, 1, 1)) WHERE {}",
                    within(&col)
                );
                rewritten += conn.execute(&middle, params![old, new])?;
                let end = format!(
                    "UPDATE \"{table}\" SET {col} = substr({col}, 1, length({col}) - length(?1)) || ?2 \
                     WHERE {}",
                    at_end(&col)
                );
                rewritten += conn.execute(&end, params![old, new])?;
            }
        }
    }

    Ok(rewritten)
}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    migrate(&conn)?;
    Ok(conn)
}

/// Add columns introduced after a user's database was first created.
fn migrate(conn: &Connection) -> Result<()> {
    for (column, decl) in [
        ("base_game_id", "INTEGER REFERENCES games(id) ON DELETE CASCADE"),
        ("patch_path", "TEXT"),
        ("patch_format", "TEXT"),
        // A per-game emulator, overriding the one set for its system.
        ("emu_mode", "TEXT"),
        ("emu_core", "TEXT"),
        ("emu_command", "TEXT"),
        // Multi-disc grouping: which playlist a disc belongs to, and whether
        // this row is the playlist standing in for a set of them.
        ("disc_playlist_id", "INTEGER"),
        ("is_disc_playlist", "INTEGER NOT NULL DEFAULT 0"),
        // Set when the cover was chosen by hand rather than fetched, which is
        // what stops a later metadata fetch replacing it.
        ("cover_custom", "INTEGER NOT NULL DEFAULT 0"),
        // Set when the title was typed by hand. A provider that later matches
        // the game would otherwise replace a name somebody chose on purpose.
        ("title_custom", "INTEGER NOT NULL DEFAULT 0"),
        // The disc or catalogue number a console game carries in its own
        // metadata - BLUS30443, CUSA00003 - for the games too large to have
        // a checksum to look up.
        ("serial", "TEXT"),
    ] {
        let mut stmt = conn.prepare("PRAGMA table_info(games)")?;
        let existing: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if !existing.iter().any(|c| c == column) {
            conn.execute(&format!("ALTER TABLE games ADD COLUMN {column} {decl}"), [])?;
        }
    }

    // Which machine a session was played on, for sessions read in from a
    // sync folder. Empty for the ones played here. A session read twice is
    // the same machine starting the same game at the same second, and the
    // index is what makes the second read add nothing.
    let has_origin: bool = conn
        .prepare("PRAGMA table_info(play_sessions)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|c| c == "origin");
    if !has_origin {
        conn.execute("ALTER TABLE play_sessions ADD COLUMN origin TEXT", [])?;
    }
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS play_sessions_origin
         ON play_sessions(origin, game_id, started_at) WHERE origin IS NOT NULL",
        [],
    )?;
    Ok(())
}

/// Record a patched ROM as its own game, linked back to the ROM it came from.
#[allow(clippy::too_many_arguments)]
pub fn insert_hack(
    conn: &Connection,
    path: &str,
    filename: &str,
    platform: &str,
    title: &str,
    size: i64,
    crc32: Option<&str>,
    base_game_id: i64,
    patch_path: &str,
    patch_format: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO games
         (path, filename, platform, title, size, crc32, added_at,
          base_game_id, patch_path, patch_format)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            path, filename, platform, title, size, crc32, now(),
            base_game_id, patch_path, patch_format
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_game(row: &Row) -> rusqlite::Result<Game> {
    Ok(Game {
        id: row.get(0)?,
        path: row.get(1)?,
        filename: row.get(2)?,
        platform: row.get(3)?,
        size: row.get(4)?,
        crc32: row.get(5)?,
        md5: row.get(6)?,
        sha1: row.get(7)?,
        inner_name: row.get(8)?,
        title: row.get(9)?,
        description: row.get(10)?,
        developer: row.get(11)?,
        publisher: row.get(12)?,
        genre: row.get(13)?,
        release_date: row.get(14)?,
        players: row.get(15)?,
        rating: row.get(16)?,
        region: row.get(17)?,
        cover_path: row.get(18)?,
        screenshot_path: row.get(19)?,
        logo_path: row.get(20)?,
        scrape_status: row.get(21)?,
        scrape_source: row.get(22)?,
        favorite: row.get::<_, i64>(23)? != 0,
        hidden: row.get::<_, i64>(24)? != 0,
        play_count: row.get(25)?,
        play_seconds: row.get(26)?,
        last_played: row.get(27)?,
        added_at: row.get(28)?,
        base_game_id: row.get(29)?,
        patch_path: row.get(30)?,
        cover_custom: row.get::<_, i64>(31)? != 0,
        title_custom: row.get::<_, i64>(32)? != 0,
        serial: row.get(33)?,
    })
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- games

/// Insert a newly scanned ROM. Returns false when the path was already known.
#[allow(clippy::too_many_arguments)]
pub fn insert_game(
    conn: &Connection,
    path: &str,
    filename: &str,
    platform: &str,
    title: &str,
    region: Option<&str>,
    size: i64,
    crc32: Option<&str>,
    md5: Option<&str>,
    sha1: Option<&str>,
    inner_name: Option<&str>,
) -> Result<bool> {
    let changed = conn.execute(
        "INSERT OR IGNORE INTO games
         (path, filename, platform, title, region, size, crc32, md5, sha1, inner_name, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            path,
            filename,
            platform,
            title,
            region,
            size,
            crc32,
            md5,
            sha1,
            inner_name,
            now()
        ],
    )?;
    Ok(changed > 0)
}

pub fn list_games(conn: &Connection, filter: &GameFilter) -> Result<Vec<Game>> {
    let mut sql = format!("SELECT {GAME_COLS} FROM games WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if !filter.show_hidden.unwrap_or(false) {
        sql.push_str(" AND hidden = 0");
    }
    if let Some(p) = filter.platform.as_deref().filter(|p| !p.is_empty()) {
        sql.push_str(" AND platform = ?");
        args.push(Box::new(p.to_string()));
    }
    if let Some(q) = filter.search.as_deref().filter(|q| !q.trim().is_empty()) {
        sql.push_str(" AND (title LIKE ? OR filename LIKE ?)");
        let like = format!("%{}%", q.trim());
        args.push(Box::new(like.clone()));
        args.push(Box::new(like));
    }
    if filter.favorites_only.unwrap_or(false) {
        sql.push_str(" AND favorite = 1");
    }
    if filter.unscraped_only.unwrap_or(false) {
        sql.push_str(" AND scrape_status != 'ok'");
    }

    sql.push_str(match filter.sort.as_deref() {
        Some("recent") => " ORDER BY last_played IS NULL, last_played DESC, title COLLATE NOCASE",
        Some("played") => " ORDER BY play_seconds DESC, title COLLATE NOCASE",
        Some("added") => " ORDER BY added_at DESC, title COLLATE NOCASE",
        Some("rating") => " ORDER BY rating IS NULL, rating DESC, title COLLATE NOCASE",
        _ => " ORDER BY title COLLATE NOCASE",
    });

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), row_to_game)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_game(conn: &Connection, id: i64) -> Result<Option<Game>> {
    let sql = format!("SELECT {GAME_COLS} FROM games WHERE id = ?1");
    Ok(conn
        .query_row(&sql, params![id], row_to_game)
        .optional()?)
}

/// Games that still need metadata, oldest first.
pub fn games_needing_scrape(conn: &Connection, platform: Option<&str>) -> Result<Vec<Game>> {
    scrape_candidates(conn, platform, true)
}

/// Every game metadata can be fetched for, whatever it already has.
///
/// This is what "fetch everything again" runs. It is deliberately a separate
/// entry point rather than a flag on the automatic pass: the pass after a scan
/// must never spend quota re-fetching art it already has, and asking for all of
/// it must never be quietly narrowed to the ones that happen to have failed.
pub fn games_for_rescrape(conn: &Connection, platform: Option<&str>) -> Result<Vec<Game>> {
    scrape_candidates(conn, platform, false)
}

fn scrape_candidates(
    conn: &Connection,
    platform: Option<&str>,
    only_pending: bool,
) -> Result<Vec<Game>> {
    // A disc folded into a multi-disc entry is not shown anywhere, so
    // fetching metadata for it would spend a scraper lookup - and on
    // ScreenScraper, part of a daily quota - on something nobody will see.
    // The playlist standing for the set is scraped instead. That holds however
    // the fetch was asked for, so it is outside the `only_pending` test.
    let mut sql = format!(
        "SELECT {GAME_COLS} FROM games WHERE disc_playlist_id IS NULL"
    );
    if only_pending {
        sql.push_str(" AND scrape_status IN ('pending', 'error')");
    }
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(p) = platform.filter(|p| !p.is_empty()) {
        sql.push_str(" AND platform = ?");
        args.push(Box::new(p.to_string()));
    }
    sql.push_str(" ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), row_to_game)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub struct Metadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub release_date: Option<String>,
    pub players: Option<String>,
    pub rating: Option<f64>,
    pub cover_path: Option<String>,
    pub screenshot_path: Option<String>,
    pub logo_path: Option<String>,
    pub source: String,
}

pub fn apply_metadata(conn: &Connection, id: i64, m: &Metadata) -> Result<()> {
    conn.execute(
        "UPDATE games SET
            -- A name typed by hand is never replaced by a fetched one.
            title           = CASE WHEN title_custom = 1
                                   THEN title
                                   ELSE COALESCE(?2, title) END,
            description     = COALESCE(?3, description),
            developer       = COALESCE(?4, developer),
            publisher       = COALESCE(?5, publisher),
            genre           = COALESCE(?6, genre),
            release_date    = COALESCE(?7, release_date),
            players         = COALESCE(?8, players),
            rating          = COALESCE(?9, rating),
            -- A cover chosen by hand is never replaced by a fetched one.
            -- Fetching everything again would otherwise quietly undo the
            -- work of picking artwork for a game no provider knows.
            cover_path      = CASE WHEN cover_custom = 1
                                   THEN cover_path
                                   ELSE COALESCE(?10, cover_path) END,
            screenshot_path = COALESCE(?11, screenshot_path),
            logo_path       = COALESCE(?12, logo_path),
            scrape_status   = 'ok',
            scrape_source   = ?13
         WHERE id = ?1",
        params![
            id,
            m.title,
            m.description,
            m.developer,
            m.publisher,
            m.genre,
            m.release_date,
            m.players,
            m.rating,
            m.cover_path,
            m.screenshot_path,
            m.logo_path,
            m.source
        ],
    )?;
    Ok(())
}

/// Record a cover the player chose themselves.
///
/// Marking it also matters: `apply_metadata` leaves a custom cover alone, so
/// this is what makes the choice stick through a later fetch.
/// Rename a game.
///
/// Marked as well as written, for the same reason a hand-picked cover is: a
/// later fetch that matches the game would otherwise put the provider's name
/// straight back over the one chosen here.
pub fn rename_game(conn: &Connection, id: i64, title: &str) -> Result<()> {
    conn.execute(
        "UPDATE games SET title = ?2, title_custom = 1 WHERE id = ?1",
        params![id, title],
    )?;
    Ok(())
}

/// Artwork paths belonging to a game, for copying elsewhere.
pub fn artwork_paths(
    conn: &Connection,
    id: i64,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    Ok(conn.query_row(
        "SELECT cover_path, screenshot_path, logo_path FROM games WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?)
}

/// Give one game the details of another, keeping its own name.
///
/// For a game no provider knows because it is a modification of one they do:
/// a hack of Super Mario 64 should look like Super Mario 64 without being
/// renamed to it. The text is copied from the source row. The artwork paths
/// are supplied by the caller rather than copied from the source, because the
/// caller has already copied the files into this game's own media folder —
/// sharing the source's paths would leave this game's art pointing at a folder
/// that removing the source game deletes.
///
/// The cover is marked as chosen, so a later fetch leaves it alone, and the
/// game is marked as having metadata so the automatic pass stops retrying it.
pub fn borrow_metadata(
    conn: &Connection,
    from: i64,
    to: i64,
    cover: Option<&str>,
    screenshot: Option<&str>,
    logo: Option<&str>,
) -> Result<()> {
    conn.execute(
        // Whatever the source has nothing for, the game keeps: borrowing
        // from a game with no description, or no cover, used to blank this
        // one's, including a cover picked by hand.
        "UPDATE games SET
             description     = COALESCE(src.description, games.description),
             developer       = COALESCE(src.developer, games.developer),
             publisher       = COALESCE(src.publisher, games.publisher),
             genre           = COALESCE(src.genre, games.genre),
             release_date    = COALESCE(src.release_date, games.release_date),
             players         = COALESCE(src.players, games.players),
             rating          = COALESCE(src.rating, games.rating),
             region          = COALESCE(src.region, games.region),
             cover_path      = COALESCE(?3, games.cover_path),
             screenshot_path = COALESCE(?4, games.screenshot_path),
             logo_path       = COALESCE(?5, games.logo_path),
             cover_custom    = CASE WHEN ?3 IS NULL THEN games.cover_custom ELSE 1 END,
             scrape_status   = 'ok',
             scrape_source   = 'borrowed'
         FROM (SELECT * FROM games WHERE id = ?1) AS src
         WHERE games.id = ?2",
        params![from, to, cover, screenshot, logo],
    )?;
    Ok(())
}

pub fn set_custom_cover(conn: &Connection, id: i64, path: &str) -> Result<()> {
    conn.execute(
        "UPDATE games SET cover_path = ?2, cover_custom = 1 WHERE id = ?1",
        params![id, path],
    )?;
    Ok(())
}

/// Give up a hand-picked cover, so metadata fetches can supply one again.
///
/// The file itself is left in place. It is small, it is in this game's own
/// media folder, and removing the whole folder is already what happens when
/// the game is removed.
pub fn clear_custom_cover(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE games SET cover_path = NULL, cover_custom = 0 WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn set_scrape_status(conn: &Connection, id: i64, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE games SET scrape_status = ?2 WHERE id = ?1",
        params![id, status],
    )?;
    Ok(())
}

pub fn set_favorite(conn: &Connection, id: i64, value: bool) -> Result<()> {
    conn.execute(
        "UPDATE games SET favorite = ?2 WHERE id = ?1",
        params![id, if value { 1 } else { 0 }],
    )?;
    Ok(())
}

pub fn set_hidden(conn: &Connection, id: i64, value: bool) -> Result<()> {
    conn.execute(
        "UPDATE games SET hidden = ?2 WHERE id = ?1",
        params![id, if value { 1 } else { 0 }],
    )?;
    Ok(())
}

pub fn set_platform(conn: &Connection, id: i64, platform: &str) -> Result<()> {
    conn.execute(
        "UPDATE games SET platform = ?2, scrape_status = 'pending' WHERE id = ?1",
        params![id, platform],
    )?;
    Ok(())
}

/// Remove a game from the library. Never touches the ROM on disk.
/// Forget one game. The file on disk is not touched.
///
/// Hacks made from it stay, as games of their own: their ROMs are separate
/// files, and removing the original was never asking to lose them - the
/// table's cascade used to take them, play time and all. A disc set goes
/// with its discs, which are the same game; leaving them would leave them
/// hidden and pointing at a set that no longer exists.
pub fn remove_game(conn: &Connection, id: i64) -> Result<()> {
    // A savepoint rather than a transaction, so this also works inside one -
    // regrouping disc sets removes stale ones from within its own.
    conn.execute_batch("SAVEPOINT remove_game")?;
    let steps = || -> rusqlite::Result<()> {
        conn.execute("UPDATE games SET base_game_id = NULL WHERE base_game_id = ?1", params![id])?;
        conn.execute("DELETE FROM games WHERE disc_playlist_id = ?1", params![id])?;
        conn.execute("DELETE FROM games WHERE id = ?1", params![id])?;
        Ok(())
    };
    match steps() {
        Ok(()) => conn.execute_batch("RELEASE remove_game")?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO remove_game; RELEASE remove_game");
            return Err(e.into());
        }
    }
    Ok(())
}

/// The entries whose files are gone, by id, and how many more could not be
/// checked because the drive or share they are on is not there at all.
///
/// A drive that is not plugged in is not a library that has been deleted.
/// Every game on an unreachable library folder, or under a drive or network
/// share root that does not exist, is left out, so cleaning up while the NAS
/// is asleep does not throw away everything on it.
pub fn missing_entries(conn: &Connection) -> Result<(Vec<i64>, usize)> {
    let roots: Vec<std::path::PathBuf> = list_folders(conn)?
        .into_iter()
        .map(|f| std::path::PathBuf::from(f.path))
        .collect();
    let mut stmt = conn.prepare("SELECT id, path FROM games WHERE disc_playlist_id IS NULL")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;

    let mut missing = Vec::new();
    let mut unreachable = 0;
    for row in rows {
        let (id, path) = row?;
        let path = std::path::Path::new(&path);
        if path.exists() {
            continue;
        }
        let library_offline = roots.iter().any(|r| path.starts_with(r) && !r.exists());
        // A path's last ancestor is its drive (`D:\`) or share (`\\nas\roms`).
        let volume_offline = path
            .ancestors()
            .last()
            .is_some_and(|top| !top.as_os_str().is_empty() && !top.exists());
        if library_offline || volume_offline {
            unreachable += 1;
        } else {
            missing.push(id);
        }
    }
    Ok((missing, unreachable))
}

pub fn record_play(conn: &Connection, id: i64, started_at: i64, seconds: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO play_sessions (game_id, started_at, seconds) VALUES (?1, ?2, ?3)",
        params![id, started_at, seconds],
    )?;
    conn.execute(
        "UPDATE games SET play_count = play_count + 1,
                          play_seconds = play_seconds + ?2,
                          last_played = ?3
         WHERE id = ?1",
        params![id, seconds, started_at + seconds],
    )?;
    Ok(())
}

/// What a scan needs to know about the rows already in the library: enough to
/// tell an entry that is fine from one indexed under an older, worse rule.
pub struct IndexState {
    pub id: i64,
    pub platform: String,
    pub inner_name: Option<String>,
    pub filename: String,
    /// True when the entry carries something a scan must never throw away on
    /// a heuristic: time played, a favourite mark, or a tie to a ROM hack.
    pub precious: bool,
}

pub fn index_state(conn: &Connection) -> Result<std::collections::HashMap<String, IndexState>> {
    let mut stmt = conn.prepare(
        "SELECT g.id, g.path, g.platform, g.inner_name,
                (g.play_seconds > 0 OR g.play_count > 0 OR g.favorite <> 0
                 OR g.base_game_id IS NOT NULL OR g.patch_path IS NOT NULL
                 OR EXISTS (SELECT 1 FROM games h WHERE h.base_game_id = g.id)),
                g.filename
         FROM games g",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(1)?,
            IndexState {
                id: r.get(0)?,
                platform: r.get(2)?,
                inner_name: r.get(3)?,
                precious: r.get(4)?,
                filename: r.get(5)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?)
}

/// Correct the indexing facts for a row that was recorded under an older rule.
///
/// Only the facts a scan owns are touched. Title, artwork and everything else
/// a scraper wrote stay as they are, and a platform that is already set is
/// left alone — it may well have been chosen by hand.
pub fn reindex_game(
    conn: &Connection,
    id: i64,
    platform: &str,
    size: i64,
    crc32: Option<&str>,
    md5: Option<&str>,
    sha1: Option<&str>,
    inner_name: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE games SET
            platform   = CASE WHEN platform = 'unknown' THEN ?2 ELSE platform END,
            size       = ?3,
            crc32      = ?4,
            md5        = ?5,
            sha1       = ?6,
            inner_name = ?7
         WHERE id = ?1",
        params![id, platform, size, crc32, md5, sha1, inner_name],
    )?;
    Ok(())
}

/// Describe an existing row as the console game it turns out to be.
///
/// A PlayStation 3 `EBOOT.BIN` indexed before Arcadex knew the system was
/// filed as an unidentified `.bin` called "EBOOT". The folder around it says
/// what it really is, and that outranks what the row was guessed to be - a
/// platform picked by hand for a file called EBOOT was picked for want of a
/// better answer. A title typed by hand is still kept.
pub fn set_console_identity(
    conn: &Connection,
    id: i64,
    platform: &str,
    filename: &str,
    title: &str,
    size: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE games SET
            platform   = ?2,
            filename   = ?3,
            title      = CASE WHEN title_custom <> 0 THEN title ELSE ?4 END,
            size       = ?5,
            crc32      = NULL,
            md5        = NULL,
            sha1       = NULL,
            inner_name = NULL
         WHERE id = ?1",
        params![id, platform, filename, title, size],
    )?;
    Ok(())
}

/// Record a console game's serial, once it is known.
pub fn set_serial(conn: &Connection, id: i64, serial: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE games SET serial = ?2 WHERE id = ?1 AND serial IS NOT ?2",
        params![id, serial],
    )?;
    Ok(())
}

/// Store checksums worked out after the fact, for a file a scan skipped as
/// too large to hash.
pub fn set_hashes(
    conn: &Connection,
    id: i64,
    crc32: &str,
    md5: &str,
    sha1: &str,
    inner_name: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE games SET crc32 = ?2, md5 = ?3, sha1 = ?4,
                          inner_name = COALESCE(?5, inner_name)
         WHERE id = ?1",
        params![id, crc32, md5, sha1, inner_name],
    )?;
    Ok(())
}

// ------------------------------------------------------------ platforms

pub fn platform_counts(conn: &Connection) -> Result<Vec<PlatformInfo>> {
    let mut stmt = conn.prepare(
        "SELECT platform, COUNT(*) FROM games WHERE hidden = 0 GROUP BY platform ORDER BY 2 DESC",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;

    let mut out = Vec::new();
    for row in rows {
        let (slug, count) = row?;
        let def = platforms::by_slug(&slug);
        out.push(PlatformInfo {
            name: platforms::display_name(&slug),
            slug,
            extensions: def
                .map(|d| d.exts.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default(),
            cores: def
                .map(|d| d.cores.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default(),
            game_count: count,
        });
    }
    Ok(out)
}

/// The library as its lists show it: hidden entries and the discs folded
/// into a multi-disc set are not games of their own. Counting them made "All
/// games" disagree with the grid, and "Needs metadata" never reach nought,
/// since a set's discs are never fetched for.
const LISTED: &str = "hidden = 0 AND disc_playlist_id IS NULL";

pub fn stats(conn: &Connection) -> Result<LibraryStats> {
    Ok(LibraryStats {
        total_games: conn.query_row(&format!("SELECT COUNT(*) FROM games WHERE {LISTED}"), [], |r| {
            r.get(0)
        })?,
        total_platforms: conn.query_row(
            &format!("SELECT COUNT(DISTINCT platform) FROM games WHERE {LISTED}"),
            [],
            |r| r.get(0),
        )?,
        scraped: conn.query_row(
            &format!("SELECT COUNT(*) FROM games WHERE scrape_status = 'ok' AND {LISTED}"),
            [],
            |r| r.get(0),
        )?,
        total_play_seconds: conn
            .query_row("SELECT COALESCE(SUM(play_seconds), 0) FROM games", [], |r| {
                r.get(0)
            })?,
    })
}

// -------------------------------------------------------------- folders

pub fn add_folder(conn: &Connection, path: &str, platform: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO folders (path, platform_override, added_at)
         VALUES (?1, ?2, COALESCE((SELECT added_at FROM folders WHERE path = ?1), ?3))",
        params![path, platform, now()],
    )?;
    Ok(())
}

pub fn remove_folder(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM folders WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_folders(conn: &Connection) -> Result<Vec<LibraryFolder>> {
    let mut stmt =
        conn.prepare("SELECT id, path, platform_override, added_at FROM folders ORDER BY path")?;
    let rows = stmt.query_map([], |r| {
        Ok(LibraryFolder {
            id: r.get(0)?,
            path: r.get(1)?,
            platform_override: r.get(2)?,
            added_at: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ------------------------------------------------------------- settings

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn delete_setting(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
    Ok(())
}

pub fn all_settings(conn: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?)
}

// ------------------------------------------------------------ emulators

pub fn get_emulator(conn: &Connection, platform: &str) -> Result<Option<EmulatorConfig>> {
    Ok(conn
        .query_row(
            "SELECT platform, mode, core, custom_command FROM emulators WHERE platform = ?1",
            params![platform],
            |r| {
                Ok(EmulatorConfig {
                    platform: r.get(0)?,
                    mode: r.get(1)?,
                    core: r.get(2)?,
                    custom_command: r.get(3)?,
                })
            },
        )
        .optional()?)
}

pub fn set_emulator(conn: &Connection, cfg: &EmulatorConfig) -> Result<()> {
    conn.execute(
        "INSERT INTO emulators (platform, mode, core, custom_command) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(platform) DO UPDATE SET
            mode = excluded.mode,
            core = excluded.core,
            custom_command = excluded.custom_command",
        params![cfg.platform, cfg.mode, cfg.core, cfg.custom_command],
    )?;
    Ok(())
}

// ------------------------------------------------------------ multi-disc

/// Every entry that could be one disc of a set: the real games, not the
/// playlists that already stand for them.
pub fn disc_candidates(conn: &Connection) -> Result<Vec<Game>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {GAME_COLS} FROM games WHERE is_disc_playlist = 0 ORDER BY path"
    ))?;
    let rows = stmt.query_map([], row_to_game)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The playlists standing in for disc sets.
pub fn disc_playlists(conn: &Connection) -> Result<Vec<Game>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {GAME_COLS} FROM games WHERE is_disc_playlist = 1 ORDER BY path"
    ))?;
    let rows = stmt.query_map([], row_to_game)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The discs one playlist covers, in the order they are listed.
pub fn disc_members(conn: &Connection, playlist_id: i64) -> Result<Vec<Game>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {GAME_COLS} FROM games WHERE disc_playlist_id = ?1 ORDER BY path"
    ))?;
    let rows = stmt.query_map(params![playlist_id], row_to_game)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Break every disc set apart, ready to be rebuilt from what is on disk now.
/// The discs come back into view; nothing on disk is touched.
pub fn release_all_discs(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE games SET disc_playlist_id = NULL, hidden = 0
         WHERE disc_playlist_id IS NOT NULL",
        [],
    )?;
    Ok(())
}

/// Record the playlist for a disc set, keeping the row it already had so its
/// play time, favourite mark and artwork survive a rescan.
pub fn upsert_disc_playlist(
    conn: &Connection,
    path: &str,
    filename: &str,
    platform: &str,
    title: &str,
    size: i64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO games (path, filename, platform, title, size, added_at, is_disc_playlist)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
         ON CONFLICT(path) DO UPDATE SET
             filename = excluded.filename,
             platform = excluded.platform,
             size     = excluded.size,
             is_disc_playlist = 1",
        params![path, filename, platform, title, size, now()],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM games WHERE path = ?1",
        params![path],
        |r| r.get(0),
    )?)
}

/// Put these discs under a playlist and take them out of the library view.
/// They are hidden rather than deleted: the files are yours, and "Show
/// hidden" brings them straight back.
pub fn set_disc_members(conn: &Connection, playlist_id: i64, ids: &[i64]) -> Result<()> {
    for id in ids {
        conn.execute(
            "UPDATE games SET disc_playlist_id = ?2, hidden = 1 WHERE id = ?1",
            params![id, playlist_id],
        )?;
    }
    Ok(())
}

/// Give a playlist the metadata one of its discs already has, so a set that
/// was scraped before it was grouped does not come back blank.
pub fn copy_metadata(conn: &Connection, from: i64, to: i64) -> Result<()> {
    conn.execute(
        "UPDATE games SET
             description   = src.description,
             developer     = src.developer,
             publisher     = src.publisher,
             genre         = src.genre,
             release_date  = src.release_date,
             players       = src.players,
             rating        = src.rating,
             region        = src.region,
             cover_path    = src.cover_path,
             screenshot_path = src.screenshot_path,
             logo_path     = src.logo_path,
             scrape_status = src.scrape_status,
             scrape_source = src.scrape_source
         FROM (SELECT * FROM games WHERE id = ?1) AS src
         WHERE games.id = ?2 AND games.scrape_status <> 'ok'",
        params![from, to],
    )?;
    Ok(())
}

/// The emulator set for one game in particular, if it has one.
///
/// Systems mostly want one emulator, which is why the setting lives on the
/// system. But "mostly" is not "always": one game in a system can need a
/// different core to run at all, and before this the only way to give it one
/// was to change the setting for every game beside it.
pub fn game_emulator(conn: &Connection, id: i64) -> Result<Option<EmulatorConfig>> {
    let row = conn
        .query_row(
            "SELECT platform, emu_mode, emu_core, emu_command FROM games WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;

    Ok(row.and_then(|(platform, mode, core, command)| {
        mode.map(|mode| EmulatorConfig {
            platform,
            mode,
            core,
            custom_command: command,
        })
    }))
}

/// Set or clear one game's own emulator. `None` returns it to its system's.
pub fn set_game_emulator(conn: &Connection, id: i64, cfg: Option<&EmulatorConfig>) -> Result<()> {
    conn.execute(
        "UPDATE games SET emu_mode = ?2, emu_core = ?3, emu_command = ?4 WHERE id = ?1",
        params![
            id,
            cfg.map(|c| c.mode.as_str()),
            cfg.and_then(|c| c.core.as_deref()),
            cfg.and_then(|c| c.custom_command.as_deref()),
        ],
    )?;
    Ok(())
}

pub fn list_emulators(conn: &Connection) -> Result<Vec<EmulatorConfig>> {
    let mut stmt =
        conn.prepare("SELECT platform, mode, core, custom_command FROM emulators ORDER BY platform")?;
    let rows = stmt.query_map([], |r| {
        Ok(EmulatorConfig {
            platform: r.get(0)?,
            mode: r.get(1)?,
            core: r.get(2)?,
            custom_command: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ------------------------------------------------------- patch catalog

const PATCH_COLS: &str =
    "id, path, name, format, source_crc, system_hint, target_hint, origin, added_at";

fn row_to_patch(row: &Row) -> rusqlite::Result<PatchEntry> {
    Ok(PatchEntry {
        id: row.get(0)?,
        path: row.get(1)?,
        name: row.get(2)?,
        format: row.get(3)?,
        source_crc: row.get(4)?,
        system_hint: row.get(5)?,
        target_hint: row.get(6)?,
        origin: row.get(7)?,
        added_at: row.get(8)?,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn insert_patch(
    conn: &Connection,
    path: &str,
    name: &str,
    format: &str,
    source_crc: Option<&str>,
    system_hint: Option<&str>,
    target_hint: Option<&str>,
    origin: &str,
) -> Result<bool> {
    let changed = conn.execute(
        "INSERT OR IGNORE INTO patches
         (path, name, format, source_crc, system_hint, target_hint, origin, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![path, name, format, source_crc, system_hint, target_hint, origin, now()],
    )?;
    Ok(changed > 0)
}

/// Catalogued patches built against exactly this ROM.
pub fn patches_for_crc(conn: &Connection, crc: &str) -> Result<Vec<PatchEntry>> {
    let sql = format!(
        "SELECT {PATCH_COLS} FROM patches WHERE source_crc = ?1 COLLATE NOCASE          ORDER BY name COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![crc], row_to_patch)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn list_patches(conn: &Connection, search: Option<&str>, limit: i64) -> Result<Vec<PatchEntry>> {
    let mut sql = format!("SELECT {PATCH_COLS} FROM patches WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(q) = search.filter(|q| !q.trim().is_empty()) {
        sql.push_str(" AND name LIKE ?");
        args.push(Box::new(format!("%{}%", q.trim())));
    }
    sql.push_str(" ORDER BY name COLLATE NOCASE LIMIT ?");
    args.push(Box::new(limit));

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), row_to_patch)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_patch(conn: &Connection, id: i64) -> Result<Option<PatchEntry>> {
    let sql = format!("SELECT {PATCH_COLS} FROM patches WHERE id = ?1");
    Ok(conn.query_row(&sql, params![id], row_to_patch).optional()?)
}

pub fn patch_catalog_size(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM patches", [], |r| r.get(0))?)
}

pub fn clear_patch_catalog(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM patches", [])?;
    Ok(())
}

// -------------------------------------------------------------- cheats

/// Replace a game's cheat list, keeping whatever was already switched on.
pub fn replace_cheats(conn: &Connection, game_id: i64, cheats: &[Cheat]) -> Result<()> {
    let previously_on: Vec<i64> = {
        let mut stmt =
            conn.prepare("SELECT idx FROM cheats WHERE game_id = ?1 AND enabled = 1")?;
        let rows = stmt.query_map(params![game_id], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    conn.execute("DELETE FROM cheats WHERE game_id = ?1", params![game_id])?;
    for cheat in cheats {
        let on = cheat.enabled || previously_on.contains(&cheat.index);
        conn.execute(
            "INSERT OR REPLACE INTO cheats (game_id, idx, description, code, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                game_id,
                cheat.index,
                cheat.description,
                cheat.code,
                if on { 1 } else { 0 }
            ],
        )?;
    }
    Ok(())
}

pub fn list_cheats(conn: &Connection, game_id: i64) -> Result<Vec<Cheat>> {
    let mut stmt = conn.prepare(
        "SELECT idx, description, code, enabled FROM cheats WHERE game_id = ?1 ORDER BY idx",
    )?;
    let rows = stmt.query_map(params![game_id], |r| {
        Ok(Cheat {
            index: r.get(0)?,
            description: r.get(1)?,
            code: r.get(2)?,
            enabled: r.get::<_, i64>(3)? != 0,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Switch every cheat for a game at once. One statement rather than a
/// round trip per cheat — some games have several hundred.
pub fn set_all_cheats_enabled(conn: &Connection, game_id: i64, on: bool) -> Result<usize> {
    let changed = conn.execute(
        "UPDATE cheats SET enabled = ?2 WHERE game_id = ?1",
        params![game_id, if on { 1 } else { 0 }],
    )?;
    Ok(changed)
}

pub fn set_cheat_enabled(conn: &Connection, game_id: i64, idx: i64, on: bool) -> Result<()> {
    conn.execute(
        "UPDATE cheats SET enabled = ?3 WHERE game_id = ?1 AND idx = ?2",
        params![game_id, idx, if on { 1 } else { 0 }],
    )?;
    Ok(())
}

// ------------------------------------------------------------ insights

/// Games with a play history, most recent first.
pub fn recently_played(conn: &Connection, limit: i64) -> Result<Vec<Game>> {
    let sql = format!(
        "SELECT {GAME_COLS} FROM games
         WHERE last_played IS NOT NULL AND hidden = 0
         ORDER BY last_played DESC LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], row_to_game)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Games ranked by time spent in them.
pub fn most_played(conn: &Connection, limit: i64) -> Result<Vec<Game>> {
    let sql = format!(
        "SELECT {GAME_COLS} FROM games
         WHERE play_seconds > 0 AND hidden = 0
         ORDER BY play_seconds DESC LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], row_to_game)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn session_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM play_sessions", [], |r| r.get(0))?)
}

pub fn longest_session(conn: &Connection) -> Result<i64> {
    Ok(conn
        .query_row("SELECT COALESCE(MAX(seconds), 0) FROM play_sessions", [], |r| {
            r.get(0)
        })?)
}

#[cfg(test)]
mod repoint_tests {
    use super::{open, repoint_data_dir};

    // The app has been Playdex, Romcade, Playdex again and now Arcadex; the
    // direction under test is from whichever name a library was last saved
    // under to the current one.
    const OLD: &str = "com.boazv.playdex";
    const NEW: &str = "com.boazv.arcadex";

    fn library() -> (scratch::Guard, rusqlite::Connection) {
        let dir = scratch::Guard::new("arcadex-repoint");
        let conn = open(&dir.path().join("library.db")).unwrap();
        (dir, conn)
    }

    fn insert_game(conn: &rusqlite::Connection, id: i64, path: &str, cover: Option<&str>) {
        conn.execute(
            "INSERT INTO games (id, path, filename, title, added_at, cover_path)
             VALUES (?1, ?2, 'f', 't', 0, ?3)",
            rusqlite::params![id, path, cover],
        )
        .unwrap();
    }

    #[test]
    fn rewrites_artwork_left_behind_by_the_rename() {
        let (_dir, conn) = library();
        let cover_before = format!(r"C:\x\{OLD}\media\1\cover.jpg");
        insert_game(&conn, 1, r"D:\roms\mario.nes", Some(&cover_before));

        assert_eq!(repoint_data_dir(&conn, OLD, NEW).unwrap(), 1);

        let cover: String = conn
            .query_row("SELECT cover_path FROM games WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cover, format!(r"C:\x\{NEW}\media\1\cover.jpg"));
    }

    #[test]
    fn rewrites_a_patched_roms_own_path_too() {
        let (_dir, conn) = library();
        insert_game(&conn, 2, &format!(r"C:\x\{OLD}\hacks\2\hack.sfc"), None);

        assert_eq!(repoint_data_dir(&conn, OLD, NEW).unwrap(), 1);

        let path: String = conn
            .query_row("SELECT path FROM games WHERE id = 2", [], |r| r.get(0))
            .unwrap();
        assert_eq!(path, format!(r"C:\x\{NEW}\hacks\2\hack.sfc"));
    }

    #[test]
    fn leaves_the_players_own_rom_paths_alone() {
        let (_dir, conn) = library();
        let mine = r"D:\roms\snes\zelda.sfc";
        insert_game(&conn, 3, mine, None);

        assert_eq!(repoint_data_dir(&conn, OLD, NEW).unwrap(), 0);

        let path: String = conn
            .query_row("SELECT path FROM games WHERE id = 3", [], |r| r.get(0))
            .unwrap();
        assert_eq!(path, mine);
    }

    #[test]
    fn is_idempotent() {
        let (_dir, conn) = library();
        insert_game(
            &conn,
            4,
            r"D:\roms\a.nes",
            Some(&format!(r"C:\x\{OLD}\media\4\cover.jpg")),
        );

        assert_eq!(repoint_data_dir(&conn, OLD, NEW).unwrap(), 1);
        assert_eq!(repoint_data_dir(&conn, OLD, NEW).unwrap(), 0);
        assert_eq!(repoint_data_dir(&conn, NEW, NEW).unwrap(), 0);
    }

    /// A test build runs under `com.boazv.playdex.verify`. Its paths begin
    /// with the old name but are another folder entirely, and rewriting them
    /// would point its artwork at a folder that is not there.
    #[test]
    fn a_folder_whose_name_only_starts_the_same_is_left_alone() {
        let (_dir, conn) = library();
        let other = format!(r"C:\x\{OLD}.verify\media\5\cover.jpg");
        insert_game(&conn, 5, r"D:\roms\b.nes", Some(&other));

        assert_eq!(repoint_data_dir(&conn, OLD, NEW).unwrap(), 0);

        let cover: String = conn
            .query_row("SELECT cover_path FROM games WHERE id = 5", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cover, other);
    }

    /// macOS and Linux paths, and a setting that names the folder itself.
    #[test]
    fn rewrites_forward_slashes_and_the_folder_at_the_end_of_a_value() {
        let (_dir, conn) = library();
        insert_game(
            &conn,
            6,
            "/home/me/roms/c.nes",
            Some(&format!("/home/me/.local/share/{OLD}/media/6/cover.png")),
        );
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('somewhere', ?1)",
            [format!(r"C:\x\{OLD}")],
        )
        .unwrap();

        assert_eq!(repoint_data_dir(&conn, OLD, NEW).unwrap(), 2);

        let cover: String = conn
            .query_row("SELECT cover_path FROM games WHERE id = 6", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cover, format!("/home/me/.local/share/{NEW}/media/6/cover.png"));
        let setting: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'somewhere'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(setting, format!(r"C:\x\{NEW}"));
    }

    /// A directory that removes itself, so each test gets its own library.
    mod scratch {
        use std::sync::atomic::{AtomicU32, Ordering};

        static NEXT: AtomicU32 = AtomicU32::new(0);

        pub struct Guard(std::path::PathBuf);

        impl Guard {
            pub fn new(tag: &str) -> Self {
                let n = NEXT.fetch_add(1, Ordering::Relaxed);
                let dir =
                    std::env::temp_dir().join(format!("{tag}-{}-{n}", std::process::id()));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).unwrap();
                Guard(dir)
            }

            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}

#[cfg(test)]
mod scrape_candidate_tests {
    use super::{games_for_rescrape, games_needing_scrape, open};

    fn library() -> (scratch2::Guard, rusqlite::Connection) {
        let dir = scratch2::Guard::new("arcadex-scrape");
        let conn = open(&dir.path().join("library.db")).unwrap();
        (dir, conn)
    }

    fn add(conn: &rusqlite::Connection, id: i64, status: &str, platform: &str) {
        conn.execute(
            "INSERT INTO games (id, path, filename, title, added_at, platform, scrape_status)
             VALUES (?1, ?2, 'f', 't', 0, ?3, ?4)",
            rusqlite::params![id, format!("D:/roms/{id}.rom"), platform, status],
        )
        .unwrap();
    }

    fn ids(games: &[crate::models::Game]) -> Vec<i64> {
        games.iter().map(|g| g.id).collect()
    }

    #[test]
    fn the_automatic_pass_skips_games_that_already_have_metadata() {
        let (_d, conn) = library();
        add(&conn, 1, "pending", "nes");
        add(&conn, 2, "ok", "nes");
        add(&conn, 3, "error", "nes");
        add(&conn, 4, "notfound", "nes");

        // 'ok' is left alone, and so is 'notfound': looking again for something
        // the providers have already said they do not have spends quota for
        // nothing.
        assert_eq!(ids(&games_needing_scrape(&conn, None).unwrap()), vec![1, 3]);
    }

    #[test]
    fn fetching_everything_again_includes_games_that_already_matched() {
        let (_d, conn) = library();
        add(&conn, 1, "pending", "nes");
        add(&conn, 2, "ok", "nes");
        add(&conn, 3, "error", "nes");
        add(&conn, 4, "notfound", "nes");

        assert_eq!(
            ids(&games_for_rescrape(&conn, None).unwrap()),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn both_skip_a_disc_folded_into_a_multi_disc_entry() {
        let (_d, conn) = library();
        add(&conn, 1, "ok", "psx");
        add(&conn, 2, "pending", "psx");
        // Disc two of the set standing behind game 1: not shown anywhere, so
        // never worth a lookup however the fetch was asked for.
        conn.execute("UPDATE games SET disc_playlist_id = 1 WHERE id = 2", [])
            .unwrap();

        assert!(games_needing_scrape(&conn, None).unwrap().is_empty());
        assert_eq!(ids(&games_for_rescrape(&conn, None).unwrap()), vec![1]);
    }

    #[test]
    fn both_honour_a_platform_filter() {
        let (_d, conn) = library();
        add(&conn, 1, "ok", "nes");
        add(&conn, 2, "ok", "snes");

        assert_eq!(ids(&games_for_rescrape(&conn, Some("snes")).unwrap()), vec![2]);
        assert!(games_for_rescrape(&conn, Some("gb")).unwrap().is_empty());
    }

    mod scratch2 {
        use std::sync::atomic::{AtomicU32, Ordering};

        static NEXT: AtomicU32 = AtomicU32::new(0);

        pub struct Guard(std::path::PathBuf);

        impl Guard {
            pub fn new(tag: &str) -> Self {
                let n = NEXT.fetch_add(1, Ordering::Relaxed);
                let dir =
                    std::env::temp_dir().join(format!("{tag}-{}-{n}", std::process::id()));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).unwrap();
                Guard(dir)
            }

            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}

#[cfg(test)]
mod custom_cover_tests {
    use super::{apply_metadata, clear_custom_cover, open, set_custom_cover, Metadata};

    fn library() -> (scratch3::Guard, rusqlite::Connection) {
        let dir = scratch3::Guard::new("arcadex-cover");
        let conn = open(&dir.path().join("library.db")).unwrap();
        conn.execute(
            "INSERT INTO games (id, path, filename, title, added_at, platform, scrape_status)
             VALUES (1, 'D:/roms/hack.sfc', 'hack.sfc', 'A Hack', 0, 'snes', 'notfound')",
            [],
        )
        .unwrap();
        (dir, conn)
    }

    fn fetched(cover: &str) -> Metadata {
        Metadata {
            title: None,
            description: None,
            developer: None,
            publisher: None,
            genre: None,
            release_date: None,
            players: None,
            rating: None,
            cover_path: Some(cover.to_string()),
            screenshot_path: None,
            logo_path: None,
            source: "test".into(),
        }
    }

    fn cover_of(conn: &rusqlite::Connection) -> Option<String> {
        conn.query_row("SELECT cover_path FROM games WHERE id = 1", [], |r| r.get(0))
            .unwrap()
    }

    /// The whole point. Someone picks artwork for a game the providers have
    /// never heard of; "fetch everything again" must not throw it away.
    #[test]
    fn a_fetch_does_not_replace_a_hand_picked_cover() {
        let (_d, conn) = library();
        set_custom_cover(&conn, 1, "C:/media/1/custom.png").unwrap();

        apply_metadata(&conn, 1, &fetched("C:/media/1/cover.jpg")).unwrap();

        assert_eq!(cover_of(&conn).as_deref(), Some("C:/media/1/custom.png"));
    }

    /// Everything else in the same fetch still lands — it is the cover that is
    /// protected, not the whole row.
    #[test]
    fn the_rest_of_the_metadata_still_applies() {
        let (_d, conn) = library();
        set_custom_cover(&conn, 1, "C:/media/1/custom.png").unwrap();

        let mut m = fetched("C:/media/1/cover.jpg");
        m.developer = Some("Someone".into());
        m.screenshot_path = Some("C:/media/1/shot.png".into());
        apply_metadata(&conn, 1, &m).unwrap();

        let (dev, shot): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT developer, screenshot_path FROM games WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(dev.as_deref(), Some("Someone"));
        assert_eq!(shot.as_deref(), Some("C:/media/1/shot.png"));
        assert_eq!(cover_of(&conn).as_deref(), Some("C:/media/1/custom.png"));
    }

    #[test]
    fn without_a_custom_cover_a_fetch_sets_one_as_before() {
        let (_d, conn) = library();
        apply_metadata(&conn, 1, &fetched("C:/media/1/cover.jpg")).unwrap();
        assert_eq!(cover_of(&conn).as_deref(), Some("C:/media/1/cover.jpg"));
    }

    #[test]
    fn giving_up_a_custom_cover_lets_fetches_win_again() {
        let (_d, conn) = library();
        set_custom_cover(&conn, 1, "C:/media/1/custom.png").unwrap();
        clear_custom_cover(&conn, 1).unwrap();
        assert_eq!(cover_of(&conn), None);

        apply_metadata(&conn, 1, &fetched("C:/media/1/cover.jpg")).unwrap();
        assert_eq!(cover_of(&conn).as_deref(), Some("C:/media/1/cover.jpg"));
    }

    mod scratch3 {
        use std::sync::atomic::{AtomicU32, Ordering};

        static NEXT: AtomicU32 = AtomicU32::new(0);

        pub struct Guard(std::path::PathBuf);

        impl Guard {
            pub fn new(tag: &str) -> Self {
                let n = NEXT.fetch_add(1, Ordering::Relaxed);
                let dir =
                    std::env::temp_dir().join(format!("{tag}-{}-{n}", std::process::id()));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).unwrap();
                Guard(dir)
            }

            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}

#[cfg(test)]
mod rename_and_borrow_tests {
    use super::{apply_metadata, borrow_metadata, open, rename_game, Metadata};

    fn library() -> (scratch4::Guard, rusqlite::Connection) {
        let dir = scratch4::Guard::new("arcadex-rename");
        let conn = open(&dir.path().join("library.db")).unwrap();
        conn.execute(
            "INSERT INTO games (id, path, filename, title, added_at, platform, scrape_status,
                                description, developer, genre, cover_path, screenshot_path)
             VALUES (1, 'D:/roms/sm64.z64', 'sm64.z64', 'Super Mario 64', 0, 'n64', 'ok',
                     'Mario in three dimensions.', 'Nintendo', 'Platform',
                     'C:/media/1/cover.jpg', 'C:/media/1/shot.png')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO games (id, path, filename, title, added_at, platform, scrape_status)
             VALUES (2, 'D:/roms/hack.z64', 'hack.z64', 'Super Mario Bros 64', 0, 'n64', 'notfound')",
            [],
        )
        .unwrap();
        (dir, conn)
    }

    fn fetched_title(title: &str) -> Metadata {
        Metadata {
            title: Some(title.to_string()),
            description: None,
            developer: None,
            publisher: None,
            genre: None,
            release_date: None,
            players: None,
            rating: None,
            cover_path: None,
            screenshot_path: None,
            logo_path: None,
            source: "test".into(),
        }
    }

    fn row(conn: &rusqlite::Connection, id: i64) -> (String, Option<String>, Option<String>, i64, String) {
        conn.query_row(
            "SELECT title, developer, cover_path, cover_custom, scrape_status FROM games WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap()
    }

    #[test]
    fn a_fetch_does_not_undo_a_rename() {
        let (_d, conn) = library();
        rename_game(&conn, 2, "My Mario Hack").unwrap();
        apply_metadata(&conn, 2, &fetched_title("Super Mario 64")).unwrap();
        assert_eq!(row(&conn, 2).0, "My Mario Hack");
    }

    #[test]
    fn an_unrenamed_title_still_takes_the_fetched_one() {
        let (_d, conn) = library();
        apply_metadata(&conn, 2, &fetched_title("Fetched Name")).unwrap();
        assert_eq!(row(&conn, 2).0, "Fetched Name");
    }

    /// The request that prompted this: the hack looks like Super Mario 64 and
    /// keeps its own name.
    #[test]
    fn borrowing_copies_the_details_but_keeps_the_name() {
        let (_d, conn) = library();
        borrow_metadata(&conn, 1, 2, Some("C:/media/2/borrowed-cover.jpg"), None, None).unwrap();

        let (title, developer, cover, custom, status) = row(&conn, 2);
        assert_eq!(title, "Super Mario Bros 64");
        assert_eq!(developer.as_deref(), Some("Nintendo"));
        // Its own copy, not the source's file.
        assert_eq!(cover.as_deref(), Some("C:/media/2/borrowed-cover.jpg"));
        assert_eq!(custom, 1);
        // Out of the automatic pass, which would otherwise keep retrying it.
        assert_eq!(status, "ok");
    }

    #[test]
    fn a_borrowed_cover_survives_a_later_fetch() {
        let (_d, conn) = library();
        borrow_metadata(&conn, 1, 2, Some("C:/media/2/borrowed-cover.jpg"), None, None).unwrap();
        let mut m = fetched_title("Super Mario 64");
        m.cover_path = Some("C:/media/2/cover.jpg".into());
        apply_metadata(&conn, 2, &m).unwrap();
        assert_eq!(row(&conn, 2).2.as_deref(), Some("C:/media/2/borrowed-cover.jpg"));
    }

    #[test]
    fn borrowing_from_a_game_with_no_art_does_not_mark_a_cover_chosen() {
        let (_d, conn) = library();
        borrow_metadata(&conn, 1, 2, None, None, None).unwrap();
        let (_, developer, cover, custom, _) = row(&conn, 2);
        assert_eq!(developer.as_deref(), Some("Nintendo"));
        assert_eq!(cover, None);
        assert_eq!(custom, 0);
    }

    #[test]
    fn the_source_game_is_left_unchanged() {
        let (_d, conn) = library();
        borrow_metadata(&conn, 1, 2, Some("C:/media/2/borrowed-cover.jpg"), None, None).unwrap();
        let (title, _, cover, custom, _) = row(&conn, 1);
        assert_eq!(title, "Super Mario 64");
        assert_eq!(cover.as_deref(), Some("C:/media/1/cover.jpg"));
        assert_eq!(custom, 0);
    }

    mod scratch4 {
        use std::sync::atomic::{AtomicU32, Ordering};

        static NEXT: AtomicU32 = AtomicU32::new(0);

        pub struct Guard(std::path::PathBuf);

        impl Guard {
            pub fn new(tag: &str) -> Self {
                let n = NEXT.fetch_add(1, Ordering::Relaxed);
                let dir =
                    std::env::temp_dir().join(format!("{tag}-{}-{n}", std::process::id()));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).unwrap();
                Guard(dir)
            }

            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}

#[cfg(test)]
mod removal_tests {
    use super::*;

    fn library(tag: &str) -> (std::path::PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("arcadex-remove-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conn = open(&dir.join("library.db")).unwrap();
        (dir, conn)
    }

    fn add(conn: &Connection, id: i64, path: &str, extra: &str) {
        conn.execute(
            &format!(
                "INSERT INTO games (id, path, filename, platform, title, added_at{}) VALUES (?1, ?2, 'f', 'snes', 't', 0{})",
                if extra.is_empty() { "" } else { ", base_game_id" },
                if extra.is_empty() { "".to_string() } else { format!(", {extra}") }
            ),
            params![id, path],
        )
        .unwrap();
    }

    fn ids(conn: &Connection) -> Vec<i64> {
        let mut stmt = conn.prepare("SELECT id FROM games ORDER BY id").unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    /// Removing a game used to take every hack made from it, by cascade.
    #[test]
    fn removing_a_game_keeps_the_hacks_made_from_it() {
        let (dir, conn) = library("hacks");
        add(&conn, 1, "C:/roms/smw.sfc", "");
        add(&conn, 2, "C:/data/hacks/1/kaizo.sfc", "1");
        remove_game(&conn, 1).unwrap();
        assert_eq!(ids(&conn), vec![2]);
        let base: Option<i64> = conn
            .query_row("SELECT base_game_id FROM games WHERE id = 2", [], |r| r.get(0))
            .unwrap();
        assert_eq!(base, None);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removing_a_disc_set_takes_its_discs() {
        let (dir, conn) = library("discs");
        add(&conn, 1, "C:/data/playlists/ff7.m3u", "");
        add(&conn, 2, "C:/roms/ff7 (Disc 1).cue", "");
        add(&conn, 3, "C:/roms/ff7 (Disc 2).cue", "");
        add(&conn, 4, "C:/roms/other.cue", "");
        set_disc_members(&conn, 1, &[2, 3]).unwrap();
        remove_game(&conn, 1).unwrap();
        assert_eq!(ids(&conn), vec![4]);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that is gone is missing; a whole drive that is not there is
    /// not plugged in, and nothing on it is touched.
    #[test]
    fn a_drive_that_is_not_there_is_not_a_deleted_library() {
        let (dir, conn) = library("missing");
        let here = dir.join("present.sfc");
        std::fs::write(&here, b"x").unwrap();
        add(&conn, 1, &here.to_string_lossy(), "");
        add(&conn, 2, &dir.join("deleted.sfc").to_string_lossy(), "");
        // A share no machine on earth has.
        add(&conn, 3, r"\arcadex-no-such-host\roms\game.sfc", "");
        add_folder(&conn, r"\arcadex-no-such-host\roms", None).unwrap();

        let (missing, unreachable) = missing_entries(&conn).unwrap();
        assert_eq!(missing, vec![2]);
        assert_eq!(unreachable, 1);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Borrowing from a game with less leaves this one's own details be.
    #[test]
    fn borrowing_keeps_what_the_source_does_not_have() {
        let (dir, conn) = library("borrow");
        add(&conn, 1, "C:/roms/source.sfc", "");
        add(&conn, 2, "C:/roms/target.sfc", "");
        conn.execute(
            "UPDATE games SET description = 'mine', cover_path = 'C:/media/2/cover.png', cover_custom = 1 WHERE id = 2",
            [],
        )
        .unwrap();
        conn.execute("UPDATE games SET developer = 'Nintendo' WHERE id = 1", []).unwrap();

        borrow_metadata(&conn, 1, 2, None, None, None).unwrap();
        let (desc, dev, cover, custom): (Option<String>, Option<String>, Option<String>, i64) = conn
            .query_row(
                "SELECT description, developer, cover_path, cover_custom FROM games WHERE id = 2",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(desc.as_deref(), Some("mine"));
        assert_eq!(dev.as_deref(), Some("Nintendo"));
        assert_eq!(cover.as_deref(), Some("C:/media/2/cover.png"));
        assert_eq!(custom, 1);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
