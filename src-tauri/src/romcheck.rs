//! Telling ROMs apart from everything else that ends up in a ROM folder.
//!
//! Extensions lie. A `.bin` is as likely to be a BIOS, a CD audio track or a
//! firmware blob as a game, and folders collect manuals, box scans, save
//! files and the odd installer. This reads the first bytes of a file and
//! decides.
//!
//! The bias is deliberately toward keeping things: a file is only rejected on
//! definite evidence. Anything merely odd is indexed and reported as suspect,
//! because losing a real game is far worse than listing a stray file.

use std::io::Read;
use std::path::Path;

use crate::hashing;
use crate::platforms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Index it.
    Rom,
    /// Index it, but say so — the contents do not match the extension.
    Suspect(String),
    /// Definitely not a game; leave it out.
    NotRom(String),
}

/// Nothing smaller than this has ever been a game.
const MIN_ROM_BYTES: u64 = 1024;

/// Playlists: a few lines of text naming the files that make up a disc.
///
/// These are exempt from the size floor, and from being judged on their
/// contents at all. A cue sheet is a couple of hundred bytes and is not the
/// game - it is the thing that says where the game is, and it is what an
/// emulator wants handed to it. Rejecting one for being small threw away the
/// only usable file in the folder and left its tracks behind.
pub const PLAYLIST_EXTS: &[&str] = &["cue", "gdi", "m3u", "ccd", "toc"];

pub fn is_playlist(ext: &str) -> bool {
    PLAYLIST_EXTS.contains(&ext)
}

/// Magic numbers for things that are definitely not ROMs. Zip and 7z are
/// absent on purpose — those are handled as archives elsewhere.
const NOT_ROM_MAGIC: &[(&[u8], &str)] = &[
    (b"%PDF", "a PDF"),
    (b"\x89PNG\r\n\x1a\n", "a PNG image"),
    (b"\xff\xd8\xff", "a JPEG image"),
    (b"GIF8", "a GIF image"),
    (b"RIFF", "an audio or video file"),
    (b"OggS", "an Ogg file"),
    (b"ID3", "an MP3"),
    (b"\x1f\x8b", "a gzip archive"),
    (b"Rar!\x1a\x07", "a RAR archive"),
    (b"\x7fELF", "a Linux executable"),
    (b"MZ", "a Windows executable"),
    (b"\xd0\xcf\x11\xe0", "an Office document"),
    (b"SQLite format 3", "a database"),
    (b"\x1a\x45\xdf\xa3", "a Matroska video"),
    (b"<!DO", "an HTML file"),
    (b"<htm", "an HTML file"),
    (b"<?xm", "an XML file"),
    (b"{\\rtf", "an RTF document"),
];

/// Filenames that mean firmware rather than a game. Every emulated system
/// wants one of these somewhere, and they live alongside the ROMs.
const BIOS_HINTS: &[&str] = &[
    "bios", "scph", "disksys", "syscard", "gba_bios", "gb_bios", "dmg_boot",
    "cgb_boot", "sgb_boot", "boot rom", "bootrom", "firmware", "kickstart",
    "neogeo.zip", "lynxboot", "panafz", "goldstar", "saturn_bios", "sega_101",
    "mpr-17933", "3do_bios", "pcfx", "x68000", "spc7110",
    // An emulator's own support files, for loose copies that turn up outside
    // the install they came from. The install itself is skipped wholesale.
    "dsp_rom", "dsp_coef", "codehandler", "font_western", "font_japanese",
    "font_sjis", "font_ansi", "nwc24", "sd.raw", "totaldb",
];

/// Extensions that make a file a program. Nothing that ships one of these is
/// a game dump: it is an emulator, an installer or a tool.
const PROGRAM_EXTS: &[&str] = &["exe", "dll", "so", "dylib", "msi", "sys", "efi"];

fn is_program(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| PROGRAM_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// True if this directory is a program's own folder rather than a place ROMs
/// are kept.
///
/// Emulators get unpacked into ROM libraries all the time, and their `Sys`
/// and data folders are full of firmware blobs, fonts and cheat databases
/// that look enough like ROMs to be indexed — Dolphin alone contributes a
/// dozen `.bin` files. Spotting the install at its root throws out the whole
/// tree in one go, which is both cheaper and more reliable than trying to
/// name every file inside it.
pub fn holds_a_program(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        e.file_type().map(|t| t.is_file()).unwrap_or(false)
            && e.file_name().to_str().map(is_program).unwrap_or(false)
    })
}

/// Header signatures we can positively confirm, per platform.
fn expected_magic(platform: &str) -> Option<(usize, &'static [&'static [u8]], &'static str)> {
    match platform {
        // iNES / NES 2.0 header.
        "nes" => Some((0, &[b"NES\x1a"], "NES")),
        // The Nintendo logo every cartridge must carry to boot.
        "gb" | "gbc" => Some((0x104, &[b"\xce\xed\x66\x66"], "Game Boy")),
        "gba" => Some((0x04, &[b"\x24\xff\xae\x51"], "Game Boy Advance")),
        // Three byte orders are all in circulation.
        "n64" => Some((
            0,
            &[b"\x80\x37\x12\x40", b"\x37\x80\x40\x12", b"\x40\x12\x37\x80"],
            "Nintendo 64",
        )),
        "genesis" | "sega32x" => Some((0x100, &[b"SEGA"], "Mega Drive")),
        _ => None,
    }
}

fn starts_with_at(buf: &[u8], offset: usize, needle: &[u8]) -> bool {
    buf.len() >= offset + needle.len() && &buf[offset..offset + needle.len()] == needle
}

fn looks_like_bios(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    BIOS_HINTS.iter().any(|hint| lower.contains(hint))
}

/// Decide what a file is. `platform` is what detection concluded from the
/// path, used only to confirm a header when we know what one should look like.
pub fn inspect(path: &Path, platform: &str) -> Verdict {
    let Ok(meta) = std::fs::metadata(path) else {
        return Verdict::NotRom("unreadable".into());
    };

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // A playlist is meant to be tiny, and is the file that matters.
    if is_playlist(&ext) {
        return if meta.len() == 0 {
            Verdict::NotRom("an empty playlist".into())
        } else {
            Verdict::Rom
        };
    }

    if meta.len() < MIN_ROM_BYTES {
        return Verdict::NotRom(format!("only {} bytes", meta.len()));
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if looks_like_bios(file_name) {
        return Verdict::NotRom("a BIOS or firmware file".into());
    }

    // 64 KiB covers every header we check, including ISO 9660 at 0x8001.
    let mut head = vec![0u8; 64 * 1024];
    let read = match std::fs::File::open(path).and_then(|mut f| f.read(&mut head)) {
        Ok(n) => n,
        Err(_) => return Verdict::NotRom("unreadable".into()),
    };
    head.truncate(read);

    for (magic, description) in NOT_ROM_MAGIC {
        if head.starts_with(magic) {
            return Verdict::NotRom(format!("{description}"));
        }
    }

    // Archives are judged by what is inside them.
    if platforms::ARCHIVE_EXTS.contains(&ext.as_str()) {
        return inspect_archive(path);
    }
    // One we have no extractor for gets the benefit of the doubt.
    if platforms::OPAQUE_ARCHIVE_EXTS.contains(&ext.as_str()) {
        return Verdict::Rom;
    }

    // Where a header is well defined, a mismatch is worth flagging — but not
    // worth discarding a file over. Headerless dumps do exist.
    if let Some((offset, candidates, system)) = expected_magic(platform) {
        if head.len() > offset && !candidates.iter().any(|m| starts_with_at(&head, offset, m)) {
            return Verdict::Suspect(format!("no {system} header"));
        }
    }

    Verdict::Rom
}

/// Judge an archive by its entries: a game dump holds a ROM, whereas an
/// emulator or an installer holds programs.
///
/// Only a program is disqualifying. An archive we cannot open, or one whose
/// contents we simply do not recognise, is still indexed — the bias stays
/// toward keeping things.
fn inspect_archive(path: &Path) -> Verdict {
    let Ok(names) = hashing::archive_entry_names(path) else {
        return Verdict::Rom;
    };
    if names.iter().any(|n| is_program(n)) {
        return Verdict::NotRom("an application, not a game".into());
    }
    Verdict::Rom
}

/// Group rejection reasons into a sentence for the scan summary.
pub fn summarise(reasons: &[String]) -> String {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for reason in reasons {
        *counts.entry(reason.as_str()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(reason, n)| format!("{n} × {reason}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn tmp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("arcadex-rc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Payload that will not compress away, so an archive built from it stays
    /// comfortably above the size floor and is judged on its contents.
    fn incompressible(len: usize) -> Vec<u8> {
        let mut state: u32 = 0x9e37_79b9;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state >> 24) as u8
            })
            .collect()
    }

    /// Padding so files clear the size floor.
    fn padded(prefix: &[u8], len: usize) -> Vec<u8> {
        let mut v = prefix.to_vec();
        v.resize(len.max(prefix.len()), 0);
        v
    }

    #[test]
    fn accepts_a_real_nes_rom() {
        let dir = tmp("nes");
        let rom = write(&dir, "Super Mario Bros.nes", &padded(b"NES\x1a", 40_000));
        assert_eq!(inspect(&rom, "nes"), Verdict::Rom);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_box_art_and_manuals_hiding_in_the_folder() {
        let dir = tmp("junk");
        let jpg = write(&dir, "Super Mario Bros.bin", &padded(b"\xff\xd8\xff\xe0", 9000));
        let pdf = write(&dir, "manual.bin", &padded(b"%PDF-1.4", 9000));
        let exe = write(&dir, "setup.bin", &padded(b"MZ\x90\x00", 9000));

        assert!(matches!(inspect(&jpg, "nes"), Verdict::NotRom(r) if r.contains("JPEG")));
        assert!(matches!(inspect(&pdf, "nes"), Verdict::NotRom(r) if r.contains("PDF")));
        assert!(matches!(inspect(&exe, "nes"), Verdict::NotRom(r) if r.contains("Windows")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_bios_files_by_name() {
        let dir = tmp("bios");
        for name in ["scph1001.bin", "gba_bios.bin", "disksys.rom", "BIOS.bin"] {
            let f = write(&dir, name, &padded(b"\x00", 200_000));
            assert!(
                matches!(inspect(&f, "ps1"), Verdict::NotRom(r) if r.contains("BIOS")),
                "{name} should be rejected"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_files_too_small_to_be_a_game() {
        let dir = tmp("tiny");
        let f = write(&dir, "notes.nes", b"hello");
        assert!(matches!(inspect(&f, "nes"), Verdict::NotRom(r) if r.contains("bytes")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A headerless dump is odd, not disqualifying — it gets indexed anyway.
    #[test]
    fn flags_a_header_mismatch_without_discarding_it() {
        let dir = tmp("suspect");
        let f = write(&dir, "mystery.nes", &padded(b"\x00\x01\x02\x03", 40_000));
        assert!(matches!(inspect(&f, "nes"), Verdict::Suspect(r) if r.contains("NES")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn accepts_every_n64_byte_order() {
        let dir = tmp("n64");
        for (i, magic) in [
            b"\x80\x37\x12\x40".as_slice(),
            b"\x37\x80\x40\x12".as_slice(),
            b"\x40\x12\x37\x80".as_slice(),
        ]
        .iter()
        .enumerate()
        {
            let f = write(&dir, &format!("game{i}.z64"), &padded(magic, 100_000));
            assert_eq!(inspect(&f, "n64"), Verdict::Rom, "byte order {i}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Archives are judged by what is inside them, not their own bytes.
    #[test]
    fn leaves_archives_alone() {
        let dir = tmp("zip");
        let f = write(&dir, "game.zip", &padded(b"PK\x03\x04", 9000));
        assert_eq!(inspect(&f, "nes"), Verdict::Rom);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An emulator unpacked into the ROM folder: its own directory is spotted
    /// by the programs in it, so nothing inside is ever inspected.
    #[test]
    fn spots_a_program_install_by_its_executables() {
        let dir = tmp("install");
        let emu = dir.join("Dolphin-x64");
        std::fs::create_dir_all(emu.join("Sys").join("GC")).unwrap();
        write(&emu, "Dolphin.exe", &padded(b"MZ", 9000));
        write(&emu, "Qt6Core.dll", &padded(b"MZ", 9000));
        write(&emu.join("Sys").join("GC"), "dsp_rom.bin", &padded(b"data", 9000));

        assert!(holds_a_program(&emu));
        // The ROM folder above it is not itself a program folder.
        assert!(!holds_a_program(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ...and a loose copy of one of those support files is still caught on
    /// its name, for when it turns up outside the install it came from.
    #[test]
    fn rejects_emulator_support_files_by_name() {
        let dir = tmp("support");
        for name in ["dsp_rom.bin", "dsp_coef.bin", "font_western.bin", "codehandler.bin"] {
            let f = write(&dir, name, &padded(b"data", 9000));
            assert!(
                matches!(inspect(&f, "gamecube"), Verdict::NotRom(r) if r.contains("BIOS")),
                "{name} should be rejected"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A 7z of an emulator is an application however it is packaged.
    #[test]
    fn rejects_an_archive_full_of_programs() {
        let dir = tmp("emuzip");

        let app = dir.join("Dolphin Emu.7z");
        {
            let mut w = sevenz_rust2::ArchiveWriter::create(&app).unwrap();
            for name in ["Dolphin-x64/Dolphin.exe", "Dolphin-x64/Sys/GC/dsp_rom.bin"] {
                w.push_archive_entry(
                    sevenz_rust2::ArchiveEntry::new_file(name),
                    Some(incompressible(4000).as_slice()),
                )
                .unwrap();
            }
            w.finish().unwrap();
        }
        assert!(
            matches!(inspect(&app, "wii"), Verdict::NotRom(r) if r.contains("application")),
            "an emulator archive should be rejected"
        );

        let game = dir.join("New Super Mario Bros Wii.7z");
        {
            let mut w = sevenz_rust2::ArchiveWriter::create(&game).unwrap();
            w.push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("New Super Mario Bros Wii.wbfs"),
                Some(incompressible(4000).as_slice()),
            )
            .unwrap();
            w.finish().unwrap();
        }
        assert_eq!(inspect(&game, "wii"), Verdict::Rom);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summarises_reasons_by_count() {
        let out = summarise(&[
            "a JPEG image".into(),
            "a JPEG image".into(),
            "a BIOS or firmware file".into(),
        ]);
        assert!(out.contains("2 × a JPEG image"));
        assert!(out.contains("1 × a BIOS or firmware file"));
    }
}

