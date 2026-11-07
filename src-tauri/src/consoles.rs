//! Games that are a folder rather than a file.
//!
//! Everything older than the PlayStation 3 is one file per game, or a cue
//! sheet and its tracks. The newer consoles install games, so a dump is the
//! installed folder: a PlayStation 3 game is `PS3_GAME/USRDIR/EBOOT.BIN` and
//! a few thousand data files beside it, a Wii U game is `code`, `content` and
//! `meta`. Walked file by file, one of those is a single game called "EBOOT"
//! and a few hundred more called whatever its data happens to be named.
//!
//! So these folders are recognised whole, by files the console itself put
//! there rather than by anything somebody named. Each becomes one entry: the
//! file the emulator is handed, the title the console wrote down, and the
//! size of the whole folder. Nothing beneath it is walked.
//!
//! An Xbox 360 Games on Demand package is the same idea in one file with no
//! extension, so it lives here too.

use std::path::{Path, PathBuf};

/// A game found by its shape on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleGame {
    pub platform: &'static str,
    /// The folder that is the game. Everything inside belongs to it.
    pub root: PathBuf,
    /// The file the emulator is handed. `None` for a folder that belongs to
    /// a game without being one: an update, or downloadable content.
    pub boot: Option<PathBuf>,
    /// The title the console wrote into the game's own metadata.
    pub title: Option<String>,
    /// The disc or catalogue number, `BLUS30443` or `CUSA00003`.
    pub serial: Option<String>,
}

impl ConsoleGame {
    /// What to call this game where a filename would normally go. The folder
    /// name is usually the dump's own name, which is what the artwork
    /// servers index, unless all it says is the serial.
    pub fn name(&self) -> String {
        let folder = self
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        // A package's folder is named for its content type, which says
        // even less than a serial.
        let says_only_serial = self
            .serial
            .as_deref()
            .map_or(false, |s| folder.eq_ignore_ascii_case(s))
            || looks_like_serial(&folder)
            || PACKAGE_FOLDERS.contains(&folder.to_ascii_lowercase().as_str());
        match &self.title {
            Some(title) if says_only_serial || folder.is_empty() => title.clone(),
            _ => folder,
        }
    }
}

/// `BLUS30443`, `CUSA00003`, `PPSA01234`: four letters and five digits.
fn looks_like_serial(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 9
        && b[..4].iter().all(|c| c.is_ascii_alphabetic())
        && b[4..].iter().all(|c| c.is_ascii_digit())
}

/// A child of `dir` matched without regard to case, since dumps made on
/// different machines disagree about `EBOOT.BIN` versus `eboot.bin`. The
/// path that comes back is spelt the way it is on disk.
fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_str().map_or(false, |n| n.eq_ignore_ascii_case(name)))
        .map(|e| e.path())
}

fn child_file(dir: &Path, name: &str) -> Option<PathBuf> {
    child(dir, name).filter(|p| p.is_file())
}

fn child_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    child(dir, name).filter(|p| p.is_dir())
}

/// Recognise a folder that is one game.
///
/// Called for every directory a scan walks into, so the common answer - an
/// ordinary folder of ROMs - has to be cheap: one listing of `dir`, and
/// nothing more unless a name in it is one of the few that matter.
pub fn identify_dir(dir: &Path) -> Option<ConsoleGame> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(|n| n.to_ascii_lowercase()))
        .collect();
    let has = |n: &str| names.iter().any(|x| x == n);

    if has("ps3_game") {
        return ps3_disc(dir);
    }
    if has("param.sfo") && has("usrdir") {
        return ps3_installed(dir);
    }
    if has("sce_sys") {
        return playstation_app(dir);
    }
    if has("code") && has("meta") {
        return wiiu(dir);
    }
    if has("default.xex") {
        return Some(ConsoleGame {
            platform: "xbox360",
            root: dir.to_path_buf(),
            boot: child_file(dir, "default.xex"),
            title: None,
            serial: None,
        });
    }
    None
}

/// A PlayStation 3 disc copied off as a folder - the layout RPCS3 calls a
/// disc game: `PS3_GAME/PARAM.SFO` and `PS3_GAME/USRDIR/EBOOT.BIN`.
fn ps3_disc(dir: &Path) -> Option<ConsoleGame> {
    let game = child_dir(dir, "PS3_GAME")?;
    let sfo = child_file(&game, "PARAM.SFO").and_then(|p| Sfo::read(&p));
    let boot = child_dir(&game, "USRDIR").and_then(|u| child_file(&u, "EBOOT.BIN"));
    Some(ConsoleGame {
        platform: "ps3",
        root: dir.to_path_buf(),
        boot,
        title: sfo.as_ref().and_then(|s| s.title()),
        serial: sfo.as_ref().and_then(|s| s.string("TITLE_ID")),
    })
}

/// A PlayStation 3 game as it sits in `dev_hdd0/game`: a download, a PS1 or
/// PS2 classic, or the update for a disc. `PARAM.SFO` says which, and only
/// the first kinds are something to start. An update in the same folder is
/// still claimed, so its data is not walked as games of its own.
fn ps3_installed(dir: &Path) -> Option<ConsoleGame> {
    let sfo = Sfo::read(&child_file(dir, "PARAM.SFO")?)?;
    let category = sfo.string("CATEGORY").unwrap_or_default();
    // GD is game data - a patch for a disc game, booted through the disc.
    let playable = category != "GD";
    let boot = if playable {
        child_dir(dir, "USRDIR").and_then(|u| child_file(&u, "EBOOT.BIN"))
    } else {
        None
    };
    Some(ConsoleGame {
        platform: "ps3",
        root: dir.to_path_buf(),
        boot,
        title: sfo.title(),
        serial: sfo.string("TITLE_ID"),
    })
}

/// A PlayStation 4 or 5 dump: `eboot.bin` beside an `sce_sys` folder. The
/// two are told apart by what `sce_sys` holds - the PlayStation 4 kept the
/// PlayStation 3's `param.sfo`, the PlayStation 5 moved to `param.json`.
fn playstation_app(dir: &Path) -> Option<ConsoleGame> {
    let sys = child_dir(dir, "sce_sys")?;
    let eboot = child_file(dir, "eboot.bin");

    if let Some(json) = child_file(&sys, "param.json") {
        let param = std::fs::read_to_string(json)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        let (title, serial) = param.as_ref().map(ps5_identity).unwrap_or_default();
        return Some(ConsoleGame {
            platform: "ps5",
            root: dir.to_path_buf(),
            boot: eboot,
            title,
            serial,
        });
    }

    let sfo = child_file(&sys, "param.sfo").and_then(|p| Sfo::read(&p))?;
    // A PlayStation Vita dump is laid out exactly the same way, and only its
    // PARAM.SFO tells them apart. Vita3K installs games rather than booting a
    // folder, so it is left to the ordinary walk.
    if sfo.has("PSP2_SYSTEM_VER") || sfo.has("PSP2_DISP_VER") {
        return None;
    }
    // gd is a game; gp is its patch and ac its downloadable content, which
    // are claimed so their files stay out of the library but are not games.
    let category = sfo.string("CATEGORY").unwrap_or_default().to_ascii_lowercase();
    let playable = !matches!(category.as_str(), "gp" | "ac");
    Some(ConsoleGame {
        platform: "ps4",
        root: dir.to_path_buf(),
        boot: eboot.filter(|_| playable),
        title: sfo.title(),
        serial: sfo.string("TITLE_ID"),
    })
}

/// A PlayStation 5 title and catalogue number out of `param.json`. The title
/// is kept per language, under the default language's key.
fn ps5_identity(param: &serde_json::Value) -> (Option<String>, Option<String>) {
    let serial = param
        .get("titleId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let localized = param.get("localizedParameters");
    let lang = localized
        .and_then(|l| l.get("defaultLanguage"))
        .and_then(|v| v.as_str())
        .unwrap_or("en-US");
    let title = localized
        .and_then(|l| l.get(lang).or_else(|| l.get("en-US")))
        .and_then(|l| l.get("titleName"))
        .and_then(|v| v.as_str())
        .map(tidy_title)
        .filter(|t| !t.is_empty());
    (title, serial)
}

/// A Wii U game kept loose: `code` holds the executable, an `.rpx`, and
/// `meta/meta.xml` holds the title.
fn wiiu(dir: &Path) -> Option<ConsoleGame> {
    let code = child_dir(dir, "code")?;
    let meta = child_dir(dir, "meta").and_then(|m| child_file(&m, "meta.xml"))?;

    let mut rpx: Vec<PathBuf> = std::fs::read_dir(&code)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map_or(false, |e| e.eq_ignore_ascii_case("rpx"))
        })
        .collect();
    rpx.sort();

    let xml = std::fs::read_to_string(&meta).unwrap_or_default();
    Some(ConsoleGame {
        platform: "wiiu",
        root: dir.to_path_buf(),
        boot: rpx.into_iter().next(),
        title: xml_text(&xml, "longname_en").map(|t| tidy_title(&t)),
        serial: xml_text(&xml, "product_code"),
    })
}

/// The text inside the first `<tag ...>` element. meta.xml is flat and
/// machine-written, so this is all the XML reading it needs.
fn xml_text(xml: &str, tag: &str) -> Option<String> {
    let open = xml.find(&format!("<{tag}"))?;
    let start = open + xml[open..].find('>')? + 1;
    let end = start + xml[start..].find(&format!("</{tag}>"))?;
    let text = xml[start..end]
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Titles are written for a console's own menu: line breaks where the menu
/// wraps them, trademark signs, the odd doubled space.
fn tidy_title(raw: &str) -> String {
    raw.replace(['™', '®', '©'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The game a file is the boot file of, if it is one.
///
/// This is the question indexing asks of every file, so a game added by
/// dropping its `EBOOT.BIN` on the window is described exactly as one found
/// by a scan. The answer only counts when the folder, read on its own terms,
/// names this very file - a stray `EBOOT.BIN` somewhere else is not a game.
pub fn for_boot_file(path: &Path) -> Option<ConsoleGame> {
    if let Some(game) = read_package(path) {
        return Some(game);
    }

    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let parent = path.parent()?;
    let parent_name = parent.file_name()?.to_str()?.to_ascii_lowercase();

    let root = match name.as_str() {
        // PS3_GAME/USRDIR/EBOOT.BIN on a disc, USRDIR/EBOOT.BIN installed.
        "eboot.bin" if parent_name == "usrdir" => {
            let above = parent.parent()?;
            let above_name = above.file_name()?.to_str()?.to_ascii_lowercase();
            if above_name == "ps3_game" {
                above.parent()?.to_path_buf()
            } else {
                above.to_path_buf()
            }
        }
        // A PlayStation 4 or 5 keeps it at the top.
        "eboot.bin" => parent.to_path_buf(),
        "default.xex" => parent.to_path_buf(),
        _ if parent_name == "code" && name.ends_with(".rpx") => parent.parent()?.to_path_buf(),
        _ => return None,
    };

    identify_dir(&root).filter(|g| g.boot.as_deref() == Some(path))
}

// ------------------------------------------------------------ PARAM.SFO

/// The value table every PlayStation since the PSP keeps its title in.
///
/// A short header, then a table of index entries each pointing at a key in
/// the key table and a value in the data table. Strings are UTF-8 padded
/// with nulls. The integers (parental level, resolution flags) say nothing
/// a library wants, so they are kept as keys with no value.
struct Sfo {
    entries: Vec<(String, Option<String>)>,
}

impl Sfo {
    fn read(path: &Path) -> Option<Sfo> {
        let bytes = std::fs::read(path).ok()?;
        // A real one is a few hundred bytes; anything vast is not one.
        if bytes.len() > 1024 * 1024 {
            return None;
        }
        Sfo::parse(&bytes)
    }

    fn parse(b: &[u8]) -> Option<Sfo> {
        let u16_at = |o: usize| Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?));
        let u32_at = |o: usize| Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?));

        if b.get(0..4)? != b"\0PSF" {
            return None;
        }
        let key_table = u32_at(0x08)? as usize;
        let data_table = u32_at(0x0C)? as usize;
        let count = u32_at(0x10)? as usize;
        if count > 256 {
            return None;
        }

        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let at = 0x14 + i * 16;
            let key_start = key_table + u16_at(at)? as usize;
            let format = u16_at(at + 2)?;
            let len = u32_at(at + 4)? as usize;
            let data_start = data_table + u32_at(at + 12)? as usize;

            let key_bytes = b.get(key_start..)?;
            let key_end = key_bytes.iter().position(|&c| c == 0)?;
            let key = String::from_utf8_lossy(&key_bytes[..key_end]).to_string();

            let data = b.get(data_start..data_start.checked_add(len)?)?;
            let value = (format != 0x0404).then(|| {
                let end = data.iter().position(|&c| c == 0).unwrap_or(data.len());
                String::from_utf8_lossy(&data[..end]).to_string()
            });
            entries.push((key, value));
        }
        Some(Sfo { entries })
    }

    fn string(&self, key: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.as_deref())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    }

    fn has(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    fn title(&self) -> Option<String> {
        self.string("TITLE").map(|t| tidy_title(&t)).filter(|t| !t.is_empty())
    }
}

// ------------------------------------------------- Xbox 360 STFS packages

/// The content types worth listing: Games on Demand, an Arcade title, and a
/// demo. Saves, updates and downloadable content use the same container.
const PACKAGE_GAME_TYPES: &[u32] = &[0x0000_7000, 0x000D_0000, 0x0008_0000];

/// The folder names those content types are filed under, in
/// `<title id>/<content type>/<package>`. A package has no extension, so
/// this is how a scan knows which extensionless files are worth opening.
pub const PACKAGE_FOLDERS: &[&str] = &["00007000", "000d0000", "00080000"];

/// Whether an extensionless file sits where a game package would.
pub fn could_be_package(path: &Path) -> bool {
    path.extension().is_none()
        && path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map_or(false, |n| PACKAGE_FOLDERS.contains(&n.to_ascii_lowercase().as_str()))
}

/// Read an Xbox 360 package header: a Games on Demand title, an Arcade game.
///
/// The header is plain, whoever signed it. The magic says who did - `CON `
/// for a console, `LIVE` and `PIRS` for Microsoft - and at fixed offsets
/// after that sit the content type, the title id and the display name in
/// UTF-16, which is exactly the title the dashboard showed.
fn read_package(path: &Path) -> Option<ConsoleGame> {
    use std::io::Read;

    if path.extension().is_some() {
        return None;
    }
    let mut head = vec![0u8; PACKAGE_HEADER_BYTES];
    let mut file = std::fs::File::open(path).ok()?;
    file.read_exact(&mut head).ok()?;
    let (title, serial) = parse_package(&head)?;

    Some(ConsoleGame {
        platform: "xbox360",
        root: path.parent()?.to_path_buf(),
        boot: Some(path.to_path_buf()),
        title,
        serial: Some(serial),
    })
}

/// How much of a package's header holds everything read from it.
const PACKAGE_HEADER_BYTES: usize = 0x1711;

/// The title and title id in a package header, if it is a game's.
fn parse_package(head: &[u8]) -> Option<(Option<String>, String)> {
    if head.len() < PACKAGE_HEADER_BYTES || !matches!(&head[0..4], b"CON " | b"LIVE" | b"PIRS") {
        return None;
    }
    let content_type = u32::from_be_bytes(head[0x344..0x348].try_into().ok()?);
    if !PACKAGE_GAME_TYPES.contains(&content_type) {
        return None;
    }
    let title_id = u32::from_be_bytes(head[0x360..0x364].try_into().ok()?);

    let utf16 = |bytes: &[u8]| -> Option<String> {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .collect();
        let s = tidy_title(&String::from_utf16_lossy(&units));
        (!s.is_empty()).then_some(s)
    };
    // The first display name is English; the title name is the fallback.
    let title = utf16(&head[0x411..0x491]).or_else(|| utf16(&head[0x1691..0x1711]));
    Some((title, format!("{title_id:08X}")))
}

// ------------------------------------------------------------ in archives

/// A console game kept in a zip or 7z: a Games on Demand package, or one of
/// the folder layouts above, archived whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivedGame {
    pub platform: &'static str,
    pub title: Option<String>,
    pub serial: Option<String>,
    /// The file the emulator is handed, by its name inside the archive.
    pub boot_entry: String,
    /// Everything in the archive, unpacked.
    pub size: u64,
}

/// Recognise a console game inside a zip or 7z.
///
/// Read off the archive's list of names, the same layouts `identify_dir`
/// finds on disk, and then the one small file inside that names the game. A
/// zipped package is a 45 KB header and several gigabytes of data, and only
/// the header is read.
pub fn identify_archive(path: &Path) -> Option<ArchivedGame> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !crate::platforms::ARCHIVE_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let entries = crate::hashing::archive_entries(path).ok()?;
    let read = |name: &str, limit: u64| crate::hashing::read_archive_entry(path, name, limit).ok();
    identify_entries(&entries, Some(&read))
}

/// The file inside an archived console game that the emulator starts from,
/// judged by names alone.
///
/// For launching and for showing the launch command, which only need to know
/// where to point once it is unpacked. Reading a header out of a large solid
/// 7z can mean decompressing much of it, and the game was already identified
/// properly when it was indexed.
pub fn archived_boot_entry(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !crate::platforms::ARCHIVE_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let entries = crate::hashing::archive_entries(path).ok()?;
    identify_entries(&entries, None).map(|g| g.boot_entry)
}

type ReadEntry<'a> = &'a dyn Fn(&str, u64) -> Option<Vec<u8>>;

/// The work of `identify_archive`, given the names and a way to read one.
/// With no way to read, layouts are recognised by their names and nothing
/// is learnt about the title.
fn identify_entries(entries: &[(String, u64)], read: Option<ReadEntry>) -> Option<ArchivedGame> {
    let names_only = read.is_none();
    let read = |name: &str, limit: u64| read.and_then(|r| r(name, limit));
    let size: u64 = entries.iter().map(|(_, s)| s).sum();
    // Compared lowercased with forward slashes; read by the archive's own
    // spelling.
    let names: Vec<(String, &str)> = entries
        .iter()
        .map(|(n, _)| (n.replace('\\', "/").to_ascii_lowercase(), n.as_str()))
        .collect();
    let find = |lower: &str| names.iter().find(|(l, _)| l == lower).map(|(_, n)| *n);
    // The folder part of a name that ends in `tail`, if it does at a folder
    // boundary: "Game/PS3_GAME/PARAM.SFO" and "PS3_GAME/PARAM.SFO" both match.
    let prefix_of = |lower: &str, tail: &str| -> Option<String> {
        let prefix = lower.strip_suffix(tail)?;
        (prefix.is_empty() || prefix.ends_with('/')).then(|| prefix.to_string())
    };
    let game = |platform, title, serial, boot: &str| ArchivedGame {
        platform,
        title,
        serial,
        boot_entry: boot.to_string(),
        size,
    };
    let sfo = |name: &str| read(name, 1024 * 1024).and_then(|b| Sfo::parse(&b));

    // A Games on Demand or Arcade package: a file with no extension in a
    // folder named for its content type.
    for (lower, name) in &names {
        let Some((dir, leaf)) = lower.rsplit_once('/') else { continue };
        let folder = dir.rsplit('/').next().unwrap_or(dir);
        if leaf.contains('.') || !PACKAGE_FOLDERS.contains(&folder) {
            continue;
        }
        // Saves and downloadable content use the same container, but live in
        // other folders; the header is still checked when it can be read.
        if names_only {
            return Some(game("xbox360", None, None, name));
        }
        if let Some((title, serial)) =
            read(name, PACKAGE_HEADER_BYTES as u64).and_then(|h| parse_package(&h))
        {
            return Some(game("xbox360", title, Some(serial), name));
        }
    }

    for (lower, name) in &names {
        // A PlayStation 3 disc copied off as a folder.
        if let Some(p) = prefix_of(lower, "ps3_game/param.sfo") {
            if let Some(boot) = find(&format!("{p}ps3_game/usrdir/eboot.bin")) {
                let s = sfo(name);
                return Some(game(
                    "ps3",
                    s.as_ref().and_then(|s| s.title()),
                    s.as_ref().and_then(|s| s.string("TITLE_ID")),
                    boot,
                ));
            }
        }
        // A PlayStation 4 or 5 dump.
        if let Some(p) = prefix_of(lower, "sce_sys/param.json") {
            if let Some(boot) = find(&format!("{p}eboot.bin")) {
                let json = read(name, 1024 * 1024)
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
                let (title, serial) = json.as_ref().map(ps5_identity).unwrap_or_default();
                return Some(game("ps5", title, serial, boot));
            }
        }
        if let Some(p) = prefix_of(lower, "sce_sys/param.sfo") {
            if let Some(boot) = find(&format!("{p}eboot.bin")) {
                let s = sfo(name);
                let playable = s.as_ref().map_or(names_only, |s| {
                    let category = s.string("CATEGORY").unwrap_or_default().to_ascii_lowercase();
                    let vita = s.has("PSP2_SYSTEM_VER") || s.has("PSP2_DISP_VER");
                    !vita && !matches!(category.as_str(), "gp" | "ac")
                });
                if playable {
                    return Some(game(
                        "ps4",
                        s.as_ref().and_then(|s| s.title()),
                        s.as_ref().and_then(|s| s.string("TITLE_ID")),
                        boot,
                    ));
                }
            }
        }
        // An installed PlayStation 3 game, which is not an update.
        if let Some(p) = prefix_of(lower, "param.sfo") {
            if !p.ends_with("ps3_game/") && !p.ends_with("sce_sys/") {
                if let Some(boot) = find(&format!("{p}usrdir/eboot.bin")) {
                    let s = sfo(name);
                    let playable = s
                        .as_ref()
                        .map_or(names_only, |s| s.string("CATEGORY").as_deref() != Some("GD"));
                    if playable {
                        return Some(game(
                            "ps3",
                            s.as_ref().and_then(|s| s.title()),
                            s.as_ref().and_then(|s| s.string("TITLE_ID")),
                            boot,
                        ));
                    }
                }
            }
        }
        // A loose Wii U game.
        if let Some(p) = prefix_of(lower, "meta/meta.xml") {
            let code = format!("{p}code/");
            let mut rpx: Vec<&str> = names
                .iter()
                .filter(|(l, _)| {
                    l.strip_prefix(&code)
                        .map_or(false, |rest| !rest.contains('/') && rest.ends_with(".rpx"))
                })
                .map(|(_, n)| *n)
                .collect();
            rpx.sort();
            if let Some(boot) = rpx.first() {
                let xml = read(name, 1024 * 1024)
                    .map(|b| String::from_utf8_lossy(&b).to_string())
                    .unwrap_or_default();
                return Some(game(
                    "wiiu",
                    xml_text(&xml, "longname_en").map(|t| tidy_title(&t)),
                    xml_text(&xml, "product_code"),
                    boot,
                ));
            }
        }
        // An extracted Xbox 360 disc.
        if prefix_of(lower, "default.xex").is_some() {
            return Some(game("xbox360", None, None, name));
        }
    }
    None
}

/// How much a console game takes up on disk: the whole folder, or for a
/// package the header file and the `.data` folder beside it.
pub fn size_on_disk(game: &ConsoleGame) -> u64 {
    let Some(boot) = &game.boot else { return 0 };
    if read_package(boot).is_some() {
        let file = std::fs::metadata(boot).map(|m| m.len()).unwrap_or(0);
        let mut data = boot.clone().into_os_string();
        data.push(".data");
        return file + crate::launch::dir_size(Path::new(&data));
    }
    crate::launch::dir_size(&game.root)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "playdex-consoles-{}-{}-{:?}",
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

    /// Build a PARAM.SFO the way the console lays one out.
    pub(crate) fn sfo(fields: &[(&str, &str)]) -> Vec<u8> {
        let count = fields.len() + 1;
        let mut keys = Vec::new();
        let mut data = Vec::new();
        let mut index = Vec::new();

        let mut add = |key: &str, format: u16, value: &[u8], max: usize| {
            let key_off = keys.len() as u16;
            keys.extend_from_slice(key.as_bytes());
            keys.push(0);
            let data_off = data.len() as u32;
            let mut padded = value.to_vec();
            padded.resize(max, 0);
            data.extend_from_slice(&padded);
            index.extend_from_slice(&key_off.to_le_bytes());
            index.extend_from_slice(&format.to_le_bytes());
            index.extend_from_slice(&(value.len() as u32).to_le_bytes());
            index.extend_from_slice(&(max as u32).to_le_bytes());
            index.extend_from_slice(&data_off.to_le_bytes());
        };
        for (k, v) in fields {
            let mut s = v.as_bytes().to_vec();
            s.push(0);
            let max = s.len().div_ceil(4) * 4;
            add(k, 0x0204, &s, max);
        }
        add("PARENTAL_LEVEL", 0x0404, &5u32.to_le_bytes(), 4);

        while keys.len() % 4 != 0 {
            keys.push(0);
        }
        let key_table = 0x14 + count * 16;
        let data_table = key_table + keys.len();

        let mut out = b"\0PSF".to_vec();
        out.extend_from_slice(&0x0101u32.to_le_bytes());
        out.extend_from_slice(&(key_table as u32).to_le_bytes());
        out.extend_from_slice(&(data_table as u32).to_le_bytes());
        out.extend_from_slice(&(count as u32).to_le_bytes());
        out.extend_from_slice(&index);
        out.extend_from_slice(&keys);
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn reads_a_param_sfo() {
        let bytes = sfo(&[
            ("CATEGORY", "DG"),
            ("TITLE", "Demon's Souls™"),
            ("TITLE_ID", "BLUS30443"),
        ]);
        let parsed = Sfo::parse(&bytes).expect("should parse");
        assert_eq!(parsed.string("TITLE_ID").as_deref(), Some("BLUS30443"));
        assert_eq!(parsed.string("CATEGORY").as_deref(), Some("DG"));
        // The trademark sign is for the console's menu, not a library.
        assert_eq!(parsed.title().as_deref(), Some("Demon's Souls"));
        // An integer is a key with no text.
        assert!(parsed.has("PARENTAL_LEVEL"));
        assert_eq!(parsed.string("PARENTAL_LEVEL"), None);

        assert!(Sfo::parse(b"not an sfo at all").is_none());
        // Cut short, it fails cleanly rather than reading past the end.
        assert!(Sfo::parse(&bytes[..40]).is_none());
    }

    /// The case all of this is for: a disc game copied off as a folder is one
    /// game, named from PARAM.SFO, booted from its EBOOT.BIN.
    #[test]
    fn a_playstation_3_disc_folder_is_one_game() {
        let root = scratch("ps3disc").join("Demon's Souls (USA)");
        put(
            &root.join("PS3_GAME").join("PARAM.SFO"),
            &sfo(&[("CATEGORY", "DG"), ("TITLE", "Demon's Souls"), ("TITLE_ID", "BLUS30443")]),
        );
        let eboot = root.join("PS3_GAME").join("USRDIR").join("EBOOT.BIN");
        put(&eboot, b"SCE\0 not really");
        put(&root.join("PS3_DISC.SFB"), b".SFB");

        let game = identify_dir(&root).expect("a disc folder");
        assert_eq!(game.platform, "ps3");
        assert_eq!(game.boot.as_deref(), Some(eboot.as_path()));
        assert_eq!(game.title.as_deref(), Some("Demon's Souls"));
        assert_eq!(game.serial.as_deref(), Some("BLUS30443"));
        // The folder's own name is the dump's name, so it is kept.
        assert_eq!(game.name(), "Demon's Souls (USA)");

        // Asked about the boot file, the answer is the same game.
        assert_eq!(for_boot_file(&eboot), Some(game));
        // Asked about a data file in the same folder, there is none.
        let data = root.join("PS3_GAME").join("USRDIR").join("data.bin");
        put(&data, b"x");
        assert_eq!(for_boot_file(&data), None);

        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    /// A download in dev_hdd0/game is a game; the update beside it is not.
    #[test]
    fn an_installed_playstation_3_game_but_not_its_update() {
        let base = scratch("ps3hdd");
        let game_dir = base.join("NPUB30910");
        put(
            &game_dir.join("PARAM.SFO"),
            &sfo(&[("CATEGORY", "HG"), ("TITLE", "Journey"), ("TITLE_ID", "NPUB30910")]),
        );
        put(&game_dir.join("USRDIR").join("EBOOT.BIN"), b"SCE\0");

        let game = identify_dir(&game_dir).unwrap();
        assert_eq!(game.platform, "ps3");
        assert!(game.boot.is_some());
        // The folder is only the serial, so the title stands in for it.
        assert_eq!(game.name(), "Journey");

        let patch = base.join("BLUS30443");
        put(
            &patch.join("PARAM.SFO"),
            &sfo(&[("CATEGORY", "GD"), ("TITLE", "Demon's Souls"), ("TITLE_ID", "BLUS30443")]),
        );
        put(&patch.join("USRDIR").join("EBOOT.BIN"), b"SCE\0");
        let update = identify_dir(&patch).expect("still claimed");
        assert_eq!(update.boot, None, "an update is not something to start");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn tells_a_playstation_4_dump_from_a_5() {
        let base = scratch("ps45");

        let ps4 = base.join("CUSA00003");
        put(
            &ps4.join("sce_sys").join("param.sfo"),
            &sfo(&[("CATEGORY", "gd"), ("TITLE", "Bloodborne"), ("TITLE_ID", "CUSA00003")]),
        );
        put(&ps4.join("eboot.bin"), b"\x4f\x15\x3d\x1d");
        let game = identify_dir(&ps4).unwrap();
        assert_eq!(game.platform, "ps4");
        assert_eq!(game.name(), "Bloodborne");
        assert_eq!(for_boot_file(&ps4.join("eboot.bin")), Some(game));

        let patch = base.join("CUSA00003-patch");
        put(
            &patch.join("sce_sys").join("param.sfo"),
            &sfo(&[("CATEGORY", "gp"), ("TITLE", "Bloodborne"), ("TITLE_ID", "CUSA00003")]),
        );
        put(&patch.join("eboot.bin"), b"\x4f\x15\x3d\x1d");
        assert_eq!(identify_dir(&patch).unwrap().boot, None);

        // A Vita dump has the same shape and is not a PlayStation 4 game.
        let vita = base.join("PCSE00000");
        put(
            &vita.join("sce_sys").join("param.sfo"),
            &sfo(&[("CATEGORY", "gd"), ("PSP2_SYSTEM_VER", "0"), ("TITLE", "Persona 4 Golden")]),
        );
        put(&vita.join("eboot.bin"), b"SCE\0");
        assert_eq!(identify_dir(&vita), None);

        let ps5 = base.join("Astro Bot");
        put(
            &ps5.join("sce_sys").join("param.json"),
            br#"{"titleId":"PPSA01234","localizedParameters":{"defaultLanguage":"en-US","en-US":{"titleName":"ASTRO BOT"}}}"#,
        );
        put(&ps5.join("eboot.bin"), b"\x4f\x15\x3d\x1d");
        let game = identify_dir(&ps5).unwrap();
        assert_eq!(game.platform, "ps5");
        assert_eq!(game.title.as_deref(), Some("ASTRO BOT"));
        assert_eq!(game.serial.as_deref(), Some("PPSA01234"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_loose_wii_u_game_is_named_from_its_meta() {
        let root = scratch("wiiu").join("Some Folder");
        put(&root.join("code").join("Game.rpx"), b"\x7fELF");
        put(&root.join("code").join("lib.rpl"), b"\x7fELF");
        put(&root.join("content").join("data.bin"), b"x");
        put(
            &root.join("meta").join("meta.xml"),
            br#"<?xml version="1.0"?><menu><longname_en type="string" length="512">Super Mario
3D World</longname_en><product_code type="string" length="32">WUP-P-ARDE</product_code></menu>"#,
        );
        let game = identify_dir(&root).unwrap();
        assert_eq!(game.platform, "wiiu");
        assert_eq!(game.title.as_deref(), Some("Super Mario 3D World"));
        assert_eq!(game.boot.as_deref(), Some(root.join("code").join("Game.rpx").as_path()));
        assert!(for_boot_file(&root.join("code").join("Game.rpx")).is_some());

        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn an_extracted_xbox_360_disc_boots_its_xex() {
        let root = scratch("x360").join("Halo 3");
        put(&root.join("default.xex"), b"XEX2");
        put(&root.join("media").join("intro.bin"), b"x");
        let game = identify_dir(&root).unwrap();
        assert_eq!(game.platform, "xbox360");
        assert_eq!(game.name(), "Halo 3");
        assert_eq!(for_boot_file(&root.join("default.xex")), Some(game));
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    pub(crate) fn package(content_type: u32, name: &str) -> Vec<u8> {
        let mut h = vec![0u8; 0xB000];
        h[0..4].copy_from_slice(b"LIVE");
        h[0x344..0x348].copy_from_slice(&content_type.to_be_bytes());
        h[0x360..0x364].copy_from_slice(&0x4D53_07E6u32.to_be_bytes());
        for (i, unit) in name.encode_utf16().enumerate() {
            h[0x411 + i * 2..0x411 + i * 2 + 2].copy_from_slice(&unit.to_be_bytes());
        }
        h
    }

    #[test]
    fn reads_a_games_on_demand_package() {
        let base = scratch("god");
        let file = base.join("4D5307E6").join("00007000").join("7D0B6B8C2C8A1E3F4B2D");
        put(&file, &package(0x7000, "Halo 3"));
        put(&base.join("4D5307E6").join("00007000").join("7D0B6B8C2C8A1E3F4B2D.data").join("Data0000"), &[0u8; 100]);

        assert!(could_be_package(&file));
        let game = for_boot_file(&file).expect("a package");
        assert_eq!(game.platform, "xbox360");
        assert_eq!(game.title.as_deref(), Some("Halo 3"));
        assert_eq!(game.serial.as_deref(), Some("4D5307E6"));
        assert_eq!(game.name(), "Halo 3", "the folder is only a content type");
        assert!(size_on_disk(&game) >= 0xB000 + 100);

        // A save game uses the same container and is not a game.
        let save = base.join("4D5307E6").join("00000001").join("SAVE");
        put(&save, &package(0x1, "Halo 3 save"));
        assert!(!could_be_package(&save));
        assert_eq!(for_boot_file(&save), None);

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Build a zip from (name, bytes) pairs; a name ending in `/` is a folder.
    pub(crate) fn zip_of(path: &Path, files: &[(&str, Vec<u8>)]) {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, bytes) in files {
            if name.ends_with('/') {
                w.add_directory(*name, opts).unwrap();
            } else {
                w.start_file(*name, opts).unwrap();
                w.write_all(bytes).unwrap();
            }
        }
        w.finish().unwrap();
    }

    pub(crate) fn zipped_package(path: &Path) {
        let base = "Skate 3/454108E6/00007000/";
        let mut files = vec![
            (base, Vec::new()),
            ("Skate 3/454108E6/00007000/07A9631BB10EE556B1CE", package(0x7000, "Skate 3")),
        ];
        let data: Vec<String> = (0..3)
            .map(|n| format!("{base}07A9631BB10EE556B1CE.data/Data000{n}"))
            .collect();
        for (n, name) in data.iter().enumerate() {
            files.push((name.as_str(), vec![n as u8; 1000]));
        }
        zip_of(path, &files);
    }

    /// The case that prompted it: a Games on Demand package zipped up whole.
    /// Nothing in it has an extension, so it used to be Unidentified.
    #[test]
    fn recognises_a_zipped_games_on_demand_package() {
        let dir = scratch("zipgod");
        let zip_path = dir.join("Skate 3.zip");
        zipped_package(&zip_path);

        let game = identify_archive(&zip_path).expect("a zipped package");
        assert_eq!(game.platform, "xbox360");
        assert_eq!(game.title.as_deref(), Some("Skate 3"));
        assert_eq!(game.serial.as_deref(), Some("4D5307E6"));
        assert_eq!(game.boot_entry, "Skate 3/454108E6/00007000/07A9631BB10EE556B1CE");
        assert_eq!(game.size, 0xB000 + 3000, "everything, once unpacked");
        // Launching needs only to know which file to start, from the names.
        assert_eq!(archived_boot_entry(&zip_path).as_deref(), Some(game.boot_entry.as_str()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recognises_zipped_console_folders() {
        let dir = scratch("zipfolders");

        let ps3 = dir.join("Demon's Souls.zip");
        zip_of(
            &ps3,
            &[
                (
                    "Demon's Souls (USA)/PS3_GAME/PARAM.SFO",
                    sfo(&[("CATEGORY", "DG"), ("TITLE", "Demon's Souls"), ("TITLE_ID", "BLUS30443")]),
                ),
                ("Demon's Souls (USA)/PS3_GAME/USRDIR/EBOOT.BIN", b"SCE\0".to_vec()),
                ("Demon's Souls (USA)/PS3_GAME/USRDIR/data.bin", vec![0; 100]),
            ],
        );
        let game = identify_archive(&ps3).unwrap();
        assert_eq!(game.platform, "ps3");
        assert_eq!(game.title.as_deref(), Some("Demon's Souls"));
        assert_eq!(game.boot_entry, "Demon's Souls (USA)/PS3_GAME/USRDIR/EBOOT.BIN");

        // Zipped from inside the folder, so the layout starts at the top.
        let ps4 = dir.join("Bloodborne.zip");
        zip_of(
            &ps4,
            &[
                (
                    "sce_sys/param.sfo",
                    sfo(&[("CATEGORY", "gd"), ("TITLE", "Bloodborne"), ("TITLE_ID", "CUSA00003")]),
                ),
                ("eboot.bin", vec![0; 10]),
            ],
        );
        let game = identify_archive(&ps4).unwrap();
        assert_eq!((game.platform, game.boot_entry.as_str()), ("ps4", "eboot.bin"));

        let x360 = dir.join("Halo 3.zip");
        zip_of(&x360, &[("Halo 3/default.xex", b"XEX2".to_vec()), ("Halo 3/media/a.bin", vec![0; 10])]);
        assert_eq!(identify_archive(&x360).unwrap().platform, "xbox360");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A zipped cartridge ROM, or an update with nothing to start, is not a
    /// console game and is left to the ordinary rules.
    #[test]
    fn an_ordinary_archive_is_not_a_console_game() {
        let dir = scratch("zipplain");
        let rom = dir.join("Super Metroid.zip");
        zip_of(&rom, &[("Super Metroid (USA).sfc", vec![0; 1000])]);
        assert_eq!(identify_archive(&rom), None);

        let patch = dir.join("Demon's Souls update.zip");
        zip_of(
            &patch,
            &[
                ("BLUS30443/PARAM.SFO", sfo(&[("CATEGORY", "GD"), ("TITLE_ID", "BLUS30443")])),
                ("BLUS30443/USRDIR/EBOOT.BIN", vec![0; 10]),
            ],
        );
        assert_eq!(identify_archive(&patch), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_ordinary_rom_folder_is_not_a_game() {
        let root = scratch("plain");
        put(&root.join("Super Metroid.sfc"), b"x");
        put(&root.join("eboot.bin"), b"x");
        assert_eq!(identify_dir(&root), None);
        assert_eq!(for_boot_file(&root.join("eboot.bin")), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
