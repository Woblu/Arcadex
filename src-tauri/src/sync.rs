//! Saves and play time across machines, through a folder you already sync.
//!
//! There is no server. Point Arcadex at a folder that Syncthing, OneDrive,
//! Dropbox or a NAS share already keeps the same on every machine, and each
//! machine writes its own files into it and reads everyone else's:
//!
//! ```text
//! <folder>/Arcadex/
//!   machines/<id>.json                     who a machine is, and when it last synced
//!   play/<id>.jsonl                        every session played on that machine
//!   saves/<system>/<game>/<id>.json        the newest saves that machine has for a game
//!   saves/<system>/<game>/versions/<when>-<id>/   the files, one folder per version
//! ```
//!
//! Every file has exactly one machine writing it. Sync tools are good at
//! carrying a file from one place to another and bad at two machines changing
//! the same one - Syncthing leaves `.sync-conflict` copies, OneDrive renames
//! one of them - so nothing here is ever written by two.
//!
//! Games are matched across machines by what they are, not where they are:
//! the ROM's checksum when there is one, else its serial, else its system and
//! file name. The same dump is the same game on every machine, whatever its
//! path, and even when one machine has it zipped and another does not.
//!
//! Nothing is deleted. Saves pulled over the top of local ones are backed up
//! first; when two machines both changed a game's saves, the newer wins and
//! the other is kept, locally or as a version in the folder.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::db;
use crate::error::{AppError, Result};
use crate::models::{Game, SaveEntry};

/// How many versions of one game's saves each machine keeps in the folder.
const KEEP_VERSIONS: usize = 10;

/// The sync settings, read fresh each time so a change in Settings applies
/// at once.
#[derive(Debug, Clone)]
pub struct Config {
    /// `<chosen folder>/Arcadex`, so the folder can be shared with other
    /// things without Arcadex's files getting in their way.
    pub root: PathBuf,
    /// This machine's id: its name, made safe for a file name, and a few
    /// random characters so two machines called "desktop" stay apart.
    pub machine: String,
    /// The name shown to people, on this machine and the others.
    pub name: String,
    /// Whether save states travel as well as saves. States are bigger, and
    /// tied to the emulator that made them.
    pub states: bool,
}

impl Config {
    /// The sync settings, or `None` when sync is off.
    pub fn load(conn: &Connection) -> Result<Option<Config>> {
        let Some(folder) = db::get_setting(conn, "sync_dir")?
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty())
        else {
            return Ok(None);
        };
        let machine = match db::get_setting(conn, "sync_machine_id")?.filter(|m| !m.is_empty()) {
            Some(m) => m,
            None => {
                let m = new_machine_id(&hostname());
                db::set_setting(conn, "sync_machine_id", &m)?;
                m
            }
        };
        let name = db::get_setting(conn, "sync_machine_name")?
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(hostname);
        let states = db::get_setting(conn, "sync_states")?.as_deref() != Some("0");
        Ok(Some(Config {
            root: Path::new(&folder).join("Arcadex"),
            machine,
            name,
            states,
        }))
    }

    /// Whether the chosen folder is there. A NAS that is switched off or a
    /// drive that is not plugged in is not an error, only a sync that waits.
    pub fn reachable(&self) -> bool {
        self.root.parent().is_some_and(Path::is_dir)
    }

    fn journal(&self, machine: &str) -> PathBuf {
        self.root.join("play").join(format!("{machine}.jsonl"))
    }
}

/// This computer's name, for showing on the others.
pub fn hostname() -> String {
    let from_env = || {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .ok()
    };
    // Linux, including inside a Flatpak, where neither variable is set.
    let from_proc = || std::fs::read_to_string("/proc/sys/kernel/hostname").ok();
    // macOS.
    let from_command = || {
        std::process::Command::new("hostname")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
    };
    from_env()
        .or_else(from_proc)
        .or_else(from_command)
        .map(|s| s.trim().trim_end_matches(".local").to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "This computer".to_string())
}

fn new_machine_id(host: &str) -> String {
    let mut slug: String = host
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    slug.truncate(24);
    let slug = slug.trim_matches('-');
    let mut h = Sha1::new();
    h.update(host.as_bytes());
    h.update(std::process::id().to_le_bytes());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    h.update(nanos.to_le_bytes());
    let tag = &hex::encode(h.finalize())[..6];
    if slug.is_empty() {
        format!("machine-{tag}")
    } else {
        format!("{slug}-{tag}")
    }
}

/// What identifies a game on every machine, whatever its path.
///
/// A checksum first: it is the dump itself. Then the serial a console game
/// carries, for the games too big to checksum. Last the system and the file
/// name, which is right as long as the file is called the same everywhere.
pub fn key_of(
    platform: &str,
    sha1: Option<&str>,
    md5: Option<&str>,
    crc32: Option<&str>,
    size: i64,
    serial: Option<&str>,
    filename: &str,
) -> String {
    let some = |v: Option<&str>| v.map(str::trim).filter(|v| !v.is_empty()).map(str::to_ascii_lowercase);
    if let Some(h) = some(sha1) {
        return format!("sha1:{h}");
    }
    if let Some(h) = some(md5) {
        return format!("md5:{h}");
    }
    if let Some(h) = some(crc32) {
        return format!("crc:{platform}:{h}:{size}");
    }
    if let Some(s) = some(serial) {
        return format!("serial:{platform}:{s}");
    }
    format!("name:{platform}:{}", filename.to_lowercase())
}

/// The folder in the sync folder holding one game's saves.
fn game_dir(cfg: &Config, platform: &str, key: &str) -> PathBuf {
    let system: String = platform
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let system = if system.is_empty() { "unknown".to_string() } else { system };
    let id = &hex::encode(Sha1::digest(key.as_bytes()))[..16];
    cfg.root.join("saves").join(system).join(id)
}

/// Write a file so that nobody reading it - this machine, or a sync tool
/// carrying it to another - ever sees half of it.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.writing"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Play time
// ---------------------------------------------------------------------------

/// One session, as a line in a machine's journal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Session {
    key: String,
    title: String,
    platform: String,
    started: i64,
    seconds: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Machine {
    id: String,
    name: String,
    updated_at: i64,
}

/// Another machine that syncs into the same folder.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtherMachine {
    pub name: String,
    pub last_seen: i64,
}

/// This machine's journal: every session played here, one per line.
///
/// Read from the library only, so the library is held for a query and not
/// for however long a network share takes to write to.
fn journal_text(conn: &Connection) -> Result<(String, usize)> {
    let mut stmt = conn.prepare(
        "SELECT g.platform, g.sha1, g.md5, g.crc32, g.size, g.serial, g.filename, g.title,
                s.started_at, s.seconds
         FROM play_sessions s JOIN games g ON g.id = s.game_id
         WHERE s.origin IS NULL
         ORDER BY s.started_at, s.id",
    )?;
    let sessions = stmt
        .query_map([], |r| {
            let platform: String = r.get(0)?;
            Ok(Session {
                key: key_of(
                    &platform,
                    r.get::<_, Option<String>>(1)?.as_deref(),
                    r.get::<_, Option<String>>(2)?.as_deref(),
                    r.get::<_, Option<String>>(3)?.as_deref(),
                    r.get(4)?,
                    r.get::<_, Option<String>>(5)?.as_deref(),
                    &r.get::<_, String>(6)?,
                ),
                title: r.get(7)?,
                platform,
                started: r.get(8)?,
                seconds: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut text = String::new();
    for s in &sessions {
        text.push_str(&serde_json::to_string(s).map_err(|e| AppError::Other(e.to_string()))?);
        text.push('\n');
    }
    Ok((text, sessions.len()))
}

/// Put this machine's journal in the folder, and say who it is.
///
/// The whole file each time rather than appending to it, so there is never
/// a half-written line, and a journal is always exactly what the library
/// says. It is small - a few hundred bytes a session.
fn publish_journal(cfg: &Config, text: &str) -> Result<()> {
    write_atomic(&cfg.journal(&cfg.machine), text.as_bytes())?;
    let me = Machine {
        id: cfg.machine.clone(),
        name: cfg.name.clone(),
        updated_at: db::now(),
    };
    write_atomic(
        &cfg.root.join("machines").join(format!("{}.json", cfg.machine)),
        &serde_json::to_vec_pretty(&me).map_err(|e| AppError::Other(e.to_string()))?,
    )
}

#[cfg(test)]
fn write_journal(conn: &Connection, cfg: &Config) -> Result<usize> {
    let (text, n) = journal_text(conn)?;
    publish_journal(cfg, &text)?;
    Ok(n)
}

/// Every game in the library by its sync key. Where the same dump is in the
/// library twice, the first one gets the play time.
fn games_by_key(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT id, platform, sha1, md5, crc32, size, serial, filename FROM games ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            key_of(
                &r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?.as_deref(),
                r.get::<_, Option<String>>(3)?.as_deref(),
                r.get::<_, Option<String>>(4)?.as_deref(),
                r.get(5)?,
                r.get::<_, Option<String>>(6)?.as_deref(),
                &r.get::<_, String>(7)?,
            ),
        ))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (id, key) = row?;
        map.entry(key).or_insert(id);
    }
    Ok(map)
}

/// What reading the other machines' journals did.
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    /// Sessions new to this library.
    pub sessions: usize,
    /// Their play time, in seconds.
    pub seconds: i64,
}

/// Every other machine's journal, read from the folder.
fn read_journals(cfg: &Config) -> Vec<(String, Vec<Session>)> {
    let Ok(entries) = std::fs::read_dir(cfg.root.join("play")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(machine) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
            continue;
        };
        // Our own, and a sync tool's conflict copies of anyone's.
        if machine == cfg.machine || machine.starts_with('.') || machine.contains(".sync-conflict") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let sessions = text
            .lines()
            .filter_map(|line| serde_json::from_str::<Session>(line).ok())
            .collect();
        out.push((machine, sessions));
    }
    out
}

/// Add other machines' sessions to this library.
///
/// A session already read is recognised by where it came from and when it
/// started, so reading the same journal again adds nothing. A session for a
/// game this library does not have is passed over, and is picked up on a
/// later read once the game has been added.
fn apply_journals(conn: &Connection, journals: &[(String, Vec<Session>)]) -> Result<Imported> {
    let mut done = Imported::default();
    if journals.is_empty() {
        return Ok(done);
    }
    let games = games_by_key(conn)?;
    let tx = conn.unchecked_transaction()?;
    for (machine, sessions) in journals {
        for s in sessions {
            let Some(&game_id) = games.get(&s.key) else {
                continue;
            };
            let seconds = s.seconds.max(0);
            let added = tx.execute(
                "INSERT OR IGNORE INTO play_sessions (game_id, started_at, seconds, origin)
                 VALUES (?1, ?2, ?3, ?4)",
                params![game_id, s.started, seconds, machine],
            )?;
            if added == 0 {
                continue;
            }
            tx.execute(
                "UPDATE games SET play_count = play_count + 1,
                                  play_seconds = play_seconds + ?2,
                                  last_played = MAX(COALESCE(last_played, 0), ?3)
                 WHERE id = ?1",
                params![game_id, seconds, s.started + seconds],
            )?;
            done.sessions += 1;
            done.seconds += seconds;
        }
    }
    tx.commit()?;
    Ok(done)
}

#[cfg(test)]
fn import_journals(conn: &Connection, cfg: &Config) -> Result<Imported> {
    apply_journals(conn, &read_journals(cfg))
}

/// The other machines syncing into this folder, most recently seen first.
pub fn other_machines(cfg: &Config) -> Vec<OtherMachine> {
    let Ok(entries) = std::fs::read_dir(cfg.root.join("machines")) else {
        return Vec::new();
    };
    let mut out: Vec<OtherMachine> = entries
        .flatten()
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<Machine>(&b).ok())
        .filter(|m| m.id != cfg.machine)
        .map(|m| OtherMachine {
            name: m.name,
            last_seen: m.updated_at,
        })
        .collect();
    out.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    out
}

// ---------------------------------------------------------------------------
// Saves
// ---------------------------------------------------------------------------

/// One of a game's save files, named by what follows the ROM's own name:
/// `.srm`, `.state1`, `.state1.png`. That is what carries across, since the
/// same game can be called something else on another machine.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalFile {
    pub suffix: String,
    /// "save", "state", or "shot" for a state's thumbnail.
    pub kind: String,
    pub path: PathBuf,
    pub modified: i64,
}

/// The files to sync, out of what `saves::find` turned up for a ROM.
pub fn local_files(found: &[SaveEntry], rom_name: &str, states: bool) -> Vec<LocalFile> {
    let mut out: Vec<LocalFile> = Vec::new();
    let suffix_of = |name: &str| -> Option<String> {
        // `find` only returns names that begin with the ROM's, then a dot.
        name.get(rom_name.len()..)
            .filter(|s| s.starts_with('.'))
            .map(str::to_string)
    };
    for entry in found {
        if entry.kind == "state" && !states {
            continue;
        }
        let Some(suffix) = suffix_of(&entry.name) else {
            continue;
        };
        // `find` lists the newest first, so where the same name turns up in
        // two folders the newer is the one that counts.
        if out.iter().any(|f| f.suffix == suffix) {
            continue;
        }
        out.push(LocalFile {
            suffix: suffix.clone(),
            kind: entry.kind.clone(),
            path: PathBuf::from(&entry.path),
            modified: entry.modified,
        });
        if let Some(shot) = &entry.screenshot {
            out.push(LocalFile {
                suffix: format!("{suffix}.png"),
                kind: "shot".into(),
                path: PathBuf::from(shot),
                modified: entry.modified,
            });
        }
    }
    out.sort_by(|a, b| a.suffix.cmp(&b.suffix));
    out
}

fn sha1_of(path: &Path) -> Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((hex::encode(hasher.finalize()), size))
}

/// One file in a version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ManifestFile {
    suffix: String,
    kind: String,
    size: u64,
    sha1: String,
}

/// The newest saves one machine has for one game.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    machine: String,
    name: String,
    title: String,
    key: String,
    version: String,
    digest: String,
    updated_at: i64,
    files: Vec<ManifestFile>,
}

/// One fingerprint for a whole set of files, to tell at a glance whether
/// anything in it changed.
fn digest_of(files: &[ManifestFile]) -> String {
    let mut sorted: Vec<&ManifestFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.suffix.cmp(&b.suffix));
    let mut h = Sha1::new();
    for f in sorted {
        h.update(f.suffix.as_bytes());
        h.update([0]);
        h.update(f.sha1.as_bytes());
        h.update([b'\n']);
    }
    hex::encode(h.finalize())
}

fn describe(local: &[LocalFile]) -> Result<Vec<ManifestFile>> {
    local
        .iter()
        .map(|f| {
            let (sha1, size) = sha1_of(&f.path)?;
            Ok(ManifestFile {
                suffix: f.suffix.clone(),
                kind: f.kind.clone(),
                size,
                sha1,
            })
        })
        .collect()
}

/// The local digest of a game's saves, for recording after a sync.
pub fn digest_local(local: &[LocalFile]) -> Result<String> {
    Ok(digest_of(&describe(local)?))
}

fn read_manifests(dir: &Path) -> Vec<Manifest> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            !name.starts_with('.') && !name.contains(".sync-conflict")
        })
        .filter_map(|p| std::fs::read(p).ok())
        .filter_map(|b| serde_json::from_slice::<Manifest>(&b).ok())
        .collect()
}

/// What this machine last synced for a game: the fingerprint of its own
/// files afterwards, and the version in the folder it matched.
#[derive(Debug, Clone, PartialEq)]
pub struct Synced {
    pub digest: String,
    pub version: String,
}

pub fn synced(conn: &Connection, game_id: i64) -> Result<Option<Synced>> {
    Ok(conn
        .query_row(
            "SELECT digest, version FROM save_sync WHERE game_id = ?1",
            params![game_id],
            |r| {
                Ok(Synced {
                    digest: r.get(0)?,
                    version: r.get(1)?,
                })
            },
        )
        .optional()?)
}

pub fn set_synced(conn: &Connection, game_id: i64, s: &Synced) -> Result<()> {
    conn.execute(
        "INSERT INTO save_sync (game_id, digest, version, synced_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(game_id) DO UPDATE SET digest = ?2, version = ?3, synced_at = ?4",
        params![game_id, s.digest, s.version, db::now()],
    )?;
    Ok(())
}

/// Everything about one game that a save sync needs.
pub struct GameSaves<'a> {
    pub key: &'a str,
    pub platform: &'a str,
    pub title: &'a str,
    pub rom_name: &'a str,
    pub local: &'a [LocalFile],
    pub last: Option<&'a Synced>,
}

/// What a pull did.
#[derive(Debug, PartialEq)]
pub enum Pull {
    /// Nothing newer anywhere else. `Some` when this machine turned out to
    /// match the folder already, and that should be remembered.
    Nothing(Option<Synced>),
    /// Copied in another machine's saves. `kept` is where this machine's own
    /// were backed up to first, if it had any.
    Took {
        from: String,
        version: String,
        files: usize,
        kept: Option<PathBuf>,
    },
    /// Both changed, and this machine's are newer: they stay, and go up
    /// after the game.
    KeptOurs { other: String },
    /// The newest version has not finished arriving in the sync folder.
    Arriving { from: String },
}

/// Bring in another machine's newer saves for a game, before it starts.
///
/// `place` says where a file this machine does not have yet should go; a
/// file it does have is replaced where it is. Every file is copied and
/// checked before any is put in place, so a version still arriving from
/// another machine changes nothing.
pub fn pull(
    cfg: &Config,
    game: &GameSaves,
    place: impl Fn(&str, &str) -> PathBuf,
    backup_root: &Path,
) -> Result<Pull> {
    let dir = game_dir(cfg, game.platform, game.key);
    let manifests = read_manifests(&dir);
    let Some(newest) = manifests
        .iter()
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at).then(a.machine.cmp(&b.machine)))
    else {
        return Ok(Pull::Nothing(None));
    };
    if game.last.is_some_and(|l| l.version == newest.version) {
        return Ok(Pull::Nothing(None));
    }

    let mine = describe(game.local)?;
    let local_digest = digest_of(&mine);
    if newest.digest == local_digest {
        return Ok(Pull::Nothing(Some(Synced {
            digest: local_digest,
            version: newest.version.clone(),
        })));
    }
    if newest.machine == cfg.machine {
        // The newest is our own, and our files have changed since: they go
        // up after this game.
        return Ok(Pull::Nothing(None));
    }

    let changed_here = match game.last {
        Some(l) => l.digest != local_digest,
        None => !game.local.is_empty(),
    };
    if changed_here {
        let ours = game.local.iter().map(|f| f.modified).max().unwrap_or(0);
        if ours >= newest.updated_at {
            return Ok(Pull::KeptOurs {
                other: newest.name.clone(),
            });
        }
    }

    // Copy everything beside where it is going, and check it, before
    // anything is replaced.
    let version_dir = dir.join("versions").join(&newest.version);
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    let unstage = |staged: &[(PathBuf, PathBuf)]| {
        for (tmp, _) in staged {
            let _ = std::fs::remove_file(tmp);
        }
    };
    for f in &newest.files {
        let from = version_dir.join(format!("save{}", f.suffix));
        let dest = game
            .local
            .iter()
            .find(|l| l.suffix == f.suffix)
            .map(|l| l.path.clone())
            .unwrap_or_else(|| place(&f.suffix, &f.kind).join(format!("{}{}", game.rom_name, f.suffix)));
        let Some(parent) = dest.parent() else {
            continue;
        };
        if std::fs::create_dir_all(parent).is_err() {
            unstage(&staged);
            return Err(AppError::Other(format!("Could not write to {}", parent.display())));
        }
        let tmp = parent.join(format!(
            ".{}.arcadex-sync",
            dest.file_name().unwrap_or_default().to_string_lossy()
        ));
        let arrived = std::fs::copy(&from, &tmp).is_ok()
            && sha1_of(&tmp).is_ok_and(|(sha1, size)| sha1 == f.sha1 && size == f.size);
        if !arrived {
            let _ = std::fs::remove_file(&tmp);
            unstage(&staged);
            return Ok(Pull::Arriving {
                from: newest.name.clone(),
            });
        }
        staged.push((tmp, dest));
    }

    // This machine's own go somewhere safe first.
    let replaced: Vec<SaveEntry> = game
        .local
        .iter()
        .map(|l| SaveEntry {
            kind: l.kind.clone(),
            slot: None,
            name: l.path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            path: l.path.to_string_lossy().to_string(),
            size: 0,
            modified: l.modified,
            screenshot: None,
        })
        .collect();
    let kept = if replaced.is_empty() {
        None
    } else {
        match crate::saves::back_up(&replaced, backup_root, game.title) {
            Ok(dir) => Some(dir),
            Err(e) => {
                unstage(&staged);
                return Err(e);
            }
        }
    };

    for (tmp, dest) in &staged {
        std::fs::rename(tmp, dest)?;
    }
    Ok(Pull::Took {
        from: newest.name.clone(),
        version: newest.version.clone(),
        files: staged.len(),
        kept,
    })
}

/// Put a game's saves in the folder, after it closes, if they changed.
///
/// Returns what to remember, or `None` when there was nothing new to send.
pub fn push(cfg: &Config, game: &GameSaves) -> Result<Option<Synced>> {
    if game.local.is_empty() {
        return Ok(None);
    }
    let files = describe(game.local)?;
    let digest = digest_of(&files);
    if game.last.is_some_and(|l| l.digest == digest) {
        return Ok(None);
    }

    let dir = game_dir(cfg, game.platform, game.key);
    let now = db::now();
    let version = format!("{now:012}-{}", cfg.machine);
    let version_dir = dir.join("versions").join(&version);
    std::fs::create_dir_all(&version_dir)?;
    for (local, f) in game.local.iter().zip(&files) {
        std::fs::copy(&local.path, version_dir.join(format!("save{}", f.suffix)))?;
    }

    // The manifest last, so a machine reading it finds the files already
    // there, at least on this side of the sync.
    let manifest = Manifest {
        machine: cfg.machine.clone(),
        name: cfg.name.clone(),
        title: game.title.to_string(),
        key: game.key.to_string(),
        version: version.clone(),
        digest: digest.clone(),
        updated_at: now,
        files,
    };
    write_atomic(
        &dir.join(format!("{}.json", cfg.machine)),
        &serde_json::to_vec_pretty(&manifest).map_err(|e| AppError::Other(e.to_string()))?,
    )?;

    prune_versions(&dir.join("versions"), &cfg.machine);
    Ok(Some(Synced { digest, version }))
}

/// Keep the newest few of this machine's own versions. Other machines'
/// versions are theirs to tidy.
fn prune_versions(versions: &Path, machine: &str) {
    let Ok(entries) = std::fs::read_dir(versions) else {
        return;
    };
    let suffix = format!("-{machine}");
    let mut mine: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&suffix))
        })
        .collect();
    mine.sort();
    if mine.len() > KEEP_VERSIONS {
        for old in &mine[..mine.len() - KEEP_VERSIONS] {
            let _ = std::fs::remove_dir_all(old);
        }
    }
}

// ---------------------------------------------------------------------------
// What launching, closing and Settings call. The library is held only for
// lookups; files are read and written with it released, since the folder may
// be a network share that takes its time.
// ---------------------------------------------------------------------------

fn lock(db: &Mutex<Connection>) -> Result<MutexGuard<'_, Connection>> {
    db.lock().map_err(|_| AppError::Other("The library is unavailable".into()))
}

/// One game's saves, with every lookup made while the library was held.
struct Prepared {
    cfg: Config,
    game_id: i64,
    key: String,
    platform: String,
    title: String,
    rom_name: String,
    exe: PathBuf,
    rom_path: PathBuf,
    core_folder: String,
    last: Option<Synced>,
}

impl Prepared {
    /// `None` when sync is off, or there is no RetroArch to find saves with.
    fn load(conn: &Connection, game: &Game) -> Option<Prepared> {
        let cfg = Config::load(conn).ok()??;
        let exe = crate::commands::retroarch_exe(conn).ok()?;
        let core = crate::launch::resolve_for_game(conn, game).core.unwrap_or_default();
        Some(Prepared {
            key: key_of(
                &game.platform,
                game.sha1.as_deref(),
                game.md5.as_deref(),
                game.crc32.as_deref(),
                game.size,
                game.serial.as_deref(),
                &game.filename,
            ),
            platform: game.platform.clone(),
            title: game.title.clone(),
            rom_name: crate::commands::rom_name_of(game),
            exe,
            rom_path: PathBuf::from(&game.path),
            core_folder: crate::cheats::core_folder_name(&core),
            last: synced(conn, game.id).ok().flatten(),
            game_id: game.id,
            cfg,
        })
    }

    fn local(&self) -> Vec<LocalFile> {
        let found = crate::saves::find(&self.exe, &self.rom_path, &self.rom_name, &self.core_folder);
        local_files(&found, &self.rom_name, self.cfg.states)
    }

    fn saves<'a>(&'a self, local: &'a [LocalFile]) -> GameSaves<'a> {
        GameSaves {
            key: &self.key,
            platform: &self.platform,
            title: &self.title,
            rom_name: &self.rom_name,
            local,
            last: self.last.as_ref(),
        }
    }
}

/// Before a game starts: bring in newer saves from another machine.
///
/// Returns a sentence to show, when there is something worth saying. Nothing
/// here stops the game starting: a sync that cannot happen is reported and
/// the game starts with the saves this machine has.
pub fn before_play(db: &Mutex<Connection>, game: &Game, data_dir: &Path) -> Option<String> {
    let p = Prepared::load(&*lock(db).ok()?, game)?;
    if !p.cfg.reachable() {
        return Some(format!(
            "The sync folder is not there, so {} starts with this computer's saves.",
            p.title
        ));
    }
    let local = p.local();
    let place = |_: &str, kind: &str| {
        crate::saves::target_dir(&p.exe, &p.rom_path, &p.core_folder, kind != "save")
    };
    let record = |s: Synced| {
        if let Ok(conn) = lock(db) {
            let _ = set_synced(&conn, p.game_id, &s);
        }
    };
    match pull(&p.cfg, &p.saves(&local), place, data_dir) {
        Ok(Pull::Nothing(found)) => {
            if let Some(s) = found {
                record(s);
            }
            None
        }
        Ok(Pull::Took { from, version, kept, .. }) => {
            if let Ok(digest) = digest_local(&p.local()) {
                record(Synced { digest, version });
            }
            Some(match kept {
                Some(dir) => format!(
                    "Brought in {}'s saves from {from}. This computer's are kept in {}.",
                    p.title,
                    dir.display()
                ),
                None => format!("Brought in {}'s saves from {from}.", p.title),
            })
        }
        Ok(Pull::KeptOurs { other }) => Some(format!(
            "{}'s saves here are newer than {other}'s, so these were kept. {other}'s stay in the sync folder.",
            p.title
        )),
        Ok(Pull::Arriving { from }) => Some(format!(
            "Saves from {from} are still arriving in the sync folder, so {} starts with this computer's.",
            p.title
        )),
        Err(e) => Some(format!("Could not sync {}'s saves: {e}", p.title)),
    }
}

/// After a game closes: send its saves if they changed, and this machine's
/// play time. Returns a sentence to show if something went wrong.
pub fn after_play(db: &Mutex<Connection>, game: &Game) -> Option<String> {
    let mut note = publish_play(db)
        .err()
        .map(|e| format!("Could not sync play time: {e}"));
    let p = Prepared::load(&*lock(db).ok()?, game)?;
    if !p.cfg.reachable() {
        return note;
    }
    let local = p.local();
    match push(&p.cfg, &p.saves(&local)) {
        Ok(Some(s)) => {
            if let Ok(conn) = lock(db) {
                let _ = set_synced(&conn, p.game_id, &s);
            }
        }
        Ok(None) => {}
        Err(e) => note = Some(format!("Could not sync {}'s saves: {e}", p.title)),
    }
    note
}

/// Write this machine's journal to the folder, when sync is on and the
/// folder is there.
fn publish_play(db: &Mutex<Connection>) -> Result<()> {
    let (cfg, text) = {
        let conn = lock(db)?;
        let Some(cfg) = Config::load(&conn)? else {
            return Ok(());
        };
        (cfg, journal_text(&conn)?.0)
    };
    if !cfg.reachable() {
        return Ok(());
    }
    publish_journal(&cfg, &text)
}

/// Read the other machines' play time in, then write this one's out.
/// `None` when sync is off.
pub fn sync_play(db: &Mutex<Connection>) -> Result<Option<Imported>> {
    let Some(cfg) = Config::load(&*lock(db)?)? else {
        return Ok(None);
    };
    if !cfg.reachable() {
        return Err(AppError::Other(format!(
            "{} is not there. Is the drive connected?",
            cfg.root.parent().unwrap_or(&cfg.root).display()
        )));
    }
    std::fs::create_dir_all(&cfg.root)?;
    let journals = read_journals(&cfg);
    let (imported, text) = {
        let conn = lock(db)?;
        (apply_journals(&conn, &journals)?, journal_text(&conn)?.0)
    };
    publish_journal(&cfg, &text)?;
    Ok(Some(imported))
}

/// What the Sync settings show.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub folder: Option<String>,
    pub reachable: bool,
    /// What this machine is called on the others.
    pub name: String,
    pub others: Vec<OtherMachine>,
}

pub fn status(db: &Mutex<Connection>) -> Result<Status> {
    let (cfg, name) = {
        let conn = lock(db)?;
        let name = db::get_setting(&conn, "sync_machine_name")?
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(hostname);
        (Config::load(&conn)?, name)
    };
    Ok(match cfg {
        None => Status {
            enabled: false,
            folder: None,
            reachable: false,
            name,
            others: Vec::new(),
        },
        Some(cfg) => {
            let reachable = cfg.reachable();
            Status {
                enabled: true,
                folder: cfg.root.parent().map(|p| p.to_string_lossy().to_string()),
                reachable,
                name,
                others: if reachable { other_machines(&cfg) } else { Vec::new() },
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "arcadex-sync-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn config(root: &Path, machine: &str) -> Config {
        Config {
            root: root.join("Synced").join("Arcadex"),
            machine: machine.into(),
            name: machine.to_uppercase(),
            states: true,
        }
    }

    fn library(dir: &Path, name: &str) -> Connection {
        let conn = db::open(&dir.join(format!("{name}.db"))).unwrap();
        conn.execute(
            "INSERT INTO games (id, path, filename, platform, title, added_at, sha1, size)
             VALUES (1, 'D:\\roms\\zelda.sfc', 'zelda.sfc', 'snes', 'Zelda', 0, 'ABC123', 10)",
            [],
        )
        .unwrap();
        conn
    }

    fn file(dir: &Path, name: &str, bytes: &[u8], modified: i64) -> LocalFile {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        LocalFile {
            suffix: name[name.find('.').unwrap()..].to_string(),
            kind: if name.contains(".state") { "state" } else { "save" }.into(),
            path,
            modified,
        }
    }

    fn saves<'a>(local: &'a [LocalFile], last: Option<&'a Synced>, rom: &'a str) -> GameSaves<'a> {
        GameSaves {
            key: "sha1:abc123",
            platform: "snes",
            title: "Zelda",
            rom_name: rom,
            local,
            last,
        }
    }

    #[test]
    fn the_same_dump_is_the_same_game_on_every_machine() {
        let a = key_of("snes", Some("ABC"), None, Some("1234"), 10, None, "Zelda.sfc");
        let b = key_of("snes", Some("abc"), None, None, 10, None, "Legend of Zelda (USA).sfc");
        assert_eq!(a, b);
        // Too big to checksum: the serial stands in.
        assert_eq!(
            key_of("ps3", None, None, None, 0, Some("BLUS30443"), "EBOOT.BIN"),
            "serial:ps3:blus30443"
        );
        // Nothing else to go on.
        assert_eq!(key_of("ps2", None, None, None, 0, None, "Game.ISO"), "name:ps2:game.iso");
    }

    #[test]
    fn a_machine_id_is_safe_for_a_file_name_and_unlikely_to_repeat() {
        let a = new_machine_id("Boaz's Desktop");
        assert!(a.starts_with("boaz-s-desktop-"), "{a}");
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_ne!(a, new_machine_id("Boaz's Desktop"));
        assert!(new_machine_id("").starts_with("machine-"));
    }

    /// Play on one machine, open the library on another, and the session is
    /// there - once, however often the journal is read.
    #[test]
    fn play_time_from_another_machine_is_added_once() {
        let dir = Scratch::new("play");
        let desk = library(&dir.0, "desk");
        let deck = library(&dir.0, "deck");
        db::record_play(&desk, 1, 1_000, 600).unwrap();
        db::record_play(&desk, 1, 5_000, 300).unwrap();

        assert_eq!(write_journal(&desk, &config(&dir.0, "desk")).unwrap(), 2);
        let deck_cfg = config(&dir.0, "deck");
        let first = import_journals(&deck, &deck_cfg).unwrap();
        assert_eq!((first.sessions, first.seconds), (2, 900));
        let again = import_journals(&deck, &deck_cfg).unwrap();
        assert_eq!(again.sessions, 0);

        let (count, seconds, last): (i64, i64, i64) = deck
            .query_row(
                "SELECT play_count, play_seconds, last_played FROM games WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((count, seconds, last), (2, 900, 5_300));

        // What the deck read in is not written back out as its own, or the
        // desk would count it twice.
        assert_eq!(write_journal(&deck, &deck_cfg).unwrap(), 0);
        assert_eq!(import_journals(&desk, &config(&dir.0, "desk")).unwrap().sessions, 0);
        assert_eq!(other_machines(&deck_cfg)[0].name, "DESK");
    }

    /// A session for a game this library does not have waits for it.
    #[test]
    fn a_session_for_a_missing_game_is_read_once_the_game_is_added() {
        let dir = Scratch::new("later");
        let desk = library(&dir.0, "desk");
        db::record_play(&desk, 1, 1_000, 60).unwrap();
        write_journal(&desk, &config(&dir.0, "desk")).unwrap();

        let deck = db::open(&dir.0.join("empty.db")).unwrap();
        let cfg = config(&dir.0, "deck");
        assert_eq!(import_journals(&deck, &cfg).unwrap().sessions, 0);
        deck.execute(
            "INSERT INTO games (id, path, filename, platform, title, added_at, sha1, size)
             VALUES (7, '/home/deck/roms/zelda.sfc', 'zelda.sfc', 'snes', 'Zelda', 0, 'abc123', 10)",
            [],
        )
        .unwrap();
        assert_eq!(import_journals(&deck, &cfg).unwrap().sessions, 1);
    }

    /// Save on the desk, start the game on the deck, and the deck has the
    /// desk's save under its own name for the ROM.
    #[test]
    fn saves_follow_the_game_to_the_other_machine() {
        let dir = Scratch::new("follow");
        let desk = config(&dir.0, "desk");
        let deck = config(&dir.0, "deck");

        let desk_files = [file(&dir.0.join("desk"), "Zelda.srm", b"desk progress", 100)];
        let pushed = push(&desk, &saves(&desk_files, None, "Zelda")).unwrap().unwrap();
        // Nothing changed, nothing more to send.
        assert_eq!(push(&desk, &saves(&desk_files, Some(&pushed), "Zelda")).unwrap(), None);

        let deck_saves = dir.0.join("deck-saves");
        let outcome = pull(
            &deck,
            &saves(&[], None, "Legend of Zelda (USA)"),
            |_, _| deck_saves.clone(),
            &dir.0.join("backups"),
        )
        .unwrap();
        assert!(matches!(outcome, Pull::Took { files: 1, kept: None, .. }), "{outcome:?}");
        assert_eq!(
            std::fs::read(deck_saves.join("Legend of Zelda (USA).srm")).unwrap(),
            b"desk progress"
        );
    }

    /// Pulling over saves this machine already has keeps a copy of them.
    #[test]
    fn saves_replaced_by_a_pull_are_backed_up_first() {
        let dir = Scratch::new("backup");
        let desk = config(&dir.0, "desk");
        let deck = config(&dir.0, "deck");

        // The deck synced an old save, then the desk played on.
        let deck_files = [file(&dir.0.join("deck"), "Zelda.srm", b"old", 50)];
        let deck_synced = Synced {
            digest: digest_local(&deck_files).unwrap(),
            version: "000000000001-deck".into(),
        };
        let desk_files = [file(&dir.0.join("desk"), "Zelda.srm", b"newer", 100)];
        push(&desk, &saves(&desk_files, None, "Zelda")).unwrap();

        let outcome = pull(
            &deck,
            &saves(&deck_files, Some(&deck_synced), "Zelda"),
            |_, _| unreachable!("the file is already there"),
            &dir.0.join("backups"),
        )
        .unwrap();
        let Pull::Took { kept: Some(kept), .. } = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(std::fs::read(&deck_files[0].path).unwrap(), b"newer");
        assert_eq!(std::fs::read(kept.join("Zelda.srm")).unwrap(), b"old");
    }

    /// Both machines played since they last synced. The newer wins.
    #[test]
    fn when_both_changed_the_newer_saves_win() {
        let dir = Scratch::new("both");
        let desk = config(&dir.0, "desk");
        let deck = config(&dir.0, "deck");
        let desk_files = [file(&dir.0.join("desk"), "Zelda.srm", b"desk", 100)];
        push(&desk, &saves(&desk_files, None, "Zelda")).unwrap();

        // The deck's own save is newer than anything in the folder.
        let far_future = db::now() + 3_600;
        let deck_files = [file(&dir.0.join("deck"), "Zelda.srm", b"deck", far_future)];
        let outcome = pull(&deck, &saves(&deck_files, None, "Zelda"), |_, _| unreachable!(), &dir.0)
            .unwrap();
        assert_eq!(outcome, Pull::KeptOurs { other: "DESK".into() });
        assert_eq!(std::fs::read(&deck_files[0].path).unwrap(), b"deck");

        // An older one gives way, and is backed up.
        let old_files = [file(&dir.0.join("old"), "Zelda.srm", b"stale", 1)];
        let outcome = pull(
            &deck,
            &saves(&old_files, None, "Zelda"),
            |_, _| unreachable!(),
            &dir.0.join("backups"),
        )
        .unwrap();
        assert!(matches!(outcome, Pull::Took { kept: Some(_), .. }), "{outcome:?}");
    }

    /// A sync tool can deliver the manifest before the files it lists.
    #[test]
    fn a_version_still_arriving_changes_nothing() {
        let dir = Scratch::new("arriving");
        let desk = config(&dir.0, "desk");
        let deck = config(&dir.0, "deck");
        let desk_files = [
            file(&dir.0.join("desk"), "Zelda.srm", b"save", 100),
            file(&dir.0.join("desk"), "Zelda.state1", b"state", 100),
        ];
        let pushed = push(&desk, &saves(&desk_files, None, "Zelda")).unwrap().unwrap();
        let version = game_dir(&desk, "snes", "sha1:abc123").join("versions").join(&pushed.version);
        std::fs::write(version.join("save.state1"), b"sta").unwrap();

        let deck_files = [file(&dir.0.join("deck"), "Zelda.srm", b"mine", 1)];
        let outcome = pull(&deck, &saves(&deck_files, None, "Zelda"), |_, _| dir.0.join("deck"), &dir.0)
            .unwrap();
        assert_eq!(outcome, Pull::Arriving { from: "DESK".into() });
        assert_eq!(std::fs::read(&deck_files[0].path).unwrap(), b"mine");
        assert!(!dir.0.join("deck").join("Zelda.state1").exists());
        assert!(!dir.0.join("deck").join(".Zelda.srm.arcadex-sync").exists());
    }

    /// Once a version has been pulled or matched, it is not pulled again.
    #[test]
    fn a_version_already_synced_is_not_pulled_twice() {
        let dir = Scratch::new("twice");
        let desk = config(&dir.0, "desk");
        let deck = config(&dir.0, "deck");
        let desk_files = [file(&dir.0.join("desk"), "Zelda.srm", b"save", 100)];
        let pushed = push(&desk, &saves(&desk_files, None, "Zelda")).unwrap().unwrap();

        // The deck already has exactly this: remember it, copy nothing.
        let same = [file(&dir.0.join("deck"), "Zelda.srm", b"save", 100)];
        let outcome = pull(&deck, &saves(&same, None, "Zelda"), |_, _| unreachable!(), &dir.0)
            .unwrap();
        assert_eq!(outcome, Pull::Nothing(Some(pushed.clone())));
        let outcome = pull(&deck, &saves(&same, Some(&pushed), "Zelda"), |_, _| unreachable!(), &dir.0)
            .unwrap();
        assert_eq!(outcome, Pull::Nothing(None));
    }

    #[test]
    fn each_machine_keeps_only_its_newest_versions() {
        let dir = Scratch::new("prune");
        let versions = dir.0.join("versions");
        for i in 0..(KEEP_VERSIONS + 3) {
            std::fs::create_dir_all(versions.join(format!("{i:012}-desk"))).unwrap();
        }
        std::fs::create_dir_all(versions.join("000000000000-deck")).unwrap();
        prune_versions(&versions, "desk");
        let left: Vec<String> = std::fs::read_dir(&versions)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left.iter().filter(|n| n.ends_with("-desk")).count(), KEEP_VERSIONS);
        assert!(!left.contains(&"000000000000-desk".to_string()));
        // Another machine's are never touched.
        assert!(left.contains(&"000000000000-deck".to_string()));
    }

    /// One machine, as the app sees it: a library, a RetroArch with its own
    /// saves folder, and the shared folder.
    fn machine(root: &Path, id: &str, rom: &str) -> (Mutex<Connection>, Game, PathBuf) {
        let ra = root.join(format!("{id}-retroarch"));
        std::fs::create_dir_all(ra.join("saves")).unwrap();
        std::fs::write(ra.join("retroarch.exe"), b"").unwrap();
        std::fs::write(
            ra.join("retroarch.cfg"),
            "savefile_directory = \":\\saves\"\nsavestate_directory = \":\\states\"\n",
        )
        .unwrap();
        let conn = db::open(&root.join(format!("{id}.db"))).unwrap();
        for (k, v) in [
            ("retroarch_path", ra.join("retroarch.exe").to_string_lossy().to_string()),
            ("sync_dir", root.join("Synced").to_string_lossy().to_string()),
            ("sync_machine_id", id.to_string()),
            ("sync_machine_name", id.to_uppercase()),
        ] {
            db::set_setting(&conn, k, &v).unwrap();
        }
        let path = root.join("roms").join(rom);
        conn.execute(
            "INSERT INTO games (id, path, filename, platform, title, added_at, sha1, size)
             VALUES (1, ?1, ?2, 'snes', 'Zelda', 0, 'abc123', 10)",
            params![path.to_string_lossy(), rom],
        )
        .unwrap();
        let game = db::get_game(&conn, 1).unwrap().unwrap();
        (Mutex::new(conn), game, ra.join("saves"))
    }

    /// The whole round: play on the desk, close the game, start it on the
    /// deck, and the deck's RetroArch finds the desk's save under the name
    /// the deck's copy of the ROM has.
    #[test]
    fn a_save_made_on_one_machine_is_there_when_the_game_starts_on_another() {
        let dir = Scratch::new("round");
        std::fs::create_dir_all(dir.0.join("Synced")).unwrap();
        let (desk, desk_game, desk_saves) = machine(&dir.0, "desk", "Zelda.sfc");
        let (deck, deck_game, deck_saves) = machine(&dir.0, "deck", "Legend of Zelda (USA).sfc");

        std::fs::write(desk_saves.join("Zelda.srm"), b"eight hearts").unwrap();
        db::record_play(&desk.lock().unwrap(), 1, 1_000, 1_200).unwrap();
        assert_eq!(after_play(&desk, &desk_game), None);

        let note = before_play(&deck, &deck_game, &dir.0.join("deck-data"));
        assert_eq!(note.as_deref(), Some("Brought in Zelda's saves from DESK."));
        assert_eq!(
            std::fs::read(deck_saves.join("Legend of Zelda (USA).srm")).unwrap(),
            b"eight hearts"
        );
        // Nothing new the second time.
        assert_eq!(before_play(&deck, &deck_game, &dir.0.join("deck-data")), None);

        // And the desk's play time.
        let got = sync_play(&deck).unwrap().unwrap();
        assert_eq!((got.sessions, got.seconds), (1, 1_200));
        let status = status(&deck).unwrap();
        assert!(status.enabled && status.reachable);
        assert_eq!(status.others[0].name, "DESK");
    }

    /// A folder that is not there - a NAS switched off - is said once, and
    /// the game still starts.
    #[test]
    fn a_missing_sync_folder_is_reported_and_nothing_else_happens() {
        let dir = Scratch::new("gone");
        let (desk, game, _) = machine(&dir.0, "desk", "Zelda.sfc");
        let note = before_play(&desk, &game, &dir.0).unwrap();
        assert!(note.starts_with("The sync folder is not there"), "{note}");
        assert_eq!(after_play(&desk, &game), None);
        assert!(sync_play(&desk).is_err());
    }

    #[test]
    fn states_stay_behind_when_asked_to() {
        let found = vec![
            SaveEntry {
                kind: "save".into(),
                slot: None,
                name: "Zelda.srm".into(),
                path: "/s/Zelda.srm".into(),
                size: 1,
                modified: 1,
                screenshot: None,
            },
            SaveEntry {
                kind: "state".into(),
                slot: Some(1),
                name: "Zelda.state1".into(),
                path: "/s/Zelda.state1".into(),
                size: 1,
                modified: 1,
                screenshot: Some("/s/Zelda.state1.png".into()),
            },
        ];
        let all: Vec<String> = local_files(&found, "Zelda", true).into_iter().map(|f| f.suffix).collect();
        assert_eq!(all, vec![".srm", ".state1", ".state1.png"]);
        let saves_only: Vec<String> =
            local_files(&found, "Zelda", false).into_iter().map(|f| f.suffix).collect();
        assert_eq!(saves_only, vec![".srm"]);
    }
}
