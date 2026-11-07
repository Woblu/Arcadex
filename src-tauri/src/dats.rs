//! Verifying dumps against No-Intro and Redump DATs.
//!
//! A DAT is a catalogue: for every known-good dump of a system, its exact size
//! and checksums. That is all. It turns "here are your files" into "this one
//! is a correct dump, that one is not in the database, and those two are the
//! same game twice", which is the question a ROM library exists to answer and
//! the reason everything here is hashed in the first place.
//!
//! The DATs themselves are not shipped or fetched. No-Intro and Redump publish
//! through a click-through page with no API, so they are imported from files
//! you supply. That also means Redump's disc systems work exactly as well as
//! No-Intro's cartridges, which a mirror would not give us.

use std::collections::HashMap;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::{params, Connection};

use crate::error::{AppError, Result};

/// One known-good dump.
#[derive(Debug, Clone)]
pub struct DatRom {
    /// The game's catalogue name, which is also the No-Intro filename.
    pub name: String,
    pub size: i64,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
}

/// What a DAT file turned out to contain.
#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    /// The DAT's own name, from its header.
    pub dat_name: String,
    pub added: usize,
    /// Entries already catalogued, matched on checksum.
    pub duplicates: usize,
}

/// Read a No-Intro or Redump DAT.
///
/// Both use the same shape - a `<game>` holding one or more `<rom>` elements
/// carrying `size`, `crc`, `md5` and `sha1` - so one reader covers both. Multi
/// track disc entries have several `<rom>` children; each is kept, because each
/// is separately checkable and a disc's tracks are what you actually hold.
pub fn parse(path: &Path) -> Result<(String, Vec<DatRom>)> {
    let mut reader = Reader::from_file(path)
        .map_err(|e| AppError::Other(format!("Could not open the DAT: {e}")))?;
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut roms = Vec::new();
    let mut dat_name = String::new();
    let mut in_header = false;
    let mut capture_name = false;

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| AppError::Other(format!("This does not read as a DAT file: {e}")))?;

        match event {
            Event::Eof => break,

            Event::Start(ref e) => match e.name().as_ref() {
                "header" => in_header = true,
                "name" if in_header => capture_name = true,
                "rom" => {
                    if let Some(rom) = rom_from(e) {
                        roms.push(rom);
                    }
                }
                _ => {}
            },

            // `<rom .../>` is normally self-closing.
            Event::Empty(ref e) => {
                if e.name().as_ref() == "rom" {
                    if let Some(rom) = rom_from(e) {
                        roms.push(rom);
                    }
                }
            }

            Event::End(ref e) => match e.name().as_ref() {
                "header" => in_header = false,
                "name" => capture_name = false,
                _ => {}
            },

            Event::Text(ref t) => {
                if capture_name && dat_name.is_empty() {
                    dat_name = t.trim().to_string();
                }
            }

            _ => {}
        }
        buf.clear();
    }

    if roms.is_empty() {
        return Err(AppError::Other(
            "No dumps found in that file. It should be a No-Intro or Redump DAT.".into(),
        ));
    }
    if dat_name.is_empty() {
        dat_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported DAT")
            .to_string();
    }
    Ok((dat_name, roms))
}

/// Pull one dump out of a `<rom>` element.
fn rom_from(e: &quick_xml::events::BytesStart) -> Option<DatRom> {
    let mut rom = DatRom {
        name: String::new(),
        size: 0,
        crc32: None,
        md5: None,
        sha1: None,
    };
    for attr in e.attributes().flatten() {
        // DATs carry no XML declaration, so 1.0 is what the spec says to
        // assume, and what quick-xml's own default is.
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .unwrap_or_default()
            .to_string();
        match attr.key.as_ref() {
            "name" => rom.name = value,
            "size" => rom.size = value.parse().unwrap_or(0),
            // Checksums appear in either case in the wild, so they are stored
            // in one case and compared in one case.
            "crc" => rom.crc32 = Some(value.to_ascii_lowercase()),
            "md5" => rom.md5 = Some(value.to_ascii_lowercase()),
            "sha1" => rom.sha1 = Some(value.to_ascii_lowercase()),
            _ => {}
        }
    }
    (rom.crc32.is_some() || rom.sha1.is_some() || rom.md5.is_some()).then_some(rom)
}

/// Store a parsed DAT, ignoring dumps already catalogued.
pub fn store(conn: &Connection, dat_name: &str, roms: &[DatRom]) -> Result<ImportSummary> {
    let mut summary = ImportSummary {
        dat_name: dat_name.to_string(),
        ..Default::default()
    };

    let tx = conn.unchecked_transaction()?;
    {
        let mut insert = tx.prepare(
            "INSERT OR IGNORE INTO dat_roms (dat_name, name, size, crc32, md5, sha1)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for rom in roms {
            let changed = insert.execute(params![
                dat_name,
                rom.name,
                rom.size,
                rom.crc32,
                rom.md5,
                rom.sha1
            ])?;
            if changed > 0 {
                summary.added += 1;
            } else {
                summary.duplicates += 1;
            }
        }
    }
    tx.commit()?;
    Ok(summary)
}

/// What the catalogue says about one game.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Hashes match a catalogued dump exactly.
    Verified { name: String, dat_name: String },
    /// Hashed, and no catalogued dump has those hashes.
    NotInDatabase,
    /// Nothing to check against yet.
    NoDats,
    /// Too large to have been hashed, so there is nothing to compare.
    NotHashed,
}

/// Check one game against everything catalogued.
///
/// SHA-1 first, then MD5, then CRC32 - strongest evidence available. CRC32 is
/// last because it is short enough that collisions are conceivable, and a DAT
/// is meant to settle arguments rather than start them.
pub fn verify(
    conn: &Connection,
    crc32: Option<&str>,
    md5: Option<&str>,
    sha1: Option<&str>,
) -> Result<Verdict> {
    if count(conn)? == 0 {
        return Ok(Verdict::NoDats);
    }
    verify_against(conn, crc32, md5, sha1)
}

/// The lookup itself, for when it is already known that DATs are loaded.
/// Checking a whole library through `verify` counted the catalogue once per
/// game, which on a big library and a big DAT froze the Verification tab.
fn verify_against(
    conn: &Connection,
    crc32: Option<&str>,
    md5: Option<&str>,
    sha1: Option<&str>,
) -> Result<Verdict> {
    if crc32.is_none() && md5.is_none() && sha1.is_none() {
        return Ok(Verdict::NotHashed);
    }

    for (column, value) in [("sha1", sha1), ("md5", md5), ("crc32", crc32)] {
        let Some(value) = value.filter(|v| !v.is_empty()) else {
            continue;
        };
        let sql = format!(
            "SELECT name, dat_name FROM dat_roms WHERE {column} = ?1 LIMIT 1"
        );
        let found: Option<(String, String)> = conn
            .query_row(&sql, params![value.to_ascii_lowercase()], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .ok();
        if let Some((name, dat_name)) = found {
            return Ok(Verdict::Verified { name, dat_name });
        }
    }
    Ok(Verdict::NotInDatabase)
}

pub fn count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM dat_roms", [], |r| r.get(0))?)
}

/// Which DATs are loaded, and how many dumps each brought.
pub fn loaded(conn: &Connection) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT dat_name, COUNT(*) FROM dat_roms GROUP BY dat_name ORDER BY dat_name",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM dat_roms", [])?;
    Ok(())
}

/// Verify every hashed game at once, for the summary on the settings page.
pub fn verify_library(conn: &Connection) -> Result<HashMap<String, usize>> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    // Asked once, not once per game: `verify` checks whether anything is
    // catalogued at all, and a library of two thousand games would otherwise
    // run two thousand identical COUNT(*) queries to learn the same thing.
    if count(conn)? == 0 {
        let total: usize = conn.query_row("SELECT COUNT(*) FROM games", [], |r| {
            r.get::<_, i64>(0)
        })? as usize;
        counts.insert("noDats".to_string(), total);
        return Ok(counts);
    }

    let mut stmt = conn.prepare("SELECT crc32, md5, sha1 FROM games")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;

    for row in rows {
        let (crc32, md5, sha1) = row?;
        let verdict = verify_against(conn, crc32.as_deref(), md5.as_deref(), sha1.as_deref())?;
        let key = match verdict {
            Verdict::Verified { .. } => "verified",
            Verdict::NotInDatabase => "unknown",
            Verdict::NoDats => "noDats",
            Verdict::NotHashed => "notHashed",
        };
        *counts.entry(key.to_string()).or_insert(0) += 1;
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<datafile>
  <header>
    <name>Nintendo - Nintendo Entertainment System</name>
    <version>20240101</version>
  </header>
  <game name="Super Mario Bros. (World)">
    <description>Super Mario Bros. (World)</description>
    <rom name="Super Mario Bros. (World).nes" size="40976" crc="3337EC46" md5="811B027EAF99C2DEF7B933C5208636DE" sha1="EA343F4E445A9050D4B4FBAC2C77D0693B1D0922"/>
  </game>
  <game name="Tetris (USA)">
    <rom name="Tetris (USA).nes" size="24592" crc="6D72C53A" sha1="AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"/>
  </game>
</datafile>
"#;

    fn temp_db() -> Connection {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-dat-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        crate::db::open(&dir.join("library.db")).unwrap()
    }

    fn write_sample() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-datfile-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nes.dat");
        std::fs::write(&path, SAMPLE).unwrap();
        path
    }

    #[test]
    fn reads_a_dat_and_its_name() {
        let path = write_sample();
        let (name, roms) = parse(&path).unwrap();
        assert_eq!(name, "Nintendo - Nintendo Entertainment System");
        assert_eq!(roms.len(), 2);
        assert_eq!(roms[0].name, "Super Mario Bros. (World).nes");
        assert_eq!(roms[0].size, 40976);
        // Checksums are stored lower case, however the DAT wrote them.
        assert_eq!(roms[0].crc32.as_deref(), Some("3337ec46"));
        assert!(roms[0].sha1.as_deref().unwrap().starts_with("ea343f"));
        // An entry with no MD5 is still usable.
        assert_eq!(roms[1].md5, None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn verifies_a_dump_and_notices_one_that_is_not_catalogued() {
        let conn = temp_db();
        let path = write_sample();
        let (name, roms) = parse(&path).unwrap();

        let summary = store(&conn, &name, &roms).unwrap();
        assert_eq!(summary.added, 2);

        // Importing the same DAT twice catalogues nothing new.
        let again = store(&conn, &name, &roms).unwrap();
        assert_eq!(again.added, 0);
        assert_eq!(again.duplicates, 2);
        assert_eq!(count(&conn).unwrap(), 2);

        // A correct dump, matched on SHA-1.
        let verdict = verify(
            &conn,
            Some("3337ec46"),
            None,
            Some("ea343f4e445a9050d4b4fbac2c77d0693b1d0922"),
        )
        .unwrap();
        assert_eq!(
            verdict,
            Verdict::Verified {
                name: "Super Mario Bros. (World).nes".into(),
                dat_name: "Nintendo - Nintendo Entertainment System".into(),
            }
        );

        // Case in the library must not matter either.
        assert!(matches!(
            verify(&conn, Some("3337EC46"), None, None).unwrap(),
            Verdict::Verified { .. }
        ));

        // Hashes nobody has catalogued.
        assert_eq!(
            verify(&conn, Some("deadbeef"), None, None).unwrap(),
            Verdict::NotInDatabase
        );

        // A disc image too large to have been hashed has nothing to compare.
        assert_eq!(verify(&conn, None, None, None).unwrap(), Verdict::NotHashed);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn says_so_when_nothing_has_been_imported() {
        let conn = temp_db();
        assert_eq!(
            verify(&conn, Some("3337ec46"), None, None).unwrap(),
            Verdict::NoDats
        );
    }

    #[test]
    fn refuses_a_file_that_is_not_a_dat() {
        let dir = std::env::temp_dir().join(format!("arcadex-notdat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nope.xml");
        std::fs::write(&path, "<html><body>hello</body></html>").unwrap();
        assert!(parse(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
