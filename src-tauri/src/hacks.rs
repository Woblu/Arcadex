//! Patch catalog: bulk-import ROM hack patches and index them so they can be
//! matched against the library.
//!
//! Two sources are supported. A folder of patches is indexed where it sits —
//! nothing is copied. A `.7z` bundle (the shape the community archives are
//! distributed in) is streamed, and only the patch files inside are extracted;
//! the documentation, source code and everything else is skipped, so a 35 GB
//! bundle costs a read rather than 35 GB of disk.

use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use walkdir::WalkDir;

use crate::db;
use crate::error::{AppError, Result};
use crate::models::ImportProgress;
use crate::patch;
use crate::platforms;

pub const PATCH_EXTS: &[&str] = &["ips", "bps", "ups"];

fn is_patch_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| PATCH_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Guess a platform from any directory name along the path. The community
/// archives group by system ("GBA", "SNES-SFC", "Genesis-MD"), so this usually
/// lands.
fn system_from_path(rel: &str) -> Option<String> {
    Path::new(rel)
        .parent()?
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .find_map(|dir| platforms::match_alias(dir).map(|p| p.slug.to_string()))
}

/// The immediate parent folder, which in these archives is a shortened
/// No-Intro name for the ROM the patch targets.
fn target_from_path(rel: &str) -> Option<String> {
    Path::new(rel)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

/// One safe filename component, for an archive we downloaded ourselves.
pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("bundle");
    let cleaned: String = base
        .chars()
        .map(|c| if r#":*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    if cleaned.trim().is_empty() {
        "bundle.7z".to_string()
    } else {
        cleaned
    }
}

/// Strip anything that could escape the destination directory.
fn safe_relative(name: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for comp in Path::new(&name.replace('\\', "/")).components() {
        match comp {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            // `..`, absolute paths and drive prefixes are never trusted.
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

struct Tally {
    scanned: usize,
    imported: usize,
    skipped: usize,
}

/// Progress is reported through a callback rather than straight to Tauri, so
/// the importer can be exercised without an app handle.
pub type ProgressFn<'a> = &'a mut dyn FnMut(ImportProgress);

fn emit(on_progress: ProgressFn, tally: &Tally, message: &str, done: bool) {
    on_progress(ImportProgress {
        scanned: tally.scanned,
        imported: tally.imported,
        skipped: tally.skipped,
        message: message.to_string(),
        done,
    });
}

/// Index one patch. `stored_path` is where the file now lives; `rel` is its
/// path within the source, used to guess the system and target ROM.
fn index_patch(
    db_handle: &Mutex<rusqlite::Connection>,
    bytes: &[u8],
    stored_path: &Path,
    rel: &str,
    origin: &str,
    tally: &mut Tally,
) {
    let Ok(info) = patch::inspect(bytes) else {
        tally.skipped += 1;
        return;
    };

    let name = Path::new(rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("patch")
        .to_string();

    let crc = info.source_crc.map(|c| format!("{c:08X}"));

    let conn = match db_handle.lock() {
        Ok(c) => c,
        Err(_) => {
            tally.skipped += 1;
            return;
        }
    };

    match db::insert_patch(
        &conn,
        &stored_path.to_string_lossy(),
        &name,
        info.format.as_str(),
        crc.as_deref(),
        system_from_path(rel).as_deref(),
        target_from_path(rel).as_deref(),
        origin,
    ) {
        Ok(true) => tally.imported += 1,
        _ => tally.skipped += 1,
    }
}

/// Import patches from a folder or a `.7z` bundle.
pub fn import(
    on_progress: ProgressFn,
    db_handle: &Mutex<rusqlite::Connection>,
    patches_root: &Path,
    source: &Path,
) -> Result<(usize, usize)> {
    let mut tally = Tally {
        scanned: 0,
        imported: 0,
        skipped: 0,
    };

    if !source.exists() {
        return Err(AppError::Other(format!(
            "Not found: {}",
            source.to_string_lossy()
        )));
    }

    emit(on_progress, &tally, "Reading…", false);

    if source.is_dir() {
        import_dir(on_progress, db_handle, source, &mut tally)?;
    } else {
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "7z" => import_7z(on_progress, db_handle, patches_root, source, &mut tally)?,
            e if PATCH_EXTS.contains(&e) => {
                let bytes = std::fs::read(source)?;
                let rel = source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("patch")
                    .to_string();
                tally.scanned += 1;
                index_patch(
                    db_handle,
                    &bytes,
                    source,
                    &rel,
                    &source.to_string_lossy(),
                    &mut tally,
                );
            }
            _ => {
                return Err(AppError::Other(
                    "Pick a folder of patches, a .7z bundle, or a single patch file".into(),
                ))
            }
        }
    }

    emit(
        on_progress,
        &tally,
        &format!("{} added, {} skipped", tally.imported, tally.skipped),
        true,
    );

    Ok((tally.imported, tally.skipped))
}

fn import_dir(
    on_progress: ProgressFn,
    db_handle: &Mutex<rusqlite::Connection>,
    dir: &Path,
    tally: &mut Tally,
) -> Result<()> {
    for entry in WalkDir::new(dir).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_patch_name(&path.to_string_lossy()) {
            continue;
        }

        tally.scanned += 1;
        let rel = path
            .strip_prefix(dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        match std::fs::read(path) {
            // Patches found on disk are catalogued in place, not copied.
            Ok(bytes) => index_patch(
                db_handle,
                &bytes,
                path,
                &rel,
                &dir.to_string_lossy(),
                tally,
            ),
            Err(_) => tally.skipped += 1,
        }

        if tally.scanned % 25 == 0 {
            emit(on_progress, tally, &rel, false);
        }
    }
    Ok(())
}

fn import_7z(
    on_progress: ProgressFn,
    db_handle: &Mutex<rusqlite::Connection>,
    patches_root: &Path,
    archive: &Path,
    tally: &mut Tally,
) -> Result<()> {
    let stem = archive
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("bundle")
        .to_string();
    let dest_root = patches_root.join(&stem);

    let mut reader = sevenz_rust2::ArchiveReader::open(
        archive,
        sevenz_rust2::Password::empty(),
    )
    .map_err(|e| AppError::Other(format!("Could not open archive: {e}")))?;

    let origin = archive.to_string_lossy().to_string();
    let mut last_error: Option<String> = None;

    reader
        .for_each_entries(|entry, rd| {
            if entry.is_directory || !is_patch_name(&entry.name) {
                return Ok(true);
            }

            tally.scanned += 1;

            let Some(rel) = safe_relative(&entry.name) else {
                tally.skipped += 1;
                return Ok(true);
            };

            let mut bytes = Vec::new();
            if rd.read_to_end(&mut bytes).is_err() {
                tally.skipped += 1;
                return Ok(true);
            }

            let dest = dest_root.join(&rel);
            if let Some(parent) = dest.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    tally.skipped += 1;
                    return Ok(true);
                }
            }
            if let Err(e) = std::fs::write(&dest, &bytes) {
                last_error = Some(e.to_string());
                tally.skipped += 1;
                return Ok(true);
            }

            let rel_str = rel.to_string_lossy().to_string();
            index_patch(db_handle, &bytes, &dest, &rel_str, &origin, tally);
            Ok(true)
        })
        .map_err(|e| AppError::Other(format!("Could not read archive: {e}")))?;

    emit(on_progress, tally, "Finishing…", false);

    if tally.imported == 0 {
        if let Some(e) = last_error {
            return Err(AppError::Other(format!("No patches extracted: {e}")));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_patch_extensions() {
        assert!(is_patch_name("Kaizo.bps"));
        assert!(is_patch_name("hack.IPS"));
        assert!(is_patch_name("a/b/c.ups"));
        assert!(!is_patch_name("readme.txt"));
        assert!(!is_patch_name("rom.gba"));
    }

    #[test]
    fn guesses_system_and_target_from_folders() {
        let rel = "GBA/Pokemon - Emerald Version (USA)/Kaizo Emerald.bps";
        assert_eq!(system_from_path(rel).as_deref(), Some("gba"));
        assert_eq!(
            target_from_path(rel).as_deref(),
            Some("Pokemon - Emerald Version (USA)")
        );
    }

    /// Minimal BPS encoder, enough to produce a patch the importer can read.
    fn build_test_bps(source: &[u8], target: &[u8]) -> Vec<u8> {
        fn varint(out: &mut Vec<u8>, mut value: u64) {
            loop {
                let x = (value & 0x7f) as u8;
                value >>= 7;
                if value == 0 {
                    out.push(0x80 | x);
                    break;
                }
                out.push(x);
                value -= 1;
            }
        }

        let mut patch = b"BPS1".to_vec();
        varint(&mut patch, source.len() as u64);
        varint(&mut patch, target.len() as u64);
        varint(&mut patch, 0);
        varint(&mut patch, ((target.len() as u64 - 1) << 2) | 1); // TargetRead
        patch.extend_from_slice(target);
        patch.extend_from_slice(&patch::crc32(source).to_le_bytes());
        patch.extend_from_slice(&patch::crc32(target).to_le_bytes());
        let so_far = patch::crc32(&patch);
        patch.extend_from_slice(&so_far.to_le_bytes());
        patch
    }

    /// Walk a realistic folder layout, catalogue the patch, and find it again
    /// by the checksum of the ROM it targets.
    #[test]
    fn imports_a_folder_and_matches_by_crc() {
        let tmp = std::env::temp_dir().join(format!(
            "arcadex-import-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let game_dir = tmp.join("GBA").join("Pokemon - Emerald Version (USA)");
        std::fs::create_dir_all(&game_dir).unwrap();

        let source = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let target = vec![9u8, 8, 7, 6];
        std::fs::write(
            game_dir.join("Kaizo Emerald.bps"),
            build_test_bps(&source, &target),
        )
        .unwrap();
        // Everything that is not a patch should be ignored.
        std::fs::write(game_dir.join("readme.txt"), b"notes").unwrap();

        let handle = Mutex::new(crate::db::open(&tmp.join("library.db")).unwrap());
        let mut progress = |_: ImportProgress| {};
        let (imported, _) =
            import(&mut progress, &handle, &tmp.join("extracted"), &tmp).unwrap();
        assert_eq!(imported, 1, "only the .bps should be catalogued");

        let conn = handle.lock().unwrap();
        let crc = format!("{:08X}", patch::crc32(&source));
        let hits = db::patches_for_crc(&conn, &crc).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Kaizo Emerald");
        assert_eq!(hits[0].format, "BPS");
        assert_eq!(hits[0].system_hint.as_deref(), Some("gba"));
        assert_eq!(
            hits[0].target_hint.as_deref(),
            Some("Pokemon - Emerald Version (USA)")
        );

        // A different ROM must not match.
        assert!(db::patches_for_crc(&conn, "DEADBEEF").unwrap().is_empty());

        drop(conn);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_paths_that_escape_the_destination() {
        assert!(safe_relative("../../evil.bps").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert!(safe_relative("").is_none());
        assert_eq!(
            safe_relative("GBA/hack.bps"),
            Some(PathBuf::from("GBA").join("hack.bps"))
        );
    }
}
