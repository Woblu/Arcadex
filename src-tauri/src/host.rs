//! The machine Arcadex runs on, as opposed to the box it may be running in.
//!
//! The Steam Deck build is a Flatpak, and inside a Flatpak Arcadex sees its
//! own sandbox: `/usr` is the runtime's, and the host's `/usr/bin` and its
//! installed Flatpaks are not there at all. The emulators are on the host, so
//! starting one goes through `flatpak-spawn --host`, and asking whether a host
//! program is there, or what is in a host folder, asks the host.
//!
//! Folders the sandbox has been given - the home folder, SD cards under
//! `/run/media` - are the same paths inside and out, so a ROM, a save, or an
//! AppImage in `~/Applications` is reached directly.
//!
//! Outside a Flatpak, everything here is the ordinary thing.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use serde::Serialize;

/// Whether Arcadex is running inside a Flatpak.
pub fn sandboxed() -> bool {
    static SANDBOXED: OnceLock<bool> = OnceLock::new();
    *SANDBOXED.get_or_init(|| {
        std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists()
    })
}

/// The program and arguments that start `program` on the host, in `dir`.
fn wrap(program: &str, dir: Option<&Path>, sandboxed: bool) -> (String, Vec<String>) {
    if !sandboxed {
        return (program.to_string(), Vec::new());
    }
    let mut args = vec!["--host".to_string()];
    if let Some(dir) = dir {
        args.push(format!("--directory={}", dir.display()));
    }
    args.push(program.to_string());
    ("flatpak-spawn".to_string(), args)
}

/// A command that runs `program` on the host, starting in `dir`.
///
/// From inside a Flatpak, `flatpak-spawn --host` waits for the program and
/// returns its exit, so the time a game was played is measured the same way
/// either side of the sandbox.
pub fn command(program: &str, dir: Option<&Path>) -> Command {
    let inside = sandboxed();
    let (exe, args) = wrap(program, dir, inside);
    let mut cmd = Command::new(exe);
    cmd.args(args);
    if !inside {
        // Inside, the folder may not exist in the sandbox at all; the host
        // was told where to start instead.
        if let Some(dir) = dir.filter(|d| d.is_dir()) {
            cmd.current_dir(dir);
        }
    }
    cmd
}

/// Folders that are the same path inside the sandbox and out.
fn shared(path: &Path) -> bool {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    home.is_some_and(|h| path.starts_with(h))
        || ["/run/media", "/media", "/mnt"].iter().any(|p| path.starts_with(p))
}

/// Whether `path` is there on the host.
pub fn exists(path: &Path) -> bool {
    if !sandboxed() || (shared(path) && path.exists()) {
        return path.exists();
    }
    command("test", None)
        .arg("-e")
        .arg(path)
        .status()
        .is_ok_and(|s| s.success())
}

/// The names in a folder on the host.
///
/// A host folder the sandbox has its own version of - `/usr/bin` is the
/// runtime's in there - is asked about on the host, never read locally.
pub fn list(dir: &Path) -> Vec<String> {
    if !sandboxed() || shared(dir) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            return entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
        }
        if !sandboxed() {
            return Vec::new();
        }
    }
    command("ls", None)
        .arg("-1")
        .arg("--")
        .arg(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a folder needs asking the host about, rather than reading here.
pub fn host_only(dir: &Path) -> bool {
    sandboxed() && !shared(dir)
}

/// Whether this session is Steam's Game Mode - the Steam Deck's own, or any
/// other gamescope session - judged from the environment Steam starts
/// programs with.
fn game_mode_in(var: impl Fn(&str) -> Option<String>) -> bool {
    var("XDG_CURRENT_DESKTOP").is_some_and(|v| v.eq_ignore_ascii_case("gamescope"))
        || var("GAMESCOPE_WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
        || var("SteamGamepadUI").as_deref() == Some("1")
}

fn steam_deck_in(var: impl Fn(&str) -> Option<String>, board_vendor: Option<String>) -> bool {
    var("SteamDeck").as_deref() == Some("1")
        || board_vendor.is_some_and(|v| v.trim() == "Valve")
}

/// What Arcadex is running on, for the interface to fit itself to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    /// Inside a Flatpak.
    pub sandboxed: bool,
    /// Started from Steam's Game Mode: full screen, a controller, a TV or a
    /// handheld's screen.
    pub game_mode: bool,
    /// On a Steam Deck, in either mode.
    pub steam_deck: bool,
}

pub fn runtime() -> Runtime {
    let var = |k: &str| std::env::var(k).ok();
    Runtime {
        sandboxed: sandboxed(),
        game_mode: game_mode_in(var),
        steam_deck: steam_deck_in(
            var,
            std::fs::read_to_string("/sys/devices/virtual/dmi/id/board_vendor").ok(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn outside_a_flatpak_a_program_is_run_as_it_is() {
        let (exe, args) = wrap("/usr/bin/retroarch", Some(Path::new("/usr/bin")), false);
        assert_eq!(exe, "/usr/bin/retroarch");
        assert!(args.is_empty());
    }

    /// Inside, the host runs it, in the folder it would have started in.
    #[test]
    fn inside_a_flatpak_the_host_runs_it() {
        let (exe, args) = wrap(
            "/var/lib/flatpak/exports/bin/org.libretro.RetroArch",
            Some(Path::new("/var/lib/flatpak/exports/bin")),
            true,
        );
        assert_eq!(exe, "flatpak-spawn");
        assert_eq!(
            args,
            vec![
                "--host",
                "--directory=/var/lib/flatpak/exports/bin",
                "/var/lib/flatpak/exports/bin/org.libretro.RetroArch",
            ]
        );
    }

    #[test]
    fn knows_game_mode_when_it_sees_it() {
        assert!(game_mode_in(env(&[("XDG_CURRENT_DESKTOP", "gamescope")])));
        assert!(game_mode_in(env(&[("SteamGamepadUI", "1")])));
        assert!(game_mode_in(env(&[("GAMESCOPE_WAYLAND_DISPLAY", "gamescope-0")])));
        // The Deck's desktop is KDE, and that is not Game Mode.
        assert!(!game_mode_in(env(&[("XDG_CURRENT_DESKTOP", "KDE")])));
        assert!(!game_mode_in(env(&[])));
    }

    #[test]
    fn knows_a_steam_deck() {
        assert!(steam_deck_in(env(&[("SteamDeck", "1")]), None));
        assert!(steam_deck_in(env(&[]), Some("Valve\n".into())));
        assert!(!steam_deck_in(env(&[]), Some("LENOVO\n".into())));
        assert!(!steam_deck_in(env(&[]), None));
    }
}
