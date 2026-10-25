//! One-pass CRC32 / MD5 / SHA1 hashing.
//!
//! Scrapers and DAT files (No-Intro, Redump) hash the *ROM*, not the archive
//! that wraps it, so for a zip or a 7z we hash the entry inside rather than
//! the file on disk. Hashing the container instead would be worse than not
//! hashing at all: it matches nothing, but looks like a real dump hash.

use md5::{Digest, Md5};
use sha1::Sha1;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::platforms;

#[derive(Debug, Clone, Default)]
pub struct Hashes {
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
    pub size: u64,
    /// Entry name when the hashes came from inside an archive.
    pub inner_name: Option<String>,
}

/// Files larger than this are indexed but not hashed. Disc images run to
/// gigabytes and hashing them all would make a scan take hours for metadata
/// we can still look up by name.
pub const MAX_HASH_BYTES: u64 = 512 * 1024 * 1024;

fn hash_reader<R: Read>(mut r: R) -> Result<Hashes> {
    let mut crc = crc32fast::Hasher::new();
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut size: u64 = 0;
    let mut buf = vec![0u8; 1024 * 1024];

    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        crc.update(chunk);
        md5.update(chunk);
        sha1.update(chunk);
        size += n as u64;
    }

    Ok(Hashes {
        crc32: format!("{:08x}", crc.finalize()),
        md5: hex::encode(md5.finalize()),
        sha1: hex::encode(sha1.finalize()),
        size,
        inner_name: None,
    })
}

/// Hash a ROM, transparently descending into a zip when there is one clear
/// ROM inside it. Anything past `MAX_HASH_BYTES` is skipped, which is what
/// keeps a scan of a library full of disc images quick.
pub fn hash_rom(path: &Path) -> Result<Option<Hashes>> {
    hash_rom_within(path, MAX_HASH_BYTES)
}

/// Hash a ROM whatever its size. For checking one disc image when asked to,
/// where reading every byte is the point rather than a cost to avoid.
pub fn hash_rom_whole(path: &Path) -> Result<Option<Hashes>> {
    hash_rom_within(path, u64::MAX)
}

fn hash_rom_within(path: &Path, limit: u64) -> Result<Option<Hashes>> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > limit {
        return Ok(None);
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "zip" => return hash_zip_entry(path, limit),
        "7z" => return hash_7z_entry(path, limit),
        _ => {}
    }

    let f = File::open(path)?;
    Ok(Some(hash_reader(BufReader::new(f))?))
}

/// Choose the entry inside an archive most likely to be the ROM: an
/// unambiguous ROM extension first, then the largest.
fn pick_entry(entries: impl Iterator<Item = (String, u64)>) -> Option<(String, u64)> {
    let mut candidates: Vec<(String, u64, bool)> = entries
        .filter_map(|(name, size)| {
            let ext = Path::new(&name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !platforms::is_indexable_ext(&ext) {
                return None;
            }
            let unique = platforms::is_unique_ext(&ext);
            Some((name, size, unique))
        })
        .collect();

    candidates.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)));
    candidates
        .into_iter()
        .next()
        .map(|(name, size, _)| (name, size))
}

/// Pick the most ROM-looking entry in a zip and hash it.
fn hash_zip_entry(path: &Path, limit: u64) -> Result<Option<Hashes>> {
    let f = File::open(path)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(f))?;

    // Survey the entries first, so no borrow of `archive` is held while we
    // decide which one to hash.
    let Some((name, size)) = pick_entry(zip_entries(&mut archive)?.into_iter()) else {
        return Ok(None);
    };
    if size > limit {
        return Ok(None);
    }

    let mut entry = archive.by_name(&name)?;
    let mut hashes = hash_reader(&mut entry)?;
    hashes.inner_name = Some(name);
    Ok(Some(hashes))
}

/// Pick the most ROM-looking entry in a 7z and hash it.
///
/// The archive header lists every entry with its uncompressed size, so the
/// choice costs a header read; only the entry we settle on is decompressed.
fn hash_7z_entry(path: &Path, limit: u64) -> Result<Option<Hashes>> {
    let archive = sevenz_rust2::Archive::open(path)?;

    let Some((name, size)) = pick_entry(sevenz_entries(&archive).into_iter()) else {
        return Ok(None);
    };
    if size > limit {
        return Ok(None);
    }

    let mut hashes = read_7z_entry(path, &name, |rd| hash_reader(rd))?;
    hashes.inner_name = Some(name);
    Ok(Some(hashes))
}

/// Names and uncompressed sizes of the files in a zip.
fn zip_entries<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<Vec<(String, u64)>> {
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        if !entry.is_dir() {
            out.push((entry.name().to_string(), entry.size()));
        }
    }
    Ok(out)
}

/// Names and uncompressed sizes of the files in a 7z.
fn sevenz_entries(archive: &sevenz_rust2::Archive) -> Vec<(String, u64)> {
    archive
        .files
        .iter()
        .filter(|f| !f.is_directory)
        .map(|f| (f.name.clone(), f.size))
        .collect()
}

/// Stream one named entry out of a 7z and hand its reader to `f`.
///
/// 7z archives are often solid, so entries are walked in order rather than
/// seeked to; iteration stops as soon as the wanted entry has been read.
fn read_7z_entry<T>(
    path: &Path,
    name: &str,
    mut f: impl FnMut(&mut dyn Read) -> Result<T>,
) -> Result<T> {
    let mut reader = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())?;

    let mut out: Option<Result<T>> = None;
    reader.for_each_entries(|entry, rd| {
        if entry.name != name {
            return Ok(true);
        }
        out = Some(f(rd));
        Ok(false)
    })?;

    out.unwrap_or_else(|| {
        Err(crate::error::AppError::Other(format!(
            "{name} vanished from the archive while reading it"
        )))
    })
}

/// Read a ROM's actual bytes, descending into an archive when needed.
/// Returns the bytes and the ROM's own name (the entry name, when archived).
pub fn read_rom_bytes(path: &Path) -> Result<(Vec<u8>, String)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let own_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("rom")
        .to_string();

    let no_rom = || {
        crate::error::AppError::Other("No ROM found inside the archive".to_string())
    };

    match ext.as_str() {
        "zip" => {
            let f = File::open(path)?;
            let mut archive = zip::ZipArchive::new(BufReader::new(f))?;
            let (name, size) =
                pick_entry(zip_entries(&mut archive)?.into_iter()).ok_or_else(no_rom)?;

            let mut entry = archive.by_name(&name)?;
            let mut buf = Vec::with_capacity(size as usize);
            entry.read_to_end(&mut buf)?;
            Ok((buf, name))
        }
        "7z" => {
            let archive = sevenz_rust2::Archive::open(path)?;
            let (name, size) =
                pick_entry(sevenz_entries(&archive).into_iter()).ok_or_else(no_rom)?;

            let buf = read_7z_entry(path, &name, |rd| {
                let mut buf = Vec::with_capacity(size as usize);
                rd.read_to_end(&mut buf)?;
                Ok(buf)
            })?;
            Ok((buf, name))
        }
        _ => Ok((std::fs::read(path)?, own_name)),
    }
}

/// Unpack the ROM inside an archive into `dest_dir`, returning the file.
///
/// Streamed rather than read into memory: a Wii disc image is several
/// gigabytes and only the emulator needs to see all of it at once. An
/// existing file of the right size is reused, so this costs nothing on the
/// second launch.
pub fn extract_rom(path: &Path, dest_dir: &Path) -> Result<PathBuf> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let no_rom = || crate::error::AppError::Other("No ROM found inside the archive".to_string());

    // Pick the entry first, so we know where it is going before unpacking.
    let (name, size) = match ext.as_str() {
        "zip" => {
            let f = File::open(path)?;
            let mut archive = zip::ZipArchive::new(BufReader::new(f))?;
            pick_entry(zip_entries(&mut archive)?.into_iter()).ok_or_else(no_rom)?
        }
        "7z" => {
            let archive = sevenz_rust2::Archive::open(path)?;
            pick_entry(sevenz_entries(&archive).into_iter()).ok_or_else(no_rom)?
        }
        _ => return Ok(path.to_path_buf()),
    };

    // Entry names carry their own folders; only the file itself is wanted,
    // and a name that tries to climb out of the directory is refused.
    let leaf = Path::new(&name)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(no_rom)?;
    let dest = dest_dir.join(leaf);

    if dest.metadata().map(|m| m.len()) .ok() == Some(size) {
        return Ok(dest);
    }

    std::fs::create_dir_all(dest_dir)?;
    // Unpack beside the target and rename, so an interrupted extraction never
    // leaves a half a ROM looking like a finished one.
    let partial = dest.with_extension("partial");

    match ext.as_str() {
        "zip" => {
            let f = File::open(path)?;
            let mut archive = zip::ZipArchive::new(BufReader::new(f))?;
            let mut entry = archive.by_name(&name)?;
            let mut out = File::create(&partial)?;
            std::io::copy(&mut entry, &mut out)?;
        }
        _ => {
            read_7z_entry(path, &name, |rd| {
                let mut out = File::create(&partial)?;
                std::io::copy(rd, &mut out)?;
                Ok(())
            })?;
        }
    }

    std::fs::rename(&partial, &dest)?;
    Ok(dest)
}

/// The names inside an archive, used for platform detection.
pub fn archive_entry_names(path: &Path) -> Result<Vec<String>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "7z" {
        let archive = sevenz_rust2::Archive::open(path)?;
        return Ok(sevenz_entries(&archive).into_iter().map(|(n, _)| n).collect());
    }

    let f = File::open(path)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(f))?;
    Ok(zip_entries(&mut archive)?.into_iter().map(|(n, _)| n).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "playdex-hash-{}-{}-{:?}",
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

    /// The ROM we hide inside the archives, plus its hashes computed directly.
    fn rom() -> (Vec<u8>, Hashes) {
        let bytes: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
        let hashes = hash_reader(bytes.as_slice()).unwrap();
        (bytes, hashes)
    }

    #[test]
    fn hashes_the_rom_inside_a_7z_not_the_container() {
        let dir = scratch("7z");
        let (bytes, direct) = rom();

        let archive = dir.join("New Super Mario Bros Wii [SMNE01].7z");
        {
            let mut w = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
            // A readme alongside the ROM, to prove the picker ignores it.
            w.push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("readme.txt"),
                Some(b"not a game".as_slice()),
            )
            .unwrap();
            w.push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("New Super Mario Bros Wii.wbfs"),
                Some(bytes.as_slice()),
            )
            .unwrap();
            w.finish().unwrap();
        }

        let got = hash_rom(&archive).unwrap().expect("hashed");
        assert_eq!(got.crc32, direct.crc32);
        assert_eq!(got.md5, direct.md5);
        assert_eq!(got.sha1, direct.sha1);
        assert_eq!(got.size, bytes.len() as u64);
        assert_eq!(got.inner_name.as_deref(), Some("New Super Mario Bros Wii.wbfs"));

        // The container's own hash must not be what we recorded.
        let container = hash_reader(std::fs::File::open(&archive).unwrap()).unwrap();
        assert_ne!(got.crc32, container.crc32);

        // ...and the bytes come back out intact.
        let (read, name) = read_rom_bytes(&archive).unwrap();
        assert_eq!(read, bytes);
        assert_eq!(name, "New Super Mario Bros Wii.wbfs");

        let names = archive_entry_names(&archive).unwrap();
        assert!(names.iter().any(|n| n.ends_with(".wbfs")));
        assert!(names.iter().any(|n| n == "readme.txt"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn zip_and_7z_of_the_same_rom_agree() {
        let dir = scratch("both");
        let (bytes, direct) = rom();

        let zipped = dir.join("game.zip");
        {
            let f = std::fs::File::create(&zipped).unwrap();
            let mut w = zip::ZipWriter::new(f);
            w.start_file("Super Metroid.sfc", zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut w, &bytes).unwrap();
            w.finish().unwrap();
        }

        let sevened = dir.join("game.7z");
        {
            let mut w = sevenz_rust2::ArchiveWriter::create(&sevened).unwrap();
            w.push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("Super Metroid.sfc"),
                Some(bytes.as_slice()),
            )
            .unwrap();
            w.finish().unwrap();
        }

        let a = hash_rom(&zipped).unwrap().expect("zip hashed");
        let b = hash_rom(&sevened).unwrap().expect("7z hashed");
        assert_eq!(a.sha1, direct.sha1);
        assert_eq!(a.sha1, b.sha1);
        assert_eq!(a.inner_name, b.inner_name);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_archive_holding_no_rom_is_not_hashed() {
        let dir = scratch("empty");
        let archive = dir.join("scans.7z");
        {
            let mut w = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
            w.push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("manual.txt"),
                Some(b"box scan".as_slice()),
            )
            .unwrap();
            w.finish().unwrap();
        }

        assert!(hash_rom(&archive).unwrap().is_none());
        assert!(read_rom_bytes(&archive).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
