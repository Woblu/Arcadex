//! Save files and save states.
//!
//! RetroArch names both after the content, exactly as it does cheat files, and
//! keeps them in directories set in its own config — which may be absent,
//! meaning "next to the ROM". So the search covers the configured folders, the
//! per-core subfolders RetroArch makes when sorting is on, and the ROM's own
//! directory.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::models::SaveEntry;

/// Battery saves keep the game's own progress; states are emulator snapshots.
fn classify(file_name: &str) -> Option<(&'static str, Option<i64>)> {
    let lower = file_name.to_ascii_lowercase();

    // "game.state", "game.state1", "game.state.auto"
    if let Some(rest) = lower.rsplit_once(".state").map(|(_, r)| r.to_string()) {
        if lower.contains(".state") {
            let slot = rest.trim_start_matches('.').parse::<i64>().ok();
            if rest.is_empty() || slot.is_some() || rest == ".auto" {
                return Some(("state", slot));
            }
        }
    }

    for ext in [".srm", ".sav", ".rtc", ".eep", ".fla", ".mcr"] {
        if lower.ends_with(ext) {
            return Some(("save", None));
        }
    }
    None
}

/// Every folder RetroArch might have put this game's saves in.
fn candidate_dirs(exe: &Path, rom_path: &Path, core_folder: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    for key in ["savefile_directory", "savestate_directory"] {
        if let Some(dir) = crate::cheats::config_dir(exe, key) {
            // RetroArch can sort saves into a folder per core.
            dirs.push(dir.join(core_folder));
            dirs.push(dir);
        }
    }

    // With no directory configured, saves land beside the ROM.
    if let Some(parent) = rom_path.parent() {
        dirs.push(parent.to_path_buf());
    }
    if let Some(base) = exe.parent() {
        dirs.push(base.join("saves"));
        dirs.push(base.join("states"));
    }

    dirs.sort();
    dirs.dedup();
    dirs.retain(|d| d.is_dir());
    dirs
}

fn modified_secs(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Whether a save file is this ROM's: its name exactly, then a dot.
///
/// The name alone at the start is not enough. "Tetris (USA)" begins
/// "Tetris (USA) (Rev 1).state1", and that state - another dump's, perhaps
/// another game's - was listed, backed up and offered for deletion here.
fn belongs_to(file_lower: &str, rom_lower: &str) -> bool {
    file_lower
        .strip_prefix(rom_lower)
        .is_some_and(|rest| rest.starts_with('.'))
}

/// Find the saves and states belonging to one ROM.
pub fn find(exe: &Path, rom_path: &Path, rom_name: &str, core_folder: &str) -> Vec<SaveEntry> {
    let mut out: Vec<SaveEntry> = Vec::new();
    let needle = rom_name.to_ascii_lowercase();

    for dir in candidate_dirs(exe, rom_path, core_folder) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !belongs_to(&name.to_ascii_lowercase(), &needle) {
                continue;
            }
            let Some((kind, slot)) = classify(name) else {
                continue;
            };

            let path_str = path.to_string_lossy().to_string();
            if out.iter().any(|e| e.path == path_str) {
                continue;
            }

            // RetroArch writes a thumbnail beside a state when asked to.
            let shot = path.with_extension(
                format!(
                    "{}.png",
                    path.extension().and_then(|e| e.to_str()).unwrap_or("state")
                )
                .as_str(),
            );
            let screenshot = shot.is_file().then(|| shot.to_string_lossy().to_string());

            out.push(SaveEntry {
                kind: kind.to_string(),
                slot,
                name: name.to_string(),
                path: path_str,
                size: entry.metadata().map(|m| m.len() as i64).unwrap_or(0),
                modified: modified_secs(&path),
                screenshot,
            });
        }
    }

    // Newest first, saves before states so progress is the headline.
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.modified.cmp(&a.modified)));
    out
}

/// Copy the given files into a dated folder so a bad state can be undone.
pub fn back_up(entries: &[SaveEntry], root: &Path, game_title: &str) -> Result<PathBuf> {
    let stamp = crate::db::now();
    let safe: String = game_title
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let dest = root.join("save-backups").join(format!("{} {}", safe.trim(), stamp));
    std::fs::create_dir_all(&dest)?;

    for entry in entries {
        let from = Path::new(&entry.path);
        if let Some(name) = from.file_name() {
            let _ = std::fs::copy(from, dest.join(name));
        }
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_is_this_roms_only_if_named_for_it_exactly() {
        let rom = "tetris (usa)";
        assert!(belongs_to("tetris (usa).srm", rom));
        assert!(belongs_to("tetris (usa).state3", rom));
        assert!(belongs_to("tetris (usa).state.auto", rom));
        assert!(!belongs_to("tetris (usa) (rev 1).state1", rom));
        assert!(!belongs_to("tetris (usa)2.srm", rom));
        assert!(!belongs_to("tetris.srm", rom));
    }

    #[test]
    fn tells_battery_saves_from_states() {
        assert_eq!(classify("Super Mario Bros.srm"), Some(("save", None)));
        assert_eq!(classify("Super Mario Bros.sav"), Some(("save", None)));
        assert_eq!(classify("Super Mario Bros.state"), Some(("state", None)));
        assert_eq!(classify("Super Mario Bros.state3"), Some(("state", Some(3))));
        assert_eq!(classify("Super Mario Bros.state.auto"), Some(("state", None)));
        // Not ours.
        assert_eq!(classify("Super Mario Bros.nes"), None);
        assert_eq!(classify("notes.txt"), None);
    }

    #[test]
    fn finds_saves_next_to_the_rom_when_nothing_is_configured() {
        let dir = std::env::temp_dir().join(format!("playdex-saves-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let exe = dir.join("retroarch.exe");
        std::fs::write(&exe, b"").unwrap();
        let rom = dir.join("Super Mario Bros.nes");
        std::fs::write(&rom, b"NES\x1a").unwrap();
        std::fs::write(dir.join("Super Mario Bros.srm"), b"save").unwrap();
        std::fs::write(dir.join("Super Mario Bros.state1"), b"state").unwrap();
        // A different game's save must not appear.
        std::fs::write(dir.join("Zelda.srm"), b"nope").unwrap();

        let found = find(&exe, &rom, "Super Mario Bros", "Mesen");
        let names: Vec<&str> = found.iter().map(|e| e.name.as_str()).collect();

        assert!(names.contains(&"Super Mario Bros.srm"));
        assert!(names.contains(&"Super Mario Bros.state1"));
        assert!(!names.iter().any(|n| n.contains("Zelda")));
        assert_eq!(found.iter().filter(|e| e.kind == "state").count(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backs_up_into_a_dated_folder() {
        let dir = std::env::temp_dir().join(format!("playdex-bk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("Game.srm");
        std::fs::write(&src, b"progress").unwrap();

        let entry = SaveEntry {
            kind: "save".into(),
            slot: None,
            name: "Game.srm".into(),
            path: src.to_string_lossy().to_string(),
            size: 8,
            modified: 0,
            screenshot: None,
        };
        let dest = back_up(&[entry], &dir, "Game: Special/Edition").unwrap();

        assert!(dest.join("Game.srm").is_file());
        // The title is sanitised into a usable folder name. Check the folder
        // itself, not the whole path — a Windows drive letter has a colon.
        let folder = dest.file_name().unwrap().to_string_lossy();
        assert!(!folder.contains(':'), "unsanitised folder name: {folder}");
        assert!(!folder.contains('/'), "unsanitised folder name: {folder}");
        assert!(folder.starts_with("Game_ Special_Edition"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
