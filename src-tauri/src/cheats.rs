//! Game Genie and other cheat codes.
//!
//! A Game Genie was a cartridge you slotted your game into; you typed in a
//! short code and it rewrote a byte of the game as it loaded — infinite lives,
//! start on a later level, walk through walls. The codes are just an encoded
//! memory address and replacement value, which is why emulators can apply them
//! directly with no hardware.
//!
//! Codes come from libretro-database, which indexes them by the ROM's No-Intro
//! name exactly as the thumbnail server does. No account or key is needed.

use crate::error::{AppError, Result};
use crate::models::{Cheat, Game};
use crate::scrape::libretro;
use std::path::{Path, PathBuf};

const RAW: &str = "https://raw.githubusercontent.com/libretro/libretro-database/master/cht";

/// The name libretro files this ROM under, and the Game Genie variant some
/// systems keep separately.
fn candidate_names(rom_name: &str) -> Vec<String> {
    vec![
        format!("{rom_name}.cht"),
        format!("{rom_name} (Game Genie).cht"),
    ]
}

/// Parse RetroArch's cheat format:
///   cheats = 2
///   cheat0_desc = "Infinite Lives"
///   cheat0_code = "SXIOPO"
///   cheat0_enable = false
pub fn parse(body: &str) -> Vec<Cheat> {
    let mut descs: Vec<(usize, String)> = Vec::new();
    let mut codes: Vec<(usize, String)> = Vec::new();
    let mut enabled: Vec<usize> = Vec::new();

    for line in body.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim().trim_matches('"');
        if value.is_empty() {
            continue;
        }

        let Some(rest) = key.strip_prefix("cheat") else {
            continue;
        };
        let (index, field) = match rest.split_once('_') {
            Some((i, f)) => (i, f),
            None => continue,
        };
        let Ok(index) = index.parse::<usize>() else {
            continue;
        };

        match field {
            "desc" => descs.push((index, value.to_string())),
            "code" => codes.push((index, value.to_string())),
            // Both formats appear in the wild: bare true and quoted "true".
            "enable" if value.eq_ignore_ascii_case("true") => enabled.push(index),
            _ => {}
        }
    }

    descs.sort_by_key(|(i, _)| *i);
    descs
        .into_iter()
        .filter_map(|(index, description)| {
            let code = codes.iter().find(|(i, _)| *i == index)?.1.clone();
            Some(Cheat {
                index: index as i64,
                description,
                code,
                enabled: enabled.contains(&index),
            })
        })
        .collect()
}

/// Look for a cheat file for this ROM.
pub async fn fetch(
    client: &reqwest::Client,
    platform: &str,
    rom_name: &str,
) -> Result<Vec<Cheat>> {
    let Some(system) = libretro::system_for(platform) else {
        return Err(AppError::Other(format!(
            "No cheat database for {platform}"
        )));
    };

    for candidate in candidate_names(rom_name) {
        let url = format!(
            "{RAW}/{}/{}",
            urlencoding::encode(system),
            urlencoding::encode(&candidate)
        );
        let Ok(resp) = client.get(&url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(body) = resp.text().await else { continue };
        let cheats = parse(&body);
        if !cheats.is_empty() {
            return Ok(cheats);
        }
    }

    Ok(Vec::new())
}

/// Where RetroArch looks for cheat files.
pub fn cheat_dir(retroarch_path: &Path, configured: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = configured.filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let parent = retroarch_path.parent()?;
    Some(parent.join("cheats"))
}

/// Render cheats in RetroArch's **native** game-cheat format.
///
/// The short three-field form is what the online cheat *database* ships, and
/// RetroArch will load it by hand — but it will not auto-apply it. Its own
/// saved files carry twenty fields per cheat with every value quoted, and that
/// is what "Auto-Apply Cheats During Game Load" reads. The extra fields are
/// emulator-handler defaults, taken from a file RetroArch wrote itself.
///
/// Only the switched-on cheats are written, re-indexed from zero. Writing the
/// whole database instead produced a 456 KB file of 711 cheats for one ticked
/// box. Switching everything off yields `cheats = "0"`, which is what clears
/// them — not an absent file, which RetroArch would simply ignore.
pub fn render(cheats: &[Cheat]) -> String {
    let cheats: Vec<Cheat> = cheats.iter().filter(|c| c.enabled).cloned().collect();
    let cheats = &cheats[..];
    /// Field name and its default, in the alphabetical order RetroArch uses.
    const DEFAULTS: &[(&str, &str)] = &[
        ("address", "0"),
        ("address_bit_position", "0"),
        ("big_endian", "false"),
        ("cheat_type", "1"),
        ("handler", "0"),
        ("memory_search_size", "3"),
        ("repeat_add_to_address", "1"),
        ("repeat_add_to_value", "0"),
        ("repeat_count", "1"),
        ("rumble_port", "0"),
        ("rumble_primary_duration", "0"),
        ("rumble_primary_strength", "0"),
        ("rumble_secondary_duration", "0"),
        ("rumble_secondary_strength", "0"),
        ("rumble_type", "0"),
        ("rumble_value", "0"),
        ("value", "0"),
    ];

    let mut out = String::new();
    for (i, cheat) in cheats.iter().enumerate() {
        for (field, default) in DEFAULTS {
            // code, desc and enable slot into the alphabetical run.
            if *field == "handler" {
                out.push_str(&format!("cheat{i}_code = \"{}\"\n", escape(&cheat.code)));
                out.push_str(&format!(
                    "cheat{i}_desc = \"{}\"\n",
                    escape(&cheat.description)
                ));
                out.push_str(&format!(
                    "cheat{i}_enable = \"{}\"\n",
                    if cheat.enabled { "true" } else { "false" }
                ));
            }
            out.push_str(&format!("cheat{i}_{field} = \"{default}\"\n"));
        }
    }
    out.push_str(&format!("cheats = \"{}\"\n", cheats.len()));
    out
}

/// Quotes would end the value early and corrupt every following key.
fn escape(value: &str) -> String {
    value.replace('"', "'")
}

/// Loose comparison key: lowercase, letters and digits only. "Super Mario
/// Bros. (World)" and "Super Mario Bros" collapse to the same neighbourhood
/// once region tags are gone.
fn loose(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Write the cheats everywhere RetroArch might look for them.
///
/// RetroArch names a game's cheat file after the content *it* resolved, which
/// is not always the ROM's filename — a dump called
/// `Super Mario Bros. (World).nes` gets `Super Mario Bros.cht`. Writing only
/// the filename leaves RetroArch loading a stale file it made earlier, so both
/// the raw stem and the cleaned title are written, and any existing file in
/// the same folder for the same game is refreshed too.
pub fn write_file(
    dir: &Path,
    core_folder: &str,
    rom_name: &str,
    display_title: &str,
    cheats: &[Cheat],
) -> Result<Vec<PathBuf>> {
    let target = dir.join(core_folder);
    std::fs::create_dir_all(&target)?;
    let body = render(cheats);

    let mut names: Vec<String> = Vec::new();
    for candidate in [rom_name, display_title] {
        let clean = sanitize(candidate.trim());
        if !clean.is_empty() && !names.iter().any(|n| n == &clean) {
            names.push(clean);
        }
    }

    // Refresh anything RetroArch already wrote for this game under a name of
    // its own choosing.
    let keys: Vec<String> = names.iter().map(|n| loose(n)).collect();
    if let Ok(entries) = std::fs::read_dir(&target) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("cht") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // The same name give or take punctuation and case, and nothing
            // looser. A prefix was once enough, which made Super Mario Bros. 3
            // overwrite Super Mario Bros.'s codes on every launch, and the
            // other way round.
            let stem_key = loose(stem);
            let related = keys.iter().any(|k| stem_key == *k);
            if related && !names.iter().any(|n| n == stem) {
                names.push(stem.to_string());
            }
        }
    }

    let mut written = Vec::new();
    for name in names {
        let path = target.join(format!("{name}.cht"));
        std::fs::write(&path, &body)?;
        written.push(path);
    }
    Ok(written)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"cheats = 2

cheat0_desc = "10 Power Points"
cheat0_code = "ZESNLLLE"
cheat0_enable = false

cheat1_desc = "Mostly Untouchable"
cheat1_code = "OZOYUEPX+OZVULSPX"
cheat1_enable = false
"#;

    #[test]
    fn parses_the_retroarch_format() {
        let cheats = parse(SAMPLE);
        assert_eq!(cheats.len(), 2);
        assert_eq!(cheats[0].description, "10 Power Points");
        assert_eq!(cheats[0].code, "ZESNLLLE");
        // Multi-part Game Genie codes are joined with a plus.
        assert_eq!(cheats[1].code, "OZOYUEPX+OZVULSPX");
        assert!(!cheats[0].enabled);
    }

    #[test]
    fn ignores_junk_and_incomplete_entries() {
        let messy = "cheats = 3\n\
                     nonsense line\n\
                     cheat0_desc = \"Has no code\"\n\
                     cheat1_code = \"ABCD\"\n\
                     cheat2_desc = \"Complete\"\n\
                     cheat2_code = \"WXYZ\"\n";
        let cheats = parse(messy);
        assert_eq!(cheats.len(), 1, "only the complete pair survives");
        assert_eq!(cheats[0].description, "Complete");
    }

    /// Only switched-on cheats are written, re-indexed from zero — writing all
    /// 711 of Super Mario Bros's produced a 456 KB file for one ticked box.
    #[test]
    fn writes_only_what_is_switched_on() {
        let cheats = vec![
            Cheat { index: 0, description: "On".into(), code: "AAAA".into(), enabled: true },
            Cheat { index: 1, description: "Off".into(), code: "BBBB".into(), enabled: false },
            Cheat { index: 2, description: "Also on".into(), code: "CCCC".into(), enabled: true },
        ];
        let out = render(&cheats);
        assert!(out.contains(r#"cheats = "2""#));
        assert!(out.contains(r#"cheat0_code = "AAAA""#));
        // Re-indexed contiguously; RetroArch stops at the first gap.
        assert!(out.contains(r#"cheat1_code = "CCCC""#));
        assert!(!out.contains("BBBB"));
    }

    /// Clearing every box must still write a file. An absent one leaves
    /// RetroArch loading whatever it had before.
    #[test]
    fn switching_everything_off_writes_an_empty_list() {
        let out = render(&[Cheat {
            index: 0,
            description: "Off".into(),
            code: "AAAA".into(),
            enabled: false,
        }]);
        assert!(out.contains(r#"cheats = "0""#));
        assert!(!out.contains("AAAA"));
    }

    /// RetroArch names the file after the content it resolved, which drops the
    /// region tag — so both names have to be written.
    #[test]
    fn writes_under_every_name_retroarch_might_use() {
        let dir = std::env::temp_dir().join(format!("arcadex-cht-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let core = dir.join("Mesen");
        std::fs::create_dir_all(&core).unwrap();
        // A file RetroArch wrote earlier under its own name for this game.
        std::fs::write(core.join("Super Mario Bros.cht"), "cheats = \"4\"\n").unwrap();

        let cheats = [Cheat {
            index: 0,
            description: "Totally Invincible".into(),
            code: "079E:07".into(),
            enabled: true,
        }];
        let written = write_file(
            &dir,
            "Mesen",
            "Super Mario Bros. (World)",
            "Super Mario Bros",
            &cheats,
        )
        .unwrap();

        let names: Vec<String> = written
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"Super Mario Bros. (World).cht".to_string()));
        assert!(names.contains(&"Super Mario Bros.cht".to_string()));

        // The stale file RetroArch was loading is now refreshed, not ignored.
        let refreshed =
            std::fs::read_to_string(core.join("Super Mario Bros.cht")).unwrap();
        assert!(refreshed.contains(r#"cheats = "1""#));
        assert!(refreshed.contains("079E:07"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sequel's cheats are not written over the original's, nor the other
    /// way round, though one name begins with the other.
    #[test]
    fn leaves_another_games_cheat_file_alone() {
        let dir = std::env::temp_dir().join(format!("arcadex-cht-other-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let core = dir.join("Mesen");
        std::fs::create_dir_all(&core).unwrap();
        std::fs::write(core.join("Super Mario Bros.cht"), "smb1").unwrap();
        std::fs::write(core.join("Super Mario Bros. 3 (USA) (Rev 1).cht"), "smb3 rev1").unwrap();

        let cheats = [Cheat {
            index: 0,
            description: "Start with 99 lives".into(),
            code: "0736:63".into(),
            enabled: true,
        }];
        write_file(&dir, "Mesen", "Super Mario Bros. 3 (USA)", "Super Mario Bros 3", &cheats).unwrap();

        assert_eq!(std::fs::read_to_string(core.join("Super Mario Bros.cht")).unwrap(), "smb1");
        assert_eq!(
            std::fs::read_to_string(core.join("Super Mario Bros. 3 (USA) (Rev 1).cht")).unwrap(),
            "smb3 rev1",
            "a different revision is a different dump"
        );
        assert!(core.join("Super Mario Bros 3.cht").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RetroArch's own files carry twenty fields per cheat; anything less is
    /// the database format, which auto-apply ignores.
    #[test]
    fn writes_retroarchs_native_field_set() {
        let out = render(&[Cheat {
            index: 0,
            description: "Totally Invincible".into(),
            code: "079E:07".into(),
            enabled: true,
        }]);
        for field in [
            "address", "address_bit_position", "big_endian", "cheat_type", "code",
            "desc", "enable", "handler", "memory_search_size", "repeat_add_to_address",
            "repeat_add_to_value", "repeat_count", "rumble_port",
            "rumble_primary_duration", "rumble_primary_strength",
            "rumble_secondary_duration", "rumble_secondary_strength", "rumble_type",
            "rumble_value", "value",
        ] {
            assert!(
                out.contains(&format!("cheat0_{field} = \"")),
                "missing cheat0_{field}"
            );
        }
        assert_eq!(out.matches("cheat0_").count(), 20);
    }

    #[test]
    fn round_trips_through_the_native_format() {
        let parsed = parse(SAMPLE);
        let enabled: Vec<Cheat> = parsed
            .into_iter()
            .map(|mut c| { c.enabled = true; c })
            .collect();
        let reparsed = parse(&render(&enabled));
        assert_eq!(reparsed.len(), 2);
        assert_eq!(reparsed[1].code, "OZOYUEPX+OZVULSPX");
        // Enable state survives the round trip.
        assert!(reparsed.iter().all(|c| c.enabled));
    }

    #[test]
    fn quotes_in_a_description_cannot_break_the_file() {
        let out = render(&[Cheat {
            index: 0,
            description: r#"Say "hi""#.into(),
            code: "AAAA".into(),
            enabled: true,
        }]);
        assert!(!out.contains(r#""Say "hi"""#));
        assert_eq!(parse(&out).len(), 1);
    }
}

// ------------------------------------------------ RetroArch's own layout

/// RetroArch files cheats under the **core's display name**, not the system's
/// — `cheats/Nestopia/…`, not `cheats/Nintendo - Nintendo Entertainment
/// System/…`. The display name is what the core reports about itself, so it
/// does not follow from the library filename and has to be mapped.
pub fn core_folder_name(core_file: &str) -> String {
    let key = core_file.trim_end_matches("_libretro");
    let name = match key {
        "mesen" => "Mesen",
        "nestopia" => "Nestopia",
        "fceumm" => "FCEUmm",
        "snes9x" => "Snes9x",
        "bsnes" => "bsnes",
        "mesen-s" => "Mesen-S",
        "mupen64plus_next" => "Mupen64Plus-Next",
        "parallel_n64" => "ParaLLEl N64",
        "gambatte" => "Gambatte",
        "sameboy" => "SameBoy",
        "mgba" => "mGBA",
        "vbam" => "VBA-M",
        "vba_next" => "VBA Next",
        "melonds" => "melonDS",
        "desmume" => "DeSmuME",
        "citra" => "Citra",
        "dolphin" => "dolphin-emu",
        "genesis_plus_gx" => "Genesis Plus GX",
        "picodrive" => "PicoDrive",
        "flycast" => "Flycast",
        "mednafen_saturn" => "Beetle Saturn",
        "kronos" => "Kronos",
        "swanstation" => "SwanStation",
        "beetle_psx_hw" => "Beetle PSX HW",
        "pcsx_rearmed" => "PCSX-ReARMed",
        "pcsx2" => "LRPS2",
        "ppsspp" => "PPSSPP",
        "stella" => "Stella",
        "prosystem" => "ProSystem",
        "handy" => "Handy",
        "mednafen_lynx" => "Beetle Lynx",
        "virtualjaguar" => "Virtual Jaguar",
        "mednafen_pce" => "Beetle PCE Fast",
        "mednafen_supergrafx" => "Beetle SuperGrafx",
        "fbneo" => "FinalBurn Neo",
        "mame" => "MAME",
        "mednafen_ngp" => "Beetle NeoPop",
        "mednafen_wswan" => "Beetle WonderSwan",
        "mednafen_vb" => "Beetle VB",
        "opera" => "Opera",
        "bluemsx" => "blueMSX",
        "fmsx" => "fMSX",
        "vice_x64" => "VICE x64",
        "puae" => "PUAE",
        "gearcoleco" => "Gearcoleco",
        "freeintv" => "FreeIntv",
        "dosbox_pure" => "DOSBox-pure",
        "scummvm" => "ScummVM",
        other => other,
    };
    name.to_string()
}

/// What RetroArch's own config says about cheats.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetroArchCheats {
    pub config_path: Option<String>,
    pub cheat_dir: Option<String>,
    /// RetroArch will only apply a cheat file on its own when this is true.
    pub auto_apply: bool,
}

/// Where RetroArch keeps `retroarch.cfg`, in the order worth trying.
///
/// Beside the executable first, which is what a portable unzip looks like,
/// then the per-user location for each platform. These mirror the core
/// directories in `detect.rs`: the two are the same question asked about
/// different files, so they should not be allowed to drift apart.
///
/// Every platform's paths are listed rather than compiled in per target. They
/// cost a `is_file` each, none of them can exist on the wrong system, and it
/// means one test covers the lot wherever it runs.
///
/// Pure so it can be tested without reaching into the real environment.
fn config_candidates(exe: &Path, home: Option<&Path>, appdata: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(dir) = exe.parent() {
        out.push(dir.join("retroarch.cfg"));
        // A macOS bundle keeps it beside the binary, inside Contents.
        out.push(dir.join("../Resources/retroarch.cfg"));
    }
    if let Some(appdata) = appdata {
        out.push(appdata.join("RetroArch").join("retroarch.cfg"));
    }
    if let Some(home) = home {
        out.push(home.join("Library/Application Support/RetroArch/retroarch.cfg"));
        out.push(home.join(".config/retroarch/retroarch.cfg"));
        out.push(home.join(".var/app/org.libretro.RetroArch/config/retroarch/retroarch.cfg"));
    }
    out
}

fn config_path_for(exe: &Path) -> Option<PathBuf> {
    // APPDATA is Windows-only and HOME is not set on Windows, so each platform
    // contributes only the paths that mean anything on it.
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    config_candidates(exe, home.as_deref(), appdata.as_deref())
        .into_iter()
        .find(|p| p.is_file())
}

/// Read one setting out of RetroArch's config, expanding the ":" prefix it
/// uses to mean "relative to my own folder". Shared with save handling.
pub fn config_dir(exe: &Path, key: &str) -> Option<PathBuf> {
    let config_path = config_path_for(exe)?;
    let text = std::fs::read_to_string(&config_path).ok()?;
    let raw = config_value(&text, key)?;
    if raw.is_empty() || raw == "default" {
        return None;
    }
    let base = exe.parent()?;
    Some(if raw.starts_with(':') {
        base.join(raw.trim_start_matches(':').trim_start_matches(['\\', '/']))
    } else {
        PathBuf::from(raw)
    })
}

fn config_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        let Some((k, v)) = line.split_once('=') else { continue };
        if k.trim() == key {
            return Some(v.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// Read RetroArch's config. Paths there may start with ":" meaning "relative
/// to RetroArch's own folder", which has to be expanded or nothing resolves.
pub fn read_retroarch_config(exe: &Path) -> RetroArchCheats {
    let Some(config_path) = config_path_for(exe) else {
        return RetroArchCheats {
            config_path: None,
            cheat_dir: None,
            auto_apply: false,
        };
    };

    let text = std::fs::read_to_string(&config_path).unwrap_or_default();
    let base = exe.parent().map(|p| p.to_path_buf());

    let cheat_dir = config_value(&text, "cheat_database_path")
        .filter(|v| !v.is_empty())
        .map(|v| {
            let trimmed = v.trim_start_matches(':').trim_start_matches(['\\', '/']);
            match (&base, v.starts_with(':')) {
                (Some(b), true) => b.join(trimmed),
                _ => PathBuf::from(v),
            }
        })
        .or_else(|| base.map(|b| b.join("cheats")));

    RetroArchCheats {
        auto_apply: config_value(&text, "apply_cheats_after_load")
            .map(|v| v == "true")
            .unwrap_or(false),
        config_path: Some(config_path.to_string_lossy().to_string()),
        cheat_dir: cheat_dir.map(|p| p.to_string_lossy().to_string()),
    }
}

/// Flip `apply_cheats_after_load` on. RetroArch rewrites its config when it
/// exits, so this is only safe while it is closed — the caller checks.
/// Write a game's switched-on cheats where RetroArch will look for them, and
/// make sure RetroArch is actually set to apply them.
///
/// RetroArch files cheats by the *core's* display name, not the system's, and
/// it reads its cheat folder from its own config — so both come from there
/// rather than from a guess.
///
/// Returns `None` when there is nothing to do, so a launch can call this
/// unconditionally without having to know whether cheats exist.
pub fn sync_to_retroarch(conn: &rusqlite::Connection, game: &Game) -> Result<Option<String>> {
    let cheats = crate::db::list_cheats(conn, game.id)?;
    if cheats.is_empty() {
        return Ok(None);
    }
    // Switching everything off must still write the file. Stopping here would
    // leave the previous, still-enabled file in place and the cheats would
    // stay on in game.

    let retroarch = crate::db::get_setting(conn, "retroarch_path")?
        .map(|p| crate::detect::clean_path(&p))
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::Other("Set the RetroArch path in Settings first".into()))?;
    let exe = PathBuf::from(&retroarch);

    // Cheats are filed under the core that runs the game, so this has to be
    // the core that will actually run it - the game's own where it has one.
    let core = crate::launch::resolve_for_game(conn, game)
        .core
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            AppError::Other(format!(
                "No libretro core set for {} — cheats are filed per core",
                crate::platforms::display_name(&game.platform)
            ))
        })?;

    let config = read_retroarch_config(&exe);
    let dir = config
        .cheat_dir
        .clone()
        .map(PathBuf::from)
        .or_else(|| cheat_dir(&exe, None))
        .ok_or_else(|| AppError::Other("Could not work out RetroArch's cheat folder".into()))?;

    let folder = core_folder_name(&core);
    let written = write_file(&dir, &folder, &rom_name(game), &game.title, &cheats)?;

    let on = cheats.iter().filter(|c| c.enabled).count();

    // A cheat file RetroArch is not set to read is just a file. Turning the
    // setting on is only worth doing when there is something to apply, and
    // only at a moment RetroArch is not running — it rewrites its config on
    // exit and would undo us.
    let mut auto = String::new();
    if on > 0 && !config.auto_apply {
        match enable_auto_apply(&exe) {
            Ok(_) => auto = " and turned on RetroArch's auto-apply".to_string(),
            Err(e) => auto = format!(" (could not turn on auto-apply: {e})"),
        }
    }

    let files = written
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let where_to = written
        .first()
        .and_then(|p| p.parent())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(Some(if on == 0 {
        format!("All cheats cleared - wrote {files} in {where_to}")
    } else {
        format!(
            "{on} cheat{} active - wrote {files} in {where_to}{auto}",
            if on == 1 { "" } else { "s" }
        )
    }))
}

/// The ROM's own name, which is how the cheat database indexes it - not the
/// cleaned-up display title.
fn rom_name(game: &Game) -> String {
    let raw = game.inner_name.as_deref().unwrap_or(&game.filename);
    Path::new(raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(raw)
        .to_string()
}

pub fn enable_auto_apply(exe: &Path) -> Result<String> {
    let path = config_path_for(exe)
        .ok_or_else(|| AppError::Other("Could not find retroarch.cfg".into()))?;
    let text = std::fs::read_to_string(&path)?;

    let updated = if text.contains("apply_cheats_after_load") {
        text.lines()
            .map(|line| {
                if line.trim_start().starts_with("apply_cheats_after_load") {
                    "apply_cheats_after_load = \"true\"".to_string()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        format!("{}\napply_cheats_after_load = \"true\"\n", text.trim_end())
    };

    std::fs::write(&path, updated)?;
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod retroarch_tests {
    use super::*;

    /// The whole launch-time path: a game with cheats switched on gets a file
    /// written into the right core folder, and RetroArch is switched over to
    /// applying it — neither of which the user has to ask for.
    #[test]
    fn playing_a_game_writes_its_cheats_and_arms_retroarch() {
        let dir = std::env::temp_dir().join(format!(
            "arcadex-sync-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        // A RetroArch install with auto-apply switched off.
        let exe = dir.join("retroarch.exe");
        std::fs::write(&exe, b"stub").unwrap();
        std::fs::write(
            dir.join("retroarch.cfg"),
            "apply_cheats_after_load = \"false\"
cheat_database_path = \":\\cheats\"
",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("cheats")).unwrap();

        let conn = crate::db::open(&dir.join("library.db")).unwrap();
        crate::db::set_setting(&conn, "retroarch_path", &exe.to_string_lossy()).unwrap();
        conn.execute(
            "INSERT INTO games (id, path, filename, platform, title, size, added_at)
             VALUES (1, ?1, 'Super Mario Bros. (World).nes', 'nes', 'Super Mario Bros.', 40976, 0)",
            rusqlite::params![dir.join("Super Mario Bros. (World).nes").to_string_lossy()],
        )
        .unwrap();

        crate::db::replace_cheats(
            &conn,
            1,
            &[
                Cheat { index: 0, description: "Infinite lives".into(), code: "SXIOPO".into(), enabled: false },
                Cheat { index: 1, description: "Start on level 5".into(), code: "AATOZA".into(), enabled: false },
            ],
        )
        .unwrap();
        crate::db::set_cheat_enabled(&conn, 1, 0, true).unwrap();

        let game = crate::db::get_game(&conn, 1).unwrap().unwrap();
        let note = sync_to_retroarch(&conn, &game)
            .expect("sync should succeed")
            .expect("a game with cheats has something to write");

        assert!(note.contains("1 cheat active"), "note was: {note}");
        assert!(note.contains("auto-apply"), "note was: {note}");

        // Filed under the *core's* display name, not the system's: NES
        // defaults to mesen_libretro, which RetroArch calls "Mesen".
        let written = dir.join("cheats").join("Mesen");
        let files: Vec<String> = std::fs::read_dir(&written)
            .expect("core folder should exist")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(!files.is_empty(), "no cheat file written");

        let body = std::fs::read_to_string(written.join(&files[0])).unwrap();
        assert!(body.contains("SXIOPO"), "the enabled cheat is missing");

        // ...and RetroArch will now actually read it.
        assert!(read_retroarch_config(&exe).auto_apply);

        // A game with no cheats recorded is simply nothing to do, so a launch
        // can call this without checking first.
        conn.execute(
            "INSERT INTO games (id, path, filename, platform, title, size, added_at)
             VALUES (2, 'x.nes', 'x.nes', 'nes', 'No Cheats', 1024, 0)",
            [],
        )
        .unwrap();
        let bare = crate::db::get_game(&conn, 2).unwrap().unwrap();
        assert!(sync_to_retroarch(&conn, &bare).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn maps_cores_to_the_folders_retroarch_uses() {
        // Verified against a real RetroArch install's cheats directory.
        assert_eq!(core_folder_name("nestopia_libretro"), "Nestopia");
        assert_eq!(core_folder_name("mesen_libretro"), "Mesen");
        assert_eq!(core_folder_name("mupen64plus_next_libretro"), "Mupen64Plus-Next");
        assert_eq!(core_folder_name("desmume_libretro"), "DeSmuME");
        assert_eq!(core_folder_name("mednafen_pce_libretro"), "Beetle PCE Fast");
        assert_eq!(core_folder_name("dolphin_libretro"), "dolphin-emu");
        assert_eq!(core_folder_name("dosbox_pure_libretro"), "DOSBox-pure");
        // Unknown cores fall through to their bare name rather than failing.
        assert_eq!(core_folder_name("something_new_libretro"), "something_new");
    }

    #[test]
    fn expands_retroarchs_colon_relative_paths() {
        let dir = std::env::temp_dir().join(format!("arcadex-racfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("retroarch.exe");
        std::fs::write(&exe, b"").unwrap();
        // ":\cheats" means "the cheats folder inside RetroArch's own folder".
        let cfg_text = concat!(
            "apply_cheats_after_load = \"false\"\n",
            r#"cheat_database_path = ":\cheats""#,
            "\n"
        );
        std::fs::write(dir.join("retroarch.cfg"), cfg_text).unwrap();

        let cfg = read_retroarch_config(&exe);
        assert!(!cfg.auto_apply);
        let cheat_dir = cfg.cheat_dir.unwrap();
        assert!(cheat_dir.ends_with("cheats"), "got {cheat_dir}");
        assert!(cheat_dir.starts_with(dir.to_str().unwrap()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn turning_auto_apply_on_rewrites_only_that_line() {
        let dir = std::env::temp_dir().join(format!("arcadex-raflip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("retroarch.exe");
        std::fs::write(&exe, b"").unwrap();
        let cfg = dir.join("retroarch.cfg");
        std::fs::write(&cfg, "video_fullscreen = \"true\"\napply_cheats_after_load = \"false\"\naudio_enable = \"true\"\n").unwrap();

        enable_auto_apply(&exe).unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("apply_cheats_after_load = \"true\""));
        assert!(text.contains("video_fullscreen = \"true\""));
        assert!(text.contains("audio_enable = \"true\""));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod config_location_tests {
    use super::config_candidates;
    use std::path::{Path, PathBuf};

    fn candidates(exe: &str, home: Option<&str>, appdata: Option<&str>) -> Vec<String> {
        config_candidates(
            Path::new(exe),
            home.map(Path::new),
            appdata.map(Path::new),
        )
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect()
    }

    #[test]
    fn a_portable_install_is_looked_at_first() {
        let got = candidates("/opt/RetroArch/retroarch", Some("/home/me"), None);
        assert_eq!(got[0], "/opt/RetroArch/retroarch.cfg");
    }

    #[test]
    fn windows_gets_its_appdata_path() {
        let got = candidates(
            "C:/Program Files/RetroArch/retroarch.exe",
            None,
            Some("C:/Users/me/AppData/Roaming"),
        );
        assert!(
            got.iter()
                .any(|p| p == "C:/Users/me/AppData/Roaming/RetroArch/retroarch.cfg"),
            "{got:?}"
        );
    }

    /// The reason this function exists. It used to read APPDATA and nothing
    /// else, so on Linux and macOS it found no config at all and cheats and
    /// save handling both quietly did nothing.
    #[test]
    fn linux_and_mac_get_their_own_paths() {
        let got = candidates("/usr/bin/retroarch", Some("/home/me"), None);
        for expected in [
            "/home/me/Library/Application Support/RetroArch/retroarch.cfg",
            "/home/me/.config/retroarch/retroarch.cfg",
            "/home/me/.var/app/org.libretro.RetroArch/config/retroarch/retroarch.cfg",
        ] {
            assert!(got.iter().any(|p| p == expected), "missing {expected}: {got:?}");
        }
    }

    #[test]
    fn a_mac_bundle_looks_beside_the_binary_in_resources() {
        let got = candidates(
            "/Applications/RetroArch.app/Contents/MacOS/RetroArch",
            Some("/Users/me"),
            None,
        );
        assert!(
            got.iter()
                .any(|p| p == "/Applications/RetroArch.app/Contents/MacOS/../Resources/retroarch.cfg"),
            "{got:?}"
        );
    }

    #[test]
    fn nothing_is_offered_when_the_environment_is_empty() {
        let got = config_candidates(Path::new("retroarch"), None, None);
        // Only the two beside a bare executable, and with no parent directory
        // to speak of neither can point anywhere surprising.
        assert!(got.len() <= 2, "{got:?}");
        assert!(!got.iter().any(|p: &PathBuf| p.to_string_lossy().contains("RetroArch/retroarch.cfg")));
    }
}
