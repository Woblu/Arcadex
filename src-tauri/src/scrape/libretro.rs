//! libretro-thumbnails — box art with no account, no key and no quota.
//!
//! RetroArch's own artwork server. Files are addressed by the ROM's No-Intro
//! name, so no lookup call is needed: the URL *is* the query. That makes it the
//! only provider here that works the moment the app is installed.
//!
//! It serves images only — no description, developer, genre or release date —
//! so it complements ScreenScraper and TheGamesDB rather than replacing them.

use std::path::Path;

use super::GameMeta;
use crate::models::Game;

const BASE: &str = "https://thumbnails.libretro.com";

/// Our platform slug mapped to the system folder on the thumbnail server.
/// Every name here was taken from the libretro-thumbnails repository list —
/// a wrong name is a silent 404, so none of these are guesses.
const SYSTEMS: &[(&str, &str)] = &[
    ("nes", "Nintendo - Nintendo Entertainment System"),
    ("snes", "Nintendo - Super Nintendo Entertainment System"),
    ("n64", "Nintendo - Nintendo 64"),
    ("gamecube", "Nintendo - GameCube"),
    ("wii", "Nintendo - Wii"),
    ("gb", "Nintendo - Game Boy"),
    ("gbc", "Nintendo - Game Boy Color"),
    ("gba", "Nintendo - Game Boy Advance"),
    ("nds", "Nintendo - Nintendo DS"),
    ("n3ds", "Nintendo - Nintendo 3DS"),
    ("virtualboy", "Nintendo - Virtual Boy"),
    ("genesis", "Sega - Mega Drive - Genesis"),
    ("sms", "Sega - Master System - Mark III"),
    ("gamegear", "Sega - Game Gear"),
    ("sega32x", "Sega - 32X"),
    ("segacd", "Sega - Mega-CD - Sega CD"),
    ("saturn", "Sega - Saturn"),
    ("dreamcast", "Sega - Dreamcast"),
    ("wiiu", "Nintendo - Wii U"),
    ("ps1", "Sony - PlayStation"),
    ("ps2", "Sony - PlayStation 2"),
    ("ps3", "Sony - PlayStation 3"),
    ("ps4", "Sony - PlayStation 4"),
    ("psp", "Sony - PlayStation Portable"),
    ("xbox", "Microsoft - Xbox"),
    ("xbox360", "Microsoft - Xbox 360"),
    ("atari2600", "Atari - 2600"),
    ("atari7800", "Atari - 7800"),
    ("lynx", "Atari - Lynx"),
    ("jaguar", "Atari - Jaguar"),
    ("pcengine", "NEC - PC Engine - TurboGrafx 16"),
    ("neogeo", "SNK - Neo Geo"),
    ("ngp", "SNK - Neo Geo Pocket"),
    ("wonderswan", "Bandai - WonderSwan"),
    ("threedo", "The 3DO Company - 3DO"),
    ("msx", "Microsoft - MSX"),
    ("c64", "Commodore - 64"),
    ("amiga", "Commodore - Amiga"),
    ("colecovision", "Coleco - ColecoVision"),
    ("intellivision", "Mattel - Intellivision"),
    ("dos", "DOS"),
    ("scummvm", "ScummVM"),
    ("arcade", "MAME"),
];

pub fn system_for(platform: &str) -> Option<&'static str> {
    SYSTEMS
        .iter()
        .find(|(slug, _)| *slug == platform)
        .map(|(_, name)| *name)
}

/// libretro replaces these characters in a filename with an underscore.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '&' | '*' | '/' | ':' | '`' | '<' | '>' | '?' | '\\' | '|' => '_',
            c => c,
        })
        .collect()
}

/// The name the server indexes: the ROM's own filename without its extension,
/// tags and all. The cleaned-up display title would not match — the server
/// expects "Sonic The Hedgehog (USA, Europe)", not "Sonic The Hedgehog".
fn rom_key(game: &Game) -> String {
    let raw = game.inner_name.as_deref().unwrap_or(&game.filename);
    let stem = Path::new(raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(raw);
    sanitize(stem)
}

fn url_for(system: &str, folder: &str, key: &str) -> String {
    format!(
        "{BASE}/{}/{}/{}.png",
        urlencoding::encode(system),
        folder,
        urlencoding::encode(key)
    )
}

/// Build the three artwork URLs for a game, if its system is covered.
/// Returns `None` when we have no folder for that platform.
pub fn urls(game: &Game) -> Option<(String, String, String)> {
    let system = system_for(&game.platform)?;
    let key = rom_key(game);
    if key.trim().is_empty() {
        return None;
    }
    Some((
        url_for(system, "Named_Boxarts", &key),
        url_for(system, "Named_Snaps", &key),
        url_for(system, "Named_Titles", &key),
    ))
}

/// Look for artwork. One HEAD request decides whether this ROM is covered;
/// the snap and title screen are offered optimistically and simply do not
/// download if they are missing.
pub async fn lookup(client: &reqwest::Client, game: &Game) -> Option<GameMeta> {
    let (boxart, snap, title) = urls(game)?;

    let resp = client.head(&boxart).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    Some(GameMeta {
        cover_url: Some(boxart),
        screenshot_url: Some(snap),
        logo_url: Some(title),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(platform: &str, filename: &str, inner: Option<&str>) -> Game {
        Game {
            id: 1,
            path: format!("C:/roms/{filename}"),
            filename: filename.to_string(),
            platform: platform.to_string(),
            size: 0,
            crc32: None,
            md5: None,
            sha1: None,
            inner_name: inner.map(|s| s.to_string()),
            title: "cleaned title".into(),
            description: None,
            developer: None,
            publisher: None,
            genre: None,
            release_date: None,
            players: None,
            rating: None,
            region: None,
            cover_path: None,
            screenshot_path: None,
            logo_path: None,
            scrape_status: "pending".into(),
            scrape_source: None,
            favorite: false,
            hidden: false,
            play_count: 0,
            play_seconds: 0,
            last_played: None,
            added_at: 0,
            base_game_id: None,
            patch_path: None,
            cover_custom: false,
            title_custom: false,
        }
    }

    #[test]
    fn builds_the_documented_url() {
        let g = game("snes", "Super Mario World (USA).sfc", None);
        let (boxart, _, _) = urls(&g).unwrap();
        assert_eq!(
            boxart,
            "https://thumbnails.libretro.com/Nintendo%20-%20Super%20Nintendo%20Entertainment%20System\
             /Named_Boxarts/Super%20Mario%20World%20%28USA%29.png"
                .replace("\n", "")
                .replace("             ", "")
        );
    }

    /// A zipped ROM is indexed under the name inside the archive.
    #[test]
    fn prefers_the_name_inside_an_archive() {
        let g = game(
            "genesis",
            "sonic.zip",
            Some("Sonic The Hedgehog (USA, Europe).md"),
        );
        assert_eq!(rom_key(&g), "Sonic The Hedgehog (USA, Europe)");
    }

    /// The display title strips region tags, which would never match.
    #[test]
    fn uses_the_rom_name_not_the_display_title() {
        let g = game("gba", "Pokemon - Emerald Version (USA, Europe).gba", None);
        assert_eq!(rom_key(&g), "Pokemon - Emerald Version (USA, Europe)");
        assert_ne!(rom_key(&g), g.title);
    }

    #[test]
    fn replaces_the_characters_libretro_replaces() {
        assert_eq!(sanitize("Ratchet & Clank"), "Ratchet _ Clank");
        assert_eq!(sanitize("Where? When: How/Why"), "Where_ When_ How_Why");
        assert_eq!(sanitize("Normal Name (USA)"), "Normal Name (USA)");
    }

    #[test]
    fn unmapped_systems_are_skipped() {
        assert!(urls(&game("unknown", "thing.bin", None)).is_none());
        assert!(system_for("snes").is_some());
        assert!(system_for("nonexistent").is_none());
    }
}
