//! Identifying a system from a file's own bytes.
//!
//! Extensions and filenames are what somebody typed. A magic number is what
//! the machine actually wrote, so where the two disagree the bytes win.
//!
//! This exists for the ambiguous cases. `.sfc` or `.nsp` already belong to
//! exactly one system and settle themselves; `.iso`, `.bin` and `.chd` belong
//! to a dozen and, until now, fell through to guessing from folder and file
//! names. A disc image says what it is in its first few hundred bytes.
//!
//! Only the first kilobyte is read, which is where every signature here
//! lives, so this costs one short read rather than a scan of the file.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub struct Signature {
    pub offset: usize,
    pub magic: &'static [u8],
    pub platform: &'static str,
}

/// Magic numbers, longest and most specific first.
///
/// The Nintendo entries were read off real files rather than taken on trust:
/// a Wii dump carries `WBFS` at 0 and wraps the disc header at 0x200, where
/// `5D1C9EA3` sits at +0x18 next to the disc ID; a Switch NSP opens with
/// `PFS0`. The Sega and GameCube entries come from format documentation.
///
/// A wrong entry here is bounded: signatures are only consulted when the
/// extension did not already settle the system, so the worst a bad one can do
/// is mis-resolve a file that was going to be guessed at anyway.
pub const SIGNATURES: &[Signature] = &[
    // --- Nintendo
    Signature { offset: 0, magic: b"NES\x1a", platform: "nes" },
    Signature { offset: 0, magic: b"PFS0", platform: "switch" },
    Signature { offset: 0x100, magic: b"HEAD", platform: "switch" },
    Signature { offset: 0, magic: b"WBFS", platform: "wii" },
    // The Wii's disc magic, raw and as wrapped by a WBFS container.
    Signature { offset: 0x18, magic: &[0x5D, 0x1C, 0x9E, 0xA3], platform: "wii" },
    Signature { offset: 0x218, magic: &[0x5D, 0x1C, 0x9E, 0xA3], platform: "wii" },
    Signature { offset: 0x1C, magic: &[0xC2, 0x33, 0x9F, 0x3D], platform: "gamecube" },
    Signature { offset: 0x21C, magic: &[0xC2, 0x33, 0x9F, 0x3D], platform: "gamecube" },
    // --- Sega
    // Three offsets each: 0 for a plain image, 0x10 where a raw Mode 1 track
    // puts the start of its user data, and 0x18 for Mode 2 Form 1, which has
    // a further 8-byte subheader in front.
    Signature { offset: 0, magic: b"SEGADISCSYSTEM", platform: "segacd" },
    Signature { offset: 0x10, magic: b"SEGADISCSYSTEM", platform: "segacd" },
    Signature { offset: 0x18, magic: b"SEGADISCSYSTEM", platform: "segacd" },
    Signature { offset: 0, magic: b"SEGA SEGASATURN", platform: "saturn" },
    Signature { offset: 0x10, magic: b"SEGA SEGASATURN", platform: "saturn" },
    Signature { offset: 0x18, magic: b"SEGA SEGASATURN", platform: "saturn" },
    Signature { offset: 0, magic: b"SEGA SEGAKATANA", platform: "dreamcast" },
    Signature { offset: 0x10, magic: b"SEGA SEGAKATANA", platform: "dreamcast" },
    Signature { offset: 0x18, magic: b"SEGA SEGAKATANA", platform: "dreamcast" },
    Signature { offset: 0x100, magic: b"SEGA MEGA DRIVE", platform: "genesis" },
    Signature { offset: 0x100, magic: b"SEGA GENESIS", platform: "genesis" },
    Signature { offset: 0x100, magic: b"SEGA 32X", platform: "sega32x" },
    // --- Sony
    Signature { offset: 0, magic: b"PS-X EXE", platform: "ps1" },
];

/// How far in the furthest signature reaches.
const HEAD_BYTES: usize = 0x300;

/// The system a file's own bytes claim, if any of them do.
/// The system the start of a disc image says it is, given only its first
/// few megabytes - read out of an archive, where reaching the rest would mean
/// decompressing it.
///
/// Everything checked here sits near the front: header magic, an ISO 9660
/// filesystem and the small file in its root that names the console, and an
/// Xbox filesystem when the image starts with the game. A full Xbox 360 dump,
/// whose game starts a quarter of a gigabyte in, is out of reach and says
/// nothing.
pub fn identify_head(head: &[u8]) -> Option<&'static str> {
    if let Some(found) = identify_bytes(&head[..head.len().min(HEAD_BYTES)]) {
        return Some(found);
    }
    let mut disc = std::io::Cursor::new(head);
    iso9660_in(&mut disc).or_else(|| xdvdfs_in(&mut disc, head.len() as u64))
}

pub fn identify(path: &Path) -> Option<&'static str> {
    let mut head = vec![0u8; HEAD_BYTES];
    let read = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .ok()?;
    head.truncate(read);
    if let Some(found) = identify_bytes(&head) {
        return Some(found);
    }
    identify_iso9660(path).or_else(|| identify_xdvdfs(path))
}

// ------------------------------------------------------------- ISO 9660

/// The user data in one sector of an ISO 9660 filesystem, whatever the image
/// wraps it in.
const DATA: usize = 2048;

/// Where an ISO 9660 image keeps its volume descriptor.
const PVD_SECTOR: u64 = 16;

/// How an image lays its sectors out on disk.
///
/// A CD sector holds 2048 bytes of the filesystem and, on the disc itself,
/// several hundred more of sync marks, addressing and error correction. An
/// `.iso` is the filesystem with all of that stripped away, so its sectors are
/// 2048 bytes and start where the data starts. A raw `.bin` track keeps the
/// lot, so its sectors are 2352 bytes and the data begins part way in - after
/// 16 bytes in Mode 1, or 24 in Mode 2 Form 1, which carries a subheader as
/// well. A dump taken with subchannel data appended runs to 2448.
struct Layout {
    sector: u64,
    data: u64,
}

/// Tried in order. `.iso` first because it is the common case, and the
/// candidates are distinguished by whether `CD001` turns up where they say it
/// should, so a wrong guess simply fails to match.
const LAYOUTS: &[Layout] = &[
    Layout { sector: 2048, data: 0 },
    Layout { sector: 2352, data: 16 },
    Layout { sector: 2352, data: 24 },
    Layout { sector: 2448, data: 16 },
    Layout { sector: 2448, data: 24 },
];

/// Read one sector's worth of filesystem out of an image.
/// Anything a disc can be read from: a file, or the start of one held in
/// memory after reading it out of an archive.
trait Disc: Read + Seek {}
impl<T: Read + Seek> Disc for T {}

fn read_sector(file: &mut dyn Disc, layout: &Layout, lba: u64) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; DATA];
    file.seek(SeekFrom::Start(lba * layout.sector + layout.data))
        .ok()?;
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Read `len` bytes starting at a sector, across as many sectors as it takes.
///
/// An `.iso` could do this in one read, since its sectors are the data and
/// nothing else. A raw track cannot: every 2048 bytes of filesystem has a
/// few hundred bytes of sync and error correction bolted around it, so the
/// pieces have to be gathered one sector at a time.
fn read_extent(file: &mut dyn Disc, layout: &Layout, lba: u64, len: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    let sectors = len.div_ceil(DATA) as u64;
    for i in 0..sectors {
        out.extend_from_slice(&read_sector(file, layout, lba + i)?);
    }
    out.truncate(len);
    Some(out)
}

/// The layout an image is in, judged by where its primary volume descriptor
/// turns up: descriptor type 1 followed by the standard identifier.
fn layout_of(file: &mut dyn Disc) -> Option<&'static Layout> {
    LAYOUTS.iter().find(|layout| {
        read_sector(file, layout, PVD_SECTOR)
            .map(|pvd| pvd[0] == 1 && &pvd[1..6] == b"CD001")
            .unwrap_or(false)
    })
}

/// Identify a disc image by reading its filesystem.
///
/// Sony discs carry no magic number in their first bytes, which is why a
/// PlayStation 2 game came out as Unidentified: nothing in the header says
/// what it is. What does say is a file in the root of the disc. `SYSTEM.CNF`
/// names the executable to boot, under the key `BOOT2` on a PlayStation 2 and
/// `BOOT` on a PlayStation. A PSP disc instead carries `PSP_GAME` and
/// `UMD_DATA.BIN`.
///
/// So the disc's own directory is read: the volume descriptor at sector 16,
/// the root directory it points at, and then the one small file that answers
/// the question. A handful of short reads, wherever the image is on disk.
///
/// This works on a raw `.bin` track as well as an `.iso`. The filesystem is
/// the same either way; only the packaging around each sector differs, and
/// `layout_of` works out which is in front of it before reading anything.
pub fn identify_iso9660(path: &Path) -> Option<&'static str> {
    iso9660_in(&mut std::fs::File::open(path).ok()?)
}

fn iso9660_in(file: &mut dyn Disc) -> Option<&'static str> {
    let mut file = file;
    let layout = layout_of(&mut file)?;
    let pvd = read_sector(&mut file, layout, PVD_SECTOR)?;

    // The root directory's own record is embedded at offset 156.
    let (root_lba, root_len) = extent(&pvd[156..156 + 34])?;
    if root_len == 0 || root_len > 4 * 1024 * 1024 {
        return None;
    }

    let dir = read_extent(&mut file, layout, root_lba, root_len)?;
    let entries = read_directory(&dir);

    let named = |want: &str| entries.iter().any(|(name, _, _)| name == want);

    if named("PSP_GAME") || named("UMD_DATA.BIN") {
        return Some("psp");
    }
    // The start of a PlayStation 3 disc is never encrypted, and it holds the
    // filesystem: `PS3_DISC.SFB` and the `PS3_GAME` folder sit in the root.
    if named("PS3_DISC.SFB") || named("PS3_GAME") {
        return Some("ps3");
    }

    if let Some((_, lba, len)) = entries.iter().find(|(n, _, _)| n == "SYSTEM.CNF") {
        if let Some(buf) = read_extent(&mut file, layout, *lba, (*len).min(4096)) {
            let text = String::from_utf8_lossy(&buf).to_ascii_uppercase();
            // BOOT2 is the PlayStation 2 key; plain BOOT is the original.
            if text.contains("BOOT2") {
                return Some("ps2");
            }
            if text.contains("BOOT") {
                return Some("ps1");
            }
        }
    }

    None
}

/// The sector and byte length a directory record points at.
fn extent(record: &[u8]) -> Option<(u64, usize)> {
    if record.len() < 14 {
        return None;
    }
    let lba = u32::from_le_bytes(record[2..6].try_into().ok()?) as u64;
    let len = u32::from_le_bytes(record[10..14].try_into().ok()?) as usize;
    Some((lba, len))
}

/// Walk an ISO 9660 directory into (name, sector, length) triples.
fn read_directory(dir: &[u8]) -> Vec<(String, u64, usize)> {
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < dir.len() {
        let len = dir[i] as usize;
        if len == 0 {
            // Records never straddle a sector, so a zero length means the rest
            // of this sector is padding.
            let next = (i / DATA + 1) * DATA;
            if next <= i || next >= dir.len() {
                break;
            }
            i = next;
            continue;
        }
        if i + len > dir.len() || len < 34 {
            break;
        }
        let record = &dir[i..i + len];
        let name_len = record[32] as usize;
        if record.len() >= 33 + name_len {
            let raw = &record[33..33 + name_len];
            // Names carry a ";1" version suffix that nothing here wants.
            let name = String::from_utf8_lossy(raw)
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_uppercase();
            if let Some((lba, size)) = extent(record) {
                out.push((name, lba, size));
            }
        }
        i += len;
    }
    out
}

// --------------------------------------------------------------- XDVDFS

/// The identifier at the start of an Xbox disc's volume descriptor.
const XDVDFS_MAGIC: &[u8] = b"MICROSOFT*XBOX*MEDIA";

/// Where the game's filesystem starts in an image, and which console a
/// disc laid out that way belongs to when its root says nothing clearer.
///
/// A full dump keeps the disc's video partition in front of the game, at a
/// size that differs by disc generation. An image cut down to the game
/// alone (`extract-xiso`, or what most tools write) starts at 0.
const XDVDFS_PARTITIONS: &[(u64, Option<&str>)] = &[
    (0, None),
    // XGD1, the original Xbox.
    (0x1830_0000, Some("xbox")),
    // XGD2 and XGD3, the Xbox 360.
    (0x0FD9_0000, Some("xbox360")),
    (0x0208_0000, Some("xbox360")),
];

/// Identify an Xbox or Xbox 360 disc image.
///
/// Neither uses ISO 9660 for the game. They share their own filesystem,
/// XDVDFS, whose volume descriptor sits 32 sectors into the game partition,
/// and the two consoles are told apart by what the root boots: an original
/// Xbox runs `default.xbe`, an Xbox 360 `default.xex`.
pub fn identify_xdvdfs(path: &Path) -> Option<&'static str> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    xdvdfs_in(&mut file, len)
}

fn xdvdfs_in(file: &mut dyn Disc, len: u64) -> Option<&'static str> {

    for (base, generation) in XDVDFS_PARTITIONS {
        let at = base + 32 * DATA as u64;
        if at + DATA as u64 > len {
            continue;
        }
        let mut vd = vec![0u8; DATA];
        if file.seek(SeekFrom::Start(at)).is_err() || file.read_exact(&mut vd).is_err() {
            continue;
        }
        if &vd[..XDVDFS_MAGIC.len()] != XDVDFS_MAGIC {
            continue;
        }

        let root_sector = u32::from_le_bytes(vd[0x14..0x18].try_into().ok()?) as u64;
        let root_len = u32::from_le_bytes(vd[0x18..0x1C].try_into().ok()?) as usize;
        if root_len > 0 && root_len <= 4 * 1024 * 1024 {
            let mut dir = vec![0u8; root_len];
            let start = base + root_sector * DATA as u64;
            if file.seek(SeekFrom::Start(start)).is_ok() && file.read_exact(&mut dir).is_ok() {
                let names = xdvdfs_names(&dir);
                if names.iter().any(|n| n == "default.xex") {
                    return Some("xbox360");
                }
                if names.iter().any(|n| n == "default.xbe") {
                    return Some("xbox");
                }
            }
        }
        // The layout alone still says which generation of disc this is.
        if generation.is_some() {
            return *generation;
        }
    }
    None
}

/// The names in one XDVDFS directory, lowercased.
///
/// A directory is a binary tree rather than a list: each entry carries the
/// offsets of its left and right children, in four-byte units, and the
/// space between entries is padding. Walking the tree reads exactly the
/// entries there are and never mistakes padding for one.
fn xdvdfs_names(dir: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![0usize];
    let mut seen = std::collections::HashSet::new();

    while let Some(at) = stack.pop() {
        if !seen.insert(at) || at + 14 > dir.len() {
            continue;
        }
        let left = u16::from_le_bytes([dir[at], dir[at + 1]]);
        let right = u16::from_le_bytes([dir[at + 2], dir[at + 3]]);
        // An all-ones entry is padding, not a file.
        if left == 0xFFFF && right == 0xFFFF {
            continue;
        }
        let name_len = dir[at + 13] as usize;
        if let Some(raw) = dir.get(at + 14..at + 14 + name_len) {
            out.push(String::from_utf8_lossy(raw).to_ascii_lowercase());
        }
        if left != 0 {
            stack.push(left as usize * 4);
        }
        if right != 0 {
            stack.push(right as usize * 4);
        }
    }
    out
}

pub fn identify_bytes(head: &[u8]) -> Option<&'static str> {
    SIGNATURES
        .iter()
        .find(|s| {
            head.len() >= s.offset + s.magic.len()
                && &head[s.offset..s.offset + s.magic.len()] == s.magic
        })
        .map(|s| s.platform)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(offset: usize, magic: &[u8]) -> Vec<u8> {
        let mut v = vec![0u8; HEAD_BYTES];
        v[offset..offset + magic.len()].copy_from_slice(magic);
        v
    }

    /// The three read off real dumps on this machine.
    #[test]
    fn reads_the_signatures_taken_from_real_files() {
        // A Switch NSP opens with PFS0.
        assert_eq!(identify_bytes(&at(0, b"PFS0")), Some("switch"));

        // A Wii dump in a WBFS container: the container magic at 0, and the
        // disc header it wraps at 0x200, so the disc magic lands at 0x218.
        assert_eq!(identify_bytes(&at(0, b"WBFS")), Some("wii"));
        assert_eq!(
            identify_bytes(&at(0x218, &[0x5D, 0x1C, 0x9E, 0xA3])),
            Some("wii")
        );

        // And a raw disc image, where it sits at 0x18.
        assert_eq!(
            identify_bytes(&at(0x18, &[0x5D, 0x1C, 0x9E, 0xA3])),
            Some("wii")
        );
    }

    #[test]
    fn tells_gamecube_from_wii() {
        assert_eq!(
            identify_bytes(&at(0x1C, &[0xC2, 0x33, 0x9F, 0x3D])),
            Some("gamecube")
        );
        assert_eq!(
            identify_bytes(&at(0x18, &[0x5D, 0x1C, 0x9E, 0xA3])),
            Some("wii")
        );
    }

    #[test]
    fn reads_the_sega_disc_headers() {
        assert_eq!(identify_bytes(&at(0, b"SEGADISCSYSTEM")), Some("segacd"));
        assert_eq!(identify_bytes(&at(0, b"SEGA SEGASATURN")), Some("saturn"));
        assert_eq!(
            identify_bytes(&at(0x10, b"SEGA SEGAKATANA")),
            Some("dreamcast")
        );
    }

    /// Build a small but genuine ISO 9660 image: a volume descriptor at
    /// sector 16, a root directory holding one file, and that file's contents.
    fn build_iso(file_name: &str, contents: &[u8]) -> Vec<u8> {
        const SEC: usize = 2048;
        let root_sector = 17usize;
        let file_sector = 18usize;
        let mut iso = vec![0u8; SEC * 20];

        // --- one directory record for the file
        let name = file_name.as_bytes();
        let rec_len = 33 + name.len() + (1 - name.len() % 2);
        let mut rec = vec![0u8; rec_len];
        rec[0] = rec_len as u8;
        rec[2..6].copy_from_slice(&(file_sector as u32).to_le_bytes());
        rec[10..14].copy_from_slice(&(contents.len() as u32).to_le_bytes());
        rec[32] = name.len() as u8;
        rec[33..33 + name.len()].copy_from_slice(name);

        let root_off = root_sector * SEC;
        iso[root_off..root_off + rec.len()].copy_from_slice(&rec);

        // --- the primary volume descriptor
        let pvd = 16 * SEC;
        iso[pvd] = 1;
        iso[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
        // The root directory's own record lives at offset 156.
        let r = pvd + 156;
        iso[r] = 34;
        iso[r + 2..r + 6].copy_from_slice(&(root_sector as u32).to_le_bytes());
        iso[r + 10..r + 14].copy_from_slice(&(SEC as u32).to_le_bytes());

        // --- the file itself
        let f = file_sector * SEC;
        iso[f..f + contents.len()].copy_from_slice(contents);
        iso
    }

    fn write_temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-iso-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// A PlayStation 2 disc names its boot executable under BOOT2. Nothing in
    /// the header says "PlayStation", which is why these came out as
    /// Unidentified until the filesystem was read.
    #[test]
    fn reads_a_playstation_2_disc_out_of_its_filesystem() {
        let iso = build_iso("SYSTEM.CNF;1", b"BOOT2 = cdrom0:SLUS_203.12;1 VER=1.00");
        let path = write_temp("Sonic Riders.iso", &iso);
        assert_eq!(identify(&path), Some("ps2"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The original PlayStation uses BOOT, without the 2.
    #[test]
    fn tells_a_playstation_1_disc_from_a_2() {
        let iso = build_iso("SYSTEM.CNF;1", b"BOOT = cdrom:SLUS_004.02;1 TCB = 4");
        let path = write_temp("Some PS1 Game.iso", &iso);
        assert_eq!(identify(&path), Some("ps1"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A PSP disc is known by what sits in its root instead.
    #[test]
    fn reads_a_psp_disc() {
        let iso = build_iso("UMD_DATA.BIN;1", b"ULUS10041|0001|G");
        let path = write_temp("Some PSP Game.iso", &iso);
        assert_eq!(identify(&path), Some("psp"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Wrap a plain image's sectors the way a raw CD track carries them:
    /// a 12-byte sync pattern, a 4-byte address, an optional 8-byte subheader
    /// for Mode 2, then the 2048 bytes of filesystem, then room for the error
    /// correction that a real dump would fill in.
    fn to_raw_track(iso: &[u8], data_offset: usize) -> Vec<u8> {
        const RAW: usize = 2352;
        let mut out = Vec::new();
        for chunk in iso.chunks(2048) {
            let mut sector = vec![0u8; RAW];
            sector[0] = 0x00;
            for b in sector.iter_mut().take(11).skip(1) {
                *b = 0xFF;
            }
            sector[11] = 0x00;
            sector[data_offset..data_offset + chunk.len()].copy_from_slice(chunk);
            out.extend_from_slice(&sector);
        }
        out
    }

    /// The one this was written for: a bare `.bin` with no `.cue` beside it.
    /// The filesystem is identical to an `.iso`; it is only wrapped in the
    /// sync marks and error correction a real disc carries, so reading it
    /// means stepping over that packaging rather than anything new.
    #[test]
    fn reads_a_playstation_2_disc_out_of_a_raw_bin_track() {
        let iso = build_iso("SYSTEM.CNF;1", b"BOOT2 = cdrom0:SLUS_203.12;1 VER=1.00");
        let path = write_temp("Sonic Riders (Track 1).bin", &to_raw_track(&iso, 16));
        assert_eq!(identify(&path), Some("ps2"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Mode 2 Form 1 puts a further 8-byte subheader in front of the data,
    /// which is how most PlayStation discs were actually pressed.
    #[test]
    fn reads_a_raw_track_in_mode_2() {
        let iso = build_iso("SYSTEM.CNF;1", b"BOOT = cdrom:SLUS_004.02;1 TCB = 4");
        let path = write_temp("Final Fantasy VII (Disc 1).bin", &to_raw_track(&iso, 24));
        assert_eq!(identify(&path), Some("ps1"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A directory or a file longer than one sector has to be gathered a
    /// sector at a time out of a raw track, because the packaging sits
    /// between the pieces. Reading it as one run would splice sync marks
    /// into the middle of the data.
    #[test]
    fn reads_across_sector_boundaries_in_a_raw_track() {
        let mut cnf = vec![b' '; 3000];
        cnf[..5].copy_from_slice(b"BOOT2");
        let iso = build_iso("SYSTEM.CNF;1", &cnf);
        let path = write_temp("Big.bin", &to_raw_track(&iso, 16));
        assert_eq!(identify(&path), Some("ps2"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A PlayStation 3 disc is known by what sits in its root. Its
    /// filesystem is in the part of the disc that is never encrypted, so
    /// this works on a straight dump as well as a decrypted one.
    #[test]
    fn reads_a_playstation_3_disc() {
        let iso = build_iso("PS3_DISC.SFB;1", b".SFB\0\0\0\x01");
        let path = write_temp("Demon's Souls (USA).iso", &iso);
        assert_eq!(identify(&path), Some("ps3"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// One XDVDFS directory entry, padded to four bytes.
    fn xdvdfs_entry(left: u16, right: u16, name: &str) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(&left.to_le_bytes());
        e.extend_from_slice(&right.to_le_bytes());
        e.extend_from_slice(&40u32.to_le_bytes());
        e.extend_from_slice(&1024u32.to_le_bytes());
        e.push(0x20);
        e.push(name.len() as u8);
        e.extend_from_slice(name.as_bytes());
        while e.len() % 4 != 0 {
            e.push(0xFF);
        }
        e
    }

    /// An Xbox disc image with its game partition at `base`, holding a root
    /// directory of two files.
    fn build_xdvdfs(base: usize, boot: &str) -> Vec<u8> {
        const SEC: usize = 2048;
        let root_sector = 34usize;
        let mut img = vec![0u8; base + SEC * 40];

        // `media` at the top of the tree and the boot file as its right
        // child, so finding it means following the tree rather than reading
        // the first entry.
        let right_at = xdvdfs_entry(0, 0, "media").len();
        let mut dir = xdvdfs_entry(0, (right_at / 4) as u16, "media");
        dir.extend_from_slice(&xdvdfs_entry(0, 0, boot));
        dir.resize(SEC, 0xFF);

        let vd = base + 32 * SEC;
        img[vd..vd + 20].copy_from_slice(b"MICROSOFT*XBOX*MEDIA");
        img[vd + 0x14..vd + 0x18].copy_from_slice(&(root_sector as u32).to_le_bytes());
        img[vd + 0x18..vd + 0x1C].copy_from_slice(&(SEC as u32).to_le_bytes());
        let r = base + root_sector * SEC;
        img[r..r + SEC].copy_from_slice(&dir);
        img
    }

    #[test]
    fn tells_an_xbox_disc_from_an_xbox_360_one() {
        let x360 = write_temp("Halo 3.iso", &build_xdvdfs(0, "default.xex"));
        assert_eq!(identify(&x360), Some("xbox360"));

        let xbox = write_temp("Halo.iso", &build_xdvdfs(0, "default.xbe"));
        assert_eq!(identify(&xbox), Some("xbox"));

        // A full XGD3 dump keeps the game partition further in.
        let full = write_temp("Forza.iso", &build_xdvdfs(0x0208_0000, "default.xex"));
        assert_eq!(identify(&full), Some("xbox360"));

        for p in [x360, xbox, full] {
            let _ = std::fs::remove_dir_all(p.parent().unwrap());
        }
    }

    /// A raw track full of nothing is still not a disc.
    #[test]
    fn a_bin_that_is_not_a_disc_says_nothing() {
        let path = write_temp("audio.bin", &vec![0u8; 2352 * 20]);
        assert_eq!(identify(&path), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Something that is not a disc image at all stays unidentified.
    #[test]
    fn an_iso_that_is_not_iso9660_says_nothing() {
        let path = write_temp("junk.iso", &vec![0u8; 2048 * 20]);
        assert_eq!(identify(&path), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn says_nothing_when_it_recognises_nothing() {
        assert_eq!(identify_bytes(&vec![0u8; HEAD_BYTES]), None);
        assert_eq!(identify_bytes(b"not a rom"), None);
        // Too short to reach an offset is not a match.
        assert_eq!(identify_bytes(b"WBF"), None);
    }
}
