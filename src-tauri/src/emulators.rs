//! Standalone emulators Playdex knows the command line of.
//!
//! Browsing to an emulator in Settings fills in a command for you. For most
//! that is `"emulator" "{rom}"`, but several want something more, and the
//! ones that need it most are the ones people are least likely to guess:
//! RPCS3 opens its game list unless told `--no-gui`, and PCSX2 and
//! DuckStation stay open after the game closes unless told `-batch`, which
//! also keeps play time counting long after you stopped playing.
//!
//! Recognising an emulator is by the name of its executable, which is all
//! there is to go on before anything has run. The same recognition finds
//! installed emulators on disk, so the systems that need one can be set up
//! without browsing at all. Nothing is downloaded, and nothing here says
//! where to get an emulator, its firmware or its keys.

use std::path::{Path, PathBuf};

use serde::Serialize;

struct Known {
    /// What the emulator is called, for showing.
    name: &'static str,
    /// Lowercase starts of the executable's file name. Builds get named
    /// `pcsx2-qt`, `duckstation-qt-x64-ReleaseLTCG`, `PPSSPPWindows64`.
    stems: &'static [&'static str],
    /// Arguments, with `{rom}` where the game goes.
    args: &'static str,
    /// The systems it runs.
    platforms: &'static [&'static str],
    /// Its Flatpak id, which is also the name of the launcher Flatpak
    /// exports - nothing like the executable's own name.
    flatpak: &'static [&'static str],
}

/// The first match wins.
const KNOWN: &[Known] = &[
    // --- PlayStation
    Known {
        name: "RPCS3",
        stems: &["rpcs3"],
        args: r#"--no-gui "{rom}""#,
        platforms: &["ps3"],
        flatpak: &["net.rpcs3.RPCS3"],
    },
    Known {
        name: "shadPS4",
        stems: &["shadps4"],
        args: r#"-g "{rom}""#,
        platforms: &["ps4"],
        flatpak: &["net.shadps4.shadPS4"],
    },
    Known {
        name: "PCSX2",
        stems: &["pcsx2"],
        args: r#"-batch "{rom}""#,
        platforms: &["ps2"],
        flatpak: &["net.pcsx2.PCSX2"],
    },
    Known {
        name: "DuckStation",
        stems: &["duckstation"],
        args: r#"-batch "{rom}""#,
        platforms: &["ps1"],
        flatpak: &["org.duckstation.DuckStation"],
    },
    Known {
        name: "PPSSPP",
        stems: &["ppsspp"],
        args: r#""{rom}""#,
        platforms: &["psp"],
        flatpak: &["org.ppsspp.PPSSPP"],
    },
    // --- Microsoft
    Known {
        name: "Xenia",
        stems: &["xenia"],
        args: r#""{rom}""#,
        platforms: &["xbox360"],
        flatpak: &[],
    },
    Known {
        name: "xemu",
        stems: &["xemu"],
        args: r#"-dvd_path "{rom}""#,
        platforms: &["xbox"],
        flatpak: &["app.xemu.xemu"],
    },
    // --- Nintendo
    Known {
        name: "Cemu",
        stems: &["cemu"],
        args: r#"-g "{rom}""#,
        platforms: &["wiiu"],
        flatpak: &["info.cemu.Cemu"],
    },
    Known {
        name: "Dolphin",
        stems: &["dolphin"],
        args: r#"-b -e "{rom}""#,
        platforms: &["gamecube", "wii"],
        flatpak: &["org.DolphinEmu.dolphin-emu"],
    },
    // The Switch emulators descended from one codebase share its arguments.
    Known {
        name: "a Switch emulator",
        stems: &["eden", "citron", "sudachi", "suyu", "torzu", "yuzu"],
        args: r#"-g "{rom}""#,
        platforms: &["switch"],
        flatpak: &[],
    },
    Known {
        name: "Ryujinx",
        stems: &["ryujinx"],
        args: r#""{rom}""#,
        platforms: &["switch"],
        flatpak: &["io.github.ryubing.Ryujinx"],
    },
];

/// Programs that ship beside an emulator and share the start of its name
/// without being it: `DolphinTool`, `XeniaManager`, shadPS4's separate Qt
/// launcher, `Ryujinx.Headless.SDL2`, `yuzu-cmd`. Each takes other arguments,
/// or none, and a command filled in for the emulator would not run them.
const NOT_THE_EMULATOR: &[&str] = &[
    "launcher", "manager", "updat", "uninst", "setup", "install", "crash", "tool",
    "headless", "nogui", "cmd", "cli", "helper", "service",
];

/// The file name of a path, split by hand rather than with `Path`, so a
/// Windows path is read the same way wherever this runs.
fn leaf_of(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The file name without its extension, lowercased.
fn stem_of(path: &str) -> String {
    let leaf = leaf_of(path);
    let stem = match leaf.rfind('.') {
        Some(dot) if dot > 0 => &leaf[..dot],
        _ => leaf,
    };
    stem.to_ascii_lowercase()
}

/// The emulator an executable is, going by its name.
fn recognise(path: &str) -> Option<&'static Known> {
    let leaf = leaf_of(path);
    if let Some(k) = KNOWN.iter().find(|k| k.flatpak.contains(&leaf)) {
        return Some(k);
    }
    let stem = stem_of(path);
    if NOT_THE_EMULATOR.iter().any(|w| stem.contains(w)) {
        return None;
    }
    KNOWN
        .iter()
        .find(|k| k.stems.iter().any(|s| stem.starts_with(s)))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// The whole command, executable quoted, ready to save.
    pub command: String,
    /// The emulator it was recognised as, when it was.
    pub emulator: Option<String>,
}

/// The command to run a game with the emulator at `exe`.
pub fn suggest(exe: &str) -> Suggestion {
    let exe = crate::detect::clean_path(exe);
    match recognise(&exe) {
        Some(k) => Suggestion {
            command: format!(r#""{exe}" {}"#, k.args),
            emulator: Some(k.name.to_string()),
        },
        None => Suggestion {
            command: format!(r#""{exe}" "{{rom}}""#),
            emulator: None,
        },
    }
}

/// An emulator found installed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub emulator: String,
    pub path: String,
    pub command: String,
    /// The systems it runs.
    pub platforms: Vec<String>,
}

/// Whether a file could be a program someone runs, on this system.
fn runnable(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if cfg!(target_os = "windows") {
        ext == "exe"
    } else {
        // A Linux build is an AppImage, a bare binary or a Flatpak's
        // exported launcher; a macOS one is a bare binary in its bundle.
        ext.is_empty() || ext == "appimage" || recognise(&path.to_string_lossy()).map_or(false, |k| !k.flatpak.is_empty())
    }
}

/// Look `depth` folders down from `dir` for emulators.
fn search(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if depth == 0 {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            // Folders nobody keeps an emulator in, and which are slow to list.
            if name.starts_with('.')
                || name.starts_with('$')
                || matches!(name.as_str(), "windows" | "system volume information" | "node_modules")
            {
                continue;
            }
            search(&path, depth - 1, out);
        } else if runnable(&path) && recognise(&path.to_string_lossy()).is_some() {
            out.push(path);
        }
    }
}

/// Folders worth looking in, each with how deep to look.
///
/// Standalone emulators mostly come as an archive unzipped wherever someone
/// put it, so this covers the places people put them: a folder of emulators,
/// the drive root, Program Files, Downloads, and the folders EmuDeck and
/// ES-DE use. A folder called Emulators is searched a level deeper, since it
/// is usually sorted by system.
fn places() -> Vec<(PathBuf, usize)> {
    let mut out: Vec<(PathBuf, usize)> = Vec::new();
    let env = |key: &str| {
        std::env::var_os(key)
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
    };

    #[cfg(target_os = "windows")]
    {
        for drive in ('A'..='Z')
            .map(|c| PathBuf::from(format!("{c}:\\")))
            .filter(|p| p.is_dir())
        {
            out.push((drive.clone(), 1));
            for sub in ["Emulators", "Emulation", "Games\\Emulators"] {
                out.push((drive.join(sub), 2));
            }
        }
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(base) = env(key) {
                out.push((base, 1));
            }
        }
        if let Some(local) = env("LOCALAPPDATA") {
            out.push((local.join("Programs"), 1));
        }
        if let Some(roaming) = env("APPDATA") {
            out.push((roaming.join("EmuDeck").join("Emulators"), 2));
        }
        if let Some(home) = env("USERPROFILE") {
            for sub in ["Emulators", "Emulation", "EmuDeck\\Emulators", "ES-DE\\Emulators"] {
                out.push((home.join(sub), 2));
            }
            for sub in ["Desktop", "Downloads", "Documents", "Games"] {
                out.push((home.join(sub), 1));
            }
            out.push((home.join("scoop").join("apps"), 2));
        }
    }

    #[cfg(target_os = "macos")]
    {
        let mut apps = vec![PathBuf::from("/Applications")];
        if let Some(home) = env("HOME") {
            apps.push(home.join("Applications"));
        }
        // An app bundle keeps its program in Contents/MacOS.
        for dir in apps {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()) == Some("app") {
                        out.push((p.join("Contents").join("MacOS"), 0));
                    }
                }
            }
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for dir in ["/usr/bin", "/usr/local/bin", "/var/lib/flatpak/exports/bin"] {
            out.push((PathBuf::from(dir), 0));
        }
        out.push((PathBuf::from("/opt"), 1));
        if let Some(home) = env("HOME") {
            out.push((home.join(".local/bin"), 0));
            out.push((home.join(".local/share/flatpak/exports/bin"), 0));
            // Where EmuDeck puts its AppImages.
            out.push((home.join("Applications"), 1));
            out.push((home.join("Emulators"), 2));
            out.push((home.join("Downloads"), 1));
        }
    }

    out
}

/// Every known emulator installed in the usual places, in the order of
/// `KNOWN`, the same program found twice listed once.
pub fn detect() -> Vec<Found> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for (dir, depth) in places() {
        search(&dir, depth, &mut paths);
    }
    found_from(paths)
}

fn found_from(paths: Vec<PathBuf>) -> Vec<Found> {
    let mut seen = std::collections::HashSet::new();
    let mut found: Vec<(usize, Found)> = Vec::new();
    for path in paths {
        let key = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !seen.insert(key) {
            continue;
        }
        let text = path.to_string_lossy().to_string();
        let Some(known) = recognise(&text) else { continue };
        let rank = KNOWN
            .iter()
            .position(|k| std::ptr::eq(k, known))
            .unwrap_or(usize::MAX);
        found.push((
            rank,
            Found {
                emulator: known.name.to_string(),
                command: suggest(&text).command,
                platforms: known.platforms.iter().map(|p| p.to_string()).collect(),
                path: text,
            },
        ));
    }
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().map(|(_, f)| f).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_in_what_each_emulator_needs() {
        assert_eq!(
            suggest(r"C:\Emulators\RPCS3\rpcs3.exe").command,
            r#""C:\Emulators\RPCS3\rpcs3.exe" --no-gui "{rom}""#
        );
        assert_eq!(
            suggest(r"D:\xenia\xenia_canary.exe").emulator.as_deref(),
            Some("Xenia")
        );
        assert_eq!(
            suggest(r"C:\PCSX2\pcsx2-qt.exe").command,
            r#""C:\PCSX2\pcsx2-qt.exe" -batch "{rom}""#
        );
        assert_eq!(suggest("/usr/bin/eden").command, r#""/usr/bin/eden" -g "{rom}""#);
        assert_eq!(suggest(r"C:\Cemu\Cemu.exe").emulator.as_deref(), Some("Cemu"));
        // A Flatpak's launcher is named for the app, not the program.
        assert_eq!(
            suggest("/var/lib/flatpak/exports/bin/net.rpcs3.RPCS3").command,
            r#""/var/lib/flatpak/exports/bin/net.rpcs3.RPCS3" --no-gui "{rom}""#
        );
    }

    /// The programs that come with an emulator are not the emulator, and a
    /// command written for one would not run the other.
    #[test]
    fn does_not_mistake_an_emulators_companions_for_it() {
        for exe in [
            r"C:\Dolphin\DolphinTool.exe",
            r"C:\Xenia\XeniaManager.exe",
            r"C:\shadPS4\shadPS4QtLauncher.exe",
            r"C:\Ryujinx\Ryujinx.Headless.SDL2.exe",
            r"C:\yuzu\yuzu-cmd.exe",
            r"C:\PCSX2\pcsx2-updater.exe",
        ] {
            assert_eq!(suggest(exe).emulator, None, "{exe}");
        }
        assert_eq!(
            suggest(r"C:\DuckStation\duckstation-qt-x64-ReleaseLTCG.exe")
                .emulator
                .as_deref(),
            Some("DuckStation")
        );
    }

    /// Explorer's "Copy as path" quotes are stripped, or the command would
    /// end up with two sets.
    #[test]
    fn strips_pasted_quotes() {
        assert_eq!(
            suggest(r#""C:\xemu\xemu.exe""#).command,
            r#""C:\xemu\xemu.exe" -dvd_path "{rom}""#
        );
    }

    #[test]
    fn anything_else_gets_the_plain_command() {
        let s = suggest(r"C:\Emulators\Mystery\mystery.exe");
        assert_eq!(s.command, r#""C:\Emulators\Mystery\mystery.exe" "{rom}""#);
        assert_eq!(s.emulator, None);
    }

    /// A folder of emulators sorted by system, the way people keep them.
    #[test]
    fn finds_emulators_unzipped_into_a_folder() {
        let root = std::env::temp_dir().join(format!("playdex-emus-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let exe = if cfg!(target_os = "windows") { ".exe" } else { "" };
        let put = |rel: &str| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"MZ").unwrap();
        };
        put(&format!("PS3/rpcs3-v0.0.38/rpcs3{exe}"));
        put(&format!("Xbox 360/xenia_canary{exe}"));
        put(&format!("Dolphin/DolphinTool{exe}"));
        put("Dolphin/readme.txt");
        // Too deep to be worth looking for.
        put(&format!("a/b/c/cemu{exe}"));

        let mut paths = Vec::new();
        search(&root, 2, &mut paths);
        let found = found_from(paths);
        let names: Vec<&str> = found.iter().map(|f| f.emulator.as_str()).collect();
        assert_eq!(names, vec!["RPCS3", "Xenia"], "in the order emulators are listed");
        assert_eq!(found[0].platforms, vec!["ps3".to_string()]);
        assert!(found[0].command.contains("--no-gui"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detection_never_panics() {
        // Whatever this machine has, the search must finish cleanly. Printed
        // so `cargo test -- --nocapture` doubles as a diagnostic.
        for f in detect() {
            println!("found {} at {}", f.emulator, f.path);
        }
    }
}
