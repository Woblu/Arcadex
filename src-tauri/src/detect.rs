//! Finding an existing RetroArch install so the user does not have to.
//!
//! RetroArch has no single canonical install location — it ships as a portable
//! zip, through Steam, and via several package managers — so this checks the
//! places each of those put it, then falls back to `PATH`.

use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

/// Keeps `reg.exe` from flashing a console window.
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Users paste paths out of Explorer's "Copy as path", which wraps them in
/// quotes. A stored path with literal quotes matches nothing on disk.
pub fn clean_path(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// `canonicalize` hands back Windows' extended-length form (`\\?\D:\...`).
/// It is valid but ugly in the UI, and some tools refuse it.
fn tidy(path: PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            // Leave UNC network paths (\\?\UNC\server\share) alone.
            if !rest.starts_with("UNC\\") {
                return PathBuf::from(rest.to_string());
            }
        }
    }
    path
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedEmulator {
    pub path: String,
    pub cores_dir: Option<String>,
    /// How it was found, shown to the user so a surprising result is explicable.
    pub source: String,
    /// Cores present in that folder, as a sanity signal.
    pub core_count: usize,
}

fn exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "retroarch.exe"
    } else {
        "retroarch"
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

/// Candidate executables, in the order we would rather find them.
fn candidates() -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = Vec::new();
    let exe = exe_name();

    #[cfg(target_os = "windows")]
    {
        // The official Windows build is a portable folder, most often unzipped
        // to the drive root.
        for root in ["C:\\RetroArch-Win64", "C:\\RetroArch", "C:\\Program Files\\RetroArch"] {
            out.push((PathBuf::from(root).join(exe), "standard install"));
        }
        for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(base) = env_path(key) {
                out.push((base.join("RetroArch").join(exe), "program files"));
                out.push((
                    base.join("Programs").join("RetroArch").join(exe),
                    "program files",
                ));
            }
        }
        if let Some(home) = env_path("USERPROFILE") {
            out.push((
                home.join("scoop").join("apps").join("retroarch").join("current").join(exe),
                "scoop",
            ));
        }
        out.push((
            PathBuf::from("C:\\ProgramData\\chocolatey\\lib\\retroarch\\tools").join(exe),
            "chocolatey",
        ));
        for lib in steam_libraries() {
            out.push((
                lib.join("steamapps").join("common").join("RetroArch").join(exe),
                "Steam",
            ));
        }
        // A portable unzip on any drive, not just C:.
        for drive in drive_roots() {
            out.push((drive.join("RetroArch-Win64").join(exe), "portable install"));
            out.push((drive.join("RetroArch").join(exe), "portable install"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        out.push((
            PathBuf::from("/Applications/RetroArch.app/Contents/MacOS/RetroArch"),
            "Applications",
        ));
        if let Some(home) = env_path("HOME") {
            out.push((
                home.join("Applications/RetroArch.app/Contents/MacOS/RetroArch"),
                "Applications",
            ));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for p in ["/usr/bin", "/usr/local/bin", "/bin", "/var/lib/flatpak/exports/bin"] {
            out.push((PathBuf::from(p).join(exe), "system"));
        }
        if let Some(home) = env_path("HOME") {
            out.push((
                home.join(".local/share/flatpak/exports/bin/org.libretro.RetroArch"),
                "flatpak",
            ));
        }
    }

    let _ = exe;
    out
}

/// Ask the registry where Steam is. This is the only reliable answer when
/// Steam lives outside Program Files — on a second drive, say.
#[cfg(target_os = "windows")]
fn steam_root_from_registry() -> Option<PathBuf> {
    for (hive, value) in [
        (r"HKCU\Software\Valve\Steam", "SteamPath"),
        (r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (r"HKLM\SOFTWARE\Valve\Steam", "InstallPath"),
    ] {
        let mut cmd = std::process::Command::new("reg");
        cmd.args(["query", hive, "/v", value]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        let Ok(out) = cmd.output() else { continue };
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Some(pos) = line.find("REG_SZ") {
                let path = line[pos + 6..].trim();
                if !path.is_empty() {
                    return Some(PathBuf::from(path));
                }
            }
        }
    }
    None
}

/// Every fixed drive letter that currently exists.
#[cfg(target_os = "windows")]
fn drive_roots() -> Vec<PathBuf> {
    ('A'..='Z')
        .map(|c| PathBuf::from(format!("{c}:\\")))
        .filter(|p| p.is_dir())
        .collect()
}

/// Steam can keep games on several drives; the library list says where.
#[cfg(target_os = "windows")]
fn steam_libraries() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(reg) = steam_root_from_registry() {
        roots.push(reg);
    }
    for key in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = env_path(key) {
            roots.push(base.join("Steam"));
        }
    }
    // Steam is routinely installed on a second drive.
    for drive in drive_roots() {
        roots.push(drive.join("Program Files (x86)").join("Steam"));
        roots.push(drive.join("Program Files").join("Steam"));
        roots.push(drive.join("Steam"));
        roots.push(drive.join("SteamLibrary"));
    }
    roots.retain(|r| r.is_dir());

    let mut libraries: Vec<PathBuf> = roots.clone();
    for root in roots {
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        let Ok(text) = std::fs::read_to_string(&vdf) else {
            continue;
        };
        // Entries look like:  "path"    "D:\\SteamLibrary"
        for line in text.lines() {
            let line = line.trim();
            if !line.starts_with("\"path\"") {
                continue;
            }
            if let Some(start) = line[6..].find('"') {
                let rest = &line[6 + start + 1..];
                if let Some(end) = rest.find('"') {
                    let raw = rest[..end].replace("\\\\", "\\");
                    if !raw.is_empty() {
                        libraries.push(PathBuf::from(raw));
                    }
                }
            }
        }
    }
    libraries
}

/// Anything on PATH, as a last resort.
fn from_path_env() -> Option<PathBuf> {
    let exe = exe_name();
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|p| p.is_file())
}

fn core_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    }
}

fn count_cores(dir: &Path) -> usize {
    let ext = core_extension();
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case(ext))
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

/// Where this install keeps its cores.
pub fn find_cores_dir(exe: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(dir) = exe.parent() {
        candidates.push(dir.join("cores"));
        // The macOS bundle keeps them beside the binary inside Contents.
        candidates.push(dir.join("../Resources/cores"));
    }

    #[cfg(target_os = "windows")]
    if let Some(appdata) = env_path("APPDATA") {
        candidates.push(appdata.join("RetroArch").join("cores"));
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = env_path("HOME") {
        candidates.push(home.join("Library/Application Support/RetroArch/cores"));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(home) = env_path("HOME") {
            candidates.push(home.join(".config/retroarch/cores"));
            candidates.push(home.join(".var/app/org.libretro.RetroArch/config/retroarch/cores"));
        }
        candidates.push(PathBuf::from("/usr/lib/libretro"));
        candidates.push(PathBuf::from("/usr/lib/x86_64-linux-gnu/libretro"));
    }

    // Prefer a folder that actually has cores in it.
    let existing: Vec<PathBuf> = candidates.into_iter().filter(|p| p.is_dir()).collect();
    existing
        .iter()
        .find(|p| count_cores(p) > 0)
        .or_else(|| existing.first())
        .map(|p| p.to_path_buf())
}

/// Look for an installed RetroArch. Returns the first one that exists.
pub fn find_retroarch() -> Option<DetectedEmulator> {
    let found = candidates()
        .into_iter()
        .find(|(p, _)| p.is_file())
        .map(|(p, source)| (p, source))
        .or_else(|| from_path_env().map(|p| (p, "PATH")))?;

    let (exe, source) = found;
    let exe = tidy(exe.canonicalize().unwrap_or(exe));
    let cores_dir = find_cores_dir(&exe).map(tidy);
    let core_count = cores_dir.as_deref().map(count_cores).unwrap_or(0);

    Some(DetectedEmulator {
        path: exe.to_string_lossy().to_string(),
        cores_dir: cores_dir.map(|p| p.to_string_lossy().to_string()),
        source: source.to_string(),
        core_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_platform_candidates() {
        let list = candidates();
        assert!(!list.is_empty());
        // Every candidate must end in the platform's executable name. The
        // macOS bundle spells it `RetroArch`, and Flatpak exports a launcher
        // named after the app id rather than the program.
        for (p, _) in &list {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            assert!(
                name.eq_ignore_ascii_case(exe_name()) || name == "org.libretro.RetroArch",
                "unexpected candidate: {}",
                p.display()
            );
        }
    }

    #[test]
    fn counts_only_core_libraries() {
        let dir = std::env::temp_dir().join(format!("playdex-cores-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("snes9x_libretro.{}", core_extension())), b"x").unwrap();
        std::fs::write(dir.join("readme.txt"), b"x").unwrap();

        assert_eq!(count_cores(&dir), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strips_quotes_users_paste_from_explorer() {
        // Exactly what Explorer's "Copy as path" puts on the clipboard.
        assert_eq!(
            clean_path(r#""D:\Program Files (x86)\Steam\retroarch.exe""#),
            r"D:\Program Files (x86)\Steam\retroarch.exe"
        );
        assert_eq!(
            clean_path("  C:\\RetroArch\\retroarch.exe  "),
            r"C:\RetroArch\retroarch.exe"
        );
        assert_eq!(clean_path("'/usr/bin/retroarch'"), "/usr/bin/retroarch");
        assert_eq!(clean_path(""), "");
    }

    #[test]
    fn detection_never_panics() {
        // Whatever this machine has, the search must complete cleanly.
        // Printed so `cargo test -- --nocapture` doubles as a diagnostic.
        match find_retroarch() {
            Some(found) => println!(
                "detected: {} (via {}, {} cores, cores dir {:?})",
                found.path, found.source, found.core_count, found.cores_dir
            ),
            None => println!("detected: nothing"),
        }
    }
}
