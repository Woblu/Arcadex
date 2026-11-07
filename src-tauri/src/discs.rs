//! Multi-disc games, gathered into one entry.
//!
//! A three-disc PlayStation game is three files, and a library that lists
//! files lists it three times. Worse, starting disc 2 from the beginning is
//! not how anyone plays: the game asks for the next disc partway through, and
//! the emulator has to be able to swap without being restarted.
//!
//! Both are the same fix. An `.m3u` naming the discs in order is what every
//! emulator that reads discs understands - RetroArch, PCSX2, DuckStation and
//! Dolphin all take one and handle the swapping themselves. So the discs are
//! recognised by their names, a playlist is written, and the set shows up as
//! the one game it always was.
//!
//! The playlists are written into Arcadex's own folder rather than beside the
//! ROMs. Nothing here writes to, moves or deletes anything in a library
//! folder; the discs themselves are only hidden, and "Show hidden" brings
//! them straight back.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::db;
use crate::error::Result;
use crate::models::Game;
use crate::scan::clean_title;

/// The words a disc tag is written with. Longest first, so `disc` is not read
/// as `d` followed by rubbish, and `cd` cannot swallow the `c` of `disc`.
const DISC_WORDS: &[&str] = &["disc", "disk", "cd"];

/// Read a disc tag - `Disc 2`, `CD 2`, `Disk 2 of 3` - and return the number.
///
/// The number has to be there. `Sonic CD` and `Wolfenstein 3D` say a system
/// or a title, not a position in a set, and a tag with no number is not a tag.
fn disc_number(tag: &str) -> Option<u32> {
    let lower = tag.trim().to_ascii_lowercase();
    let rest = DISC_WORDS
        .iter()
        .find_map(|w| lower.strip_prefix(w))?
        .trim_start();

    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    if n == 0 {
        return None;
    }

    // What follows must be nothing, or "of 3" saying how many there are.
    let after = rest[digits.len()..].trim_start();
    let after = after.strip_prefix("of").unwrap_or(after).trim_start();
    if after.is_empty() || after.chars().all(|c| c.is_ascii_digit()) {
        Some(n)
    } else {
        None
    }
}

/// Split a filename into the name the set shares and this file's disc number.
///
/// Returns `None` for anything that is not one disc of several, which is most
/// of a library.
pub fn split_disc(filename: &str) -> Option<(String, u32)> {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    // A bracketed tag, which is how No-Intro and Redump write it:
    // "Final Fantasy VII (USA) (Disc 2).cue".
    let chars: Vec<char> = stem.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        let close = match ch {
            '(' => ')',
            '[' => ']',
            _ => continue,
        };
        let Some(end) = chars[i + 1..].iter().position(|c| *c == close) else {
            continue;
        };
        let end = i + 1 + end;
        let inner: String = chars[i + 1..end].iter().collect();
        if let Some(n) = disc_number(&inner) {
            let mut base: String = chars[..i].iter().collect();
            base.push_str(&chars[end + 1..].iter().collect::<String>());
            return Some((tidy(&base), n));
        }
    }

    // A bare tag on the end, which is how people name their own rips:
    // "Final Fantasy VII - Disc 2.cue".
    let words: Vec<&str> = stem.split_whitespace().collect();
    for take in [2usize, 1] {
        if words.len() <= take {
            continue;
        }
        let tail = words[words.len() - take..].join(" ");
        if let Some(n) = disc_number(&tail) {
            let base = words[..words.len() - take].join(" ");
            return Some((tidy(&base), n));
        }
    }

    None
}

/// Tidy up what removing a tag left behind: doubled spaces, and the dash or
/// bracket that was holding the tag on.
fn tidy(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .to_string()
}

/// What two files must agree on to be discs of the same game. Region and
/// revision tags are deliberately kept: a USA disc 1 and a Europe disc 1 are
/// two different releases, not one game with a duplicate disc.
fn group_key(platform: &str, base: &str) -> (String, String) {
    (platform.to_string(), base.to_ascii_lowercase())
}

/// Strip what a filename cannot hold.
fn safe_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "playlist".to_string()
    } else {
        cleaned
    }
}

/// What a pass of grouping did.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Grouped {
    /// Sets that now have a playlist.
    pub sets: usize,
    /// Discs folded into them.
    pub discs: usize,
}

/// Rebuild every disc set from what is in the library right now.
///
/// This runs from scratch each time rather than trying to patch up what it
/// did last: sets gain discs, lose them, get renamed and get deleted, and one
/// rule applied to the current state is easier to trust than a pile of rules
/// for each way it can change. The playlist rows themselves are matched by
/// path and updated in place, so a set keeps its play time and its artwork.
pub fn regroup(conn: &rusqlite::Connection, playlist_root: &Path) -> Result<Grouped> {
    // One transaction for the lot: a write per disc each committing on its own
    // was slow on a big library, and held every other query waiting meanwhile.
    // A failure part way also leaves the sets as they were rather than half
    // released.
    let tx = conn.unchecked_transaction()?;
    let conn: &rusqlite::Connection = &tx;

    // Everything starts ungrouped, and is grouped again below if it still
    // belongs to a set.
    db::release_all_discs(conn)?;

    let mut sets: BTreeMap<(String, String), Vec<(u32, Game)>> = BTreeMap::new();
    for game in db::disc_candidates(conn)? {
        // A playlist only helps where the emulator reads one. Xenia does
        // not, and swaps an Xbox 360 game's discs itself when asked.
        if crate::platforms::NO_DISC_PLAYLISTS.contains(&game.platform.as_str()) {
            continue;
        }
        if let Some((base, n)) = split_disc(&game.filename) {
            if base.is_empty() {
                continue;
            }
            sets.entry(group_key(&game.platform, &base))
                .or_default()
                .push((n, game));
        }
    }

    let mut wanted: Vec<PathBuf> = Vec::new();
    let mut tally = Grouped::default();

    for ((platform, _), mut members) in sets {
        // One disc is a game, not a set.
        members.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.path.cmp(&b.1.path)));
        members.dedup_by_key(|(n, _)| *n);
        if members.len() < 2 {
            continue;
        }

        // The name the set shares, taken from a disc rather than rebuilt, so
        // it reads the way the rest of the library does.
        let base = split_disc(&members[0].1.filename)
            .map(|(b, _)| b)
            .unwrap_or_default();
        let title = clean_title(&base);

        let dir = playlist_root.join(&platform);
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(format!("{}.m3u", safe_name(&base)));

        let body: String = members
            .iter()
            .map(|(_, g)| g.path.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&file, format!("{body}\n"))?;

        let path = file.to_string_lossy().to_string();
        let filename = file.file_name().unwrap_or_default().to_string_lossy().to_string();
        let size: i64 = members.iter().map(|(_, g)| g.size).sum();
        let id = db::upsert_disc_playlist(conn, &path, &filename, &platform, &title, size)?;

        // A set scraped before it was grouped has its artwork on disc 1.
        if let Some((_, first)) = members.first() {
            if first.scrape_status == "ok" {
                db::copy_metadata(conn, first.id, id)?;
            }
        }

        let ids: Vec<i64> = members.iter().map(|(_, g)| g.id).collect();
        db::set_disc_members(conn, id, &ids)?;

        tally.sets += 1;
        tally.discs += ids.len();
        wanted.push(file);
    }

    // A set that is no longer a set - discs deleted, renamed, or moved to
    // another system - leaves a playlist behind. Its members were released at
    // the top, so removing the row here is all that is left to do.
    for stale in db::disc_playlists(conn)? {
        let path = PathBuf::from(&stale.path);
        if wanted.iter().any(|w| *w == path) {
            continue;
        }
        db::remove_game(conn, stale.id)?;
        let _ = std::fs::remove_file(&path);
    }

    tx.commit()?;
    Ok(tally)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_usual_ways_a_disc_is_tagged() {
        for (name, base, n) in [
            ("Final Fantasy VII (USA) (Disc 2).cue", "Final Fantasy VII (USA)", 2),
            ("Final Fantasy VII (USA) (Disc 2 of 3).cue", "Final Fantasy VII (USA)", 2),
            ("Metal Gear Solid [Disc 1].bin", "Metal Gear Solid", 1),
            ("Some Game (CD 2).chd", "Some Game", 2),
            ("Some Game (Disk2).chd", "Some Game", 2),
            ("Riven - Disc 3.iso", "Riven", 3),
            ("Riven Disc 3.iso", "Riven", 3),
            ("Riven CD3.iso", "Riven", 3),
        ] {
            assert_eq!(
                split_disc(name),
                Some((base.to_string(), n)),
                "reading {name}"
            );
        }
    }

    /// A number in a title is not a disc number, and a system's name is not a
    /// tag. Getting this wrong would quietly merge unrelated games.
    #[test]
    fn leaves_ordinary_names_alone() {
        for name in [
            "Sonic CD (USA).cue",
            "Sonic the Hedgehog 2 (World).md",
            "Wolfenstein 3D.iso",
            "Final Fantasy VII (USA).cue",
            "Tekken 3.bin",
            "Disc Golf.nes",
        ] {
            assert_eq!(split_disc(name), None, "reading {name}");
        }
    }

    /// Region tags stay in the key. Two releases of the same game each have a
    /// disc 1, and folding them together would produce a playlist that plays
    /// half of one and half of the other.
    #[test]
    fn keeps_separate_releases_apart() {
        let (usa, _) = split_disc("Final Fantasy VII (USA) (Disc 1).cue").unwrap();
        let (eur, _) = split_disc("Final Fantasy VII (Europe) (Disc 1).cue").unwrap();
        assert_ne!(group_key("ps1", &usa), group_key("ps1", &eur));
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-discs-{}-{}-{:?}",
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

    fn add(conn: &rusqlite::Connection, path: &str, platform: &str) {
        let filename = Path::new(path).file_name().unwrap().to_string_lossy().to_string();
        conn.execute(
            "INSERT INTO games (path, filename, platform, title, size, added_at)
             VALUES (?1, ?2, ?3, ?4, 100, 0)",
            rusqlite::params![path, filename, platform, clean_title(&filename)],
        )
        .unwrap();
    }

    fn titles(conn: &rusqlite::Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT title FROM games WHERE hidden = 0 ORDER BY title")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    #[test]
    fn three_discs_become_one_game() {
        let dir = scratch("group");
        let conn = db::open(&dir.join("test.db")).unwrap();
        for n in 1..=3 {
            add(
                &conn,
                &format!("C:/roms/ps1/Final Fantasy VII (USA) (Disc {n}).cue"),
                "ps1",
            );
        }
        add(&conn, "C:/roms/ps1/Tekken 3 (USA).cue", "ps1");

        let root = dir.join("playlists");
        let tally = regroup(&conn, &root).unwrap();
        assert_eq!(tally, Grouped { sets: 1, discs: 3 });

        assert_eq!(
            titles(&conn),
            vec!["Final Fantasy VII".to_string(), "Tekken 3".to_string()],
            "the three discs are one entry, and the single-disc game is untouched"
        );

        // And the playlist really names the discs, in order.
        let m3u = root.join("ps1").join("Final Fantasy VII (USA).m3u");
        let text = std::fs::read_to_string(&m3u).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("(Disc 1).cue"), "{}", lines[0]);
        assert!(lines[2].ends_with("(Disc 3).cue"), "{}", lines[2]);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Xenia cannot open an .m3u, so an Xbox 360 game's discs stay as they
    /// are rather than disappearing into a playlist nothing will start.
    #[test]
    fn leaves_discs_alone_where_the_emulator_cannot_read_a_playlist() {
        let dir = scratch("noplaylist");
        let conn = db::open(&dir.join("test.db")).unwrap();
        for n in 1..=2 {
            add(&conn, &format!("C:/roms/x360/Mass Effect 2 (USA) (Disc {n}).iso"), "xbox360");
        }
        let root = dir.join("playlists");
        assert_eq!(regroup(&conn, &root).unwrap(), Grouped::default());
        assert_eq!(titles(&conn).len(), 2);
        assert!(!root.join("xbox360").exists());

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Running it twice must not build a second playlist beside the first,
    /// and must not lose what the set has accumulated.
    #[test]
    fn grouping_again_changes_nothing() {
        let dir = scratch("idempotent");
        let conn = db::open(&dir.join("test.db")).unwrap();
        for n in 1..=2 {
            add(&conn, &format!("C:/roms/ps1/Riven (Disc {n}).cue"), "ps1");
        }
        let root = dir.join("playlists");

        assert_eq!(regroup(&conn, &root).unwrap(), Grouped { sets: 1, discs: 2 });
        let id: i64 = conn
            .query_row(
                "SELECT id FROM games WHERE is_disc_playlist = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute("UPDATE games SET play_seconds = 900 WHERE id = ?1", [id])
            .unwrap();

        assert_eq!(regroup(&conn, &root).unwrap(), Grouped { sets: 1, discs: 2 });
        let (count, seconds): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), MAX(play_seconds) FROM games WHERE is_disc_playlist = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1, "one playlist, not two");
        assert_eq!(seconds, 900, "and it kept its play time");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Delete a disc and the set stops being a set. The remaining disc comes
    /// back into the library rather than disappearing with the playlist.
    #[test]
    fn losing_a_disc_puts_the_rest_back() {
        let dir = scratch("shrink");
        let conn = db::open(&dir.join("test.db")).unwrap();
        for n in 1..=2 {
            add(&conn, &format!("C:/roms/ps1/Riven (Disc {n}).cue"), "ps1");
        }
        let root = dir.join("playlists");
        regroup(&conn, &root).unwrap();
        let m3u = root.join("ps1").join("Riven.m3u");
        assert!(m3u.exists());

        conn.execute("DELETE FROM games WHERE filename LIKE '%Disc 2%'", [])
            .unwrap();
        assert_eq!(regroup(&conn, &root).unwrap(), Grouped::default());

        assert_eq!(titles(&conn), vec!["Riven".to_string()]);
        assert!(!m3u.exists(), "and the playlist file is gone with it");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
