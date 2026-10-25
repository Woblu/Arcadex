//! Static platform table: file extensions, directory-name aliases used to
//! disambiguate shared extensions, and preferred libretro cores.

pub struct PlatformDef {
    pub slug: &'static str,
    pub name: &'static str,
    pub exts: &'static [&'static str],
    /// Lowercase tokens matched against parent directory names and against
    /// provider platform names.
    pub aliases: &'static [&'static str],
    /// Preferred libretro cores, best first (filename without extension).
    pub cores: &'static [&'static str],
}

pub const PLATFORMS: &[PlatformDef] = &[
    PlatformDef {
        slug: "nes",
        name: "Nintendo Entertainment System",
        exts: &["nes", "unf", "unif", "fds"],
        aliases: &["nes", "famicom", "nintendo entertainment system", "fds"],
        cores: &["mesen_libretro", "nestopia_libretro", "fceumm_libretro"],
    },
    PlatformDef {
        slug: "snes",
        name: "Super Nintendo",
        exts: &["sfc", "smc", "swc", "fig", "bs"],
        aliases: &["snes", "super nintendo", "super famicom", "sfc"],
        cores: &["snes9x_libretro", "bsnes_libretro", "mesen-s_libretro"],
    },
    PlatformDef {
        slug: "n64",
        name: "Nintendo 64",
        exts: &["n64", "z64", "v64", "ndd"],
        aliases: &["n64", "nintendo 64"],
        cores: &["mupen64plus_next_libretro", "parallel_n64_libretro"],
    },
    PlatformDef {
        slug: "gamecube",
        name: "Nintendo GameCube",
        exts: &["gcm", "gcz", "rvz", "wia"],
        aliases: &["gamecube", "ngc", "gcn"],
        cores: &["dolphin_libretro"],
    },
    PlatformDef {
        slug: "wii",
        name: "Nintendo Wii",
        // Dolphin's own container formats hold either a GameCube or a Wii
        // disc, so gcz/rvz/wia are claimed by both systems and land in the
        // ambiguous path rather than settling on GameCube by accident.
        exts: &["wbfs", "wad", "gcz", "rvz", "wia"],
        aliases: &["wii"],
        cores: &["dolphin_libretro"],
    },
    PlatformDef {
        slug: "wiiu",
        name: "Nintendo Wii U",
        // A loose dump is a folder of code, content and meta, which
        // `consoles` recognises whole; these are the single-file forms.
        exts: &["wua", "wud", "wux", "rpx"],
        aliases: &["wii u", "wiiu"],
        cores: &[],
    },
    PlatformDef {
        slug: "switch",
        name: "Nintendo Switch",
        // nsz and xcz are the compressed forms of nsp and xci.
        exts: &["nsp", "xci", "nro", "nsz", "xcz"],
        aliases: &["switch", "nintendo switch", "nsw"],
        // No libretro core exists for the Switch. Point this system at a
        // standalone emulator under Settings -> Emulators; an empty core list
        // is what makes launching say so rather than blaming your setup.
        cores: &[],
    },
    PlatformDef {
        slug: "gb",
        name: "Game Boy",
        exts: &["gb"],
        aliases: &["gb", "game boy", "gameboy"],
        cores: &["sameboy_libretro", "gambatte_libretro"],
    },
    PlatformDef {
        slug: "gbc",
        name: "Game Boy Color",
        exts: &["gbc"],
        aliases: &["gbc", "game boy color", "gameboy color"],
        cores: &["sameboy_libretro", "gambatte_libretro"],
    },
    PlatformDef {
        slug: "gba",
        name: "Game Boy Advance",
        exts: &["gba"],
        aliases: &["gba", "game boy advance", "gameboy advance"],
        cores: &["mgba_libretro", "vbam_libretro"],
    },
    PlatformDef {
        slug: "nds",
        name: "Nintendo DS",
        exts: &["nds", "dsi"],
        aliases: &["nds", "nintendo ds"],
        cores: &["melonds_libretro", "desmume_libretro"],
    },
    PlatformDef {
        slug: "n3ds",
        name: "Nintendo 3DS",
        exts: &["3ds", "cci", "cxi", "cia"],
        aliases: &["3ds", "nintendo 3ds"],
        cores: &["citra_libretro"],
    },
    PlatformDef {
        slug: "virtualboy",
        name: "Virtual Boy",
        exts: &["vb", "vboy"],
        aliases: &["virtual boy", "virtualboy"],
        cores: &["mednafen_vb_libretro"],
    },
    PlatformDef {
        slug: "genesis",
        name: "Sega Genesis / Mega Drive",
        exts: &["md", "gen", "smd"],
        aliases: &["genesis", "mega drive", "megadrive", "sega genesis"],
        cores: &["genesis_plus_gx_libretro", "picodrive_libretro"],
    },
    PlatformDef {
        slug: "sms",
        name: "Sega Master System",
        exts: &["sms"],
        aliases: &["master system", "sms", "sega master system"],
        cores: &["genesis_plus_gx_libretro", "picodrive_libretro"],
    },
    PlatformDef {
        slug: "gamegear",
        name: "Sega Game Gear",
        exts: &["gg"],
        aliases: &["game gear", "gamegear"],
        cores: &["genesis_plus_gx_libretro"],
    },
    PlatformDef {
        slug: "sega32x",
        name: "Sega 32X",
        exts: &["32x"],
        aliases: &["32x", "sega 32x"],
        cores: &["picodrive_libretro"],
    },
    PlatformDef {
        slug: "segacd",
        name: "Sega CD / Mega CD",
        exts: &[],
        aliases: &["sega cd", "segacd", "mega cd", "megacd"],
        cores: &["genesis_plus_gx_libretro", "picodrive_libretro"],
    },
    PlatformDef {
        slug: "saturn",
        name: "Sega Saturn",
        exts: &[],
        aliases: &["saturn", "sega saturn"],
        cores: &["mednafen_saturn_libretro", "kronos_libretro"],
    },
    PlatformDef {
        slug: "dreamcast",
        name: "Sega Dreamcast",
        exts: &["gdi", "cdi"],
        aliases: &["dreamcast", "sega dreamcast"],
        cores: &["flycast_libretro"],
    },
    PlatformDef {
        slug: "ps1",
        name: "PlayStation",
        exts: &["pbp", "ecm"],
        aliases: &["ps1", "psx", "playstation", "psone"],
        cores: &[
            "swanstation_libretro",
            "beetle_psx_hw_libretro",
            "pcsx_rearmed_libretro",
        ],
    },
    PlatformDef {
        slug: "ps2",
        name: "PlayStation 2",
        exts: &["cso"],
        aliases: &["ps2", "playstation 2"],
        cores: &["pcsx2_libretro"],
    },
    PlatformDef {
        slug: "psp",
        name: "PlayStation Portable",
        exts: &[],
        aliases: &["psp", "playstation portable"],
        cores: &["ppsspp_libretro"],
    },
    // The three below have no extension of their own. A disc is an `.iso`
    // told apart by its filesystem, and everything else is a folder that
    // `consoles` recognises by the files the console itself put there.
    PlatformDef {
        slug: "ps3",
        name: "PlayStation 3",
        exts: &[],
        aliases: &["ps3", "playstation 3"],
        cores: &[],
    },
    PlatformDef {
        slug: "ps4",
        name: "PlayStation 4",
        exts: &[],
        aliases: &["ps4", "playstation 4"],
        cores: &[],
    },
    PlatformDef {
        slug: "ps5",
        name: "PlayStation 5",
        exts: &[],
        aliases: &["ps5", "playstation 5"],
        cores: &[],
    },
    PlatformDef {
        slug: "xbox",
        name: "Xbox",
        // Discs only: xemu boots an image, not an extracted folder.
        exts: &[],
        aliases: &["xbox", "original xbox"],
        cores: &[],
    },
    PlatformDef {
        slug: "xbox360",
        name: "Xbox 360",
        // `default.xex` is what an extracted disc boots from, and Xenia reads
        // `.zar` archives as well. Discs and Games on Demand packages are
        // recognised by their contents.
        exts: &["xex", "zar"],
        aliases: &["xbox 360", "xbox360", "x360"],
        cores: &[],
    },
    PlatformDef {
        slug: "atari2600",
        name: "Atari 2600",
        exts: &["a26"],
        aliases: &["atari 2600", "atari2600", "2600"],
        cores: &["stella_libretro"],
    },
    PlatformDef {
        slug: "atari7800",
        name: "Atari 7800",
        exts: &["a78"],
        aliases: &["atari 7800", "atari7800", "7800"],
        cores: &["prosystem_libretro"],
    },
    PlatformDef {
        slug: "lynx",
        name: "Atari Lynx",
        exts: &["lnx"],
        aliases: &["lynx", "atari lynx"],
        cores: &["handy_libretro", "mednafen_lynx_libretro"],
    },
    PlatformDef {
        slug: "jaguar",
        name: "Atari Jaguar",
        exts: &["jag", "j64"],
        aliases: &["jaguar", "atari jaguar"],
        cores: &["virtualjaguar_libretro"],
    },
    PlatformDef {
        slug: "pcengine",
        name: "PC Engine / TurboGrafx-16",
        exts: &["pce", "sgx"],
        aliases: &["pc engine", "pcengine", "turbografx", "tg16"],
        cores: &["mednafen_pce_libretro", "mednafen_supergrafx_libretro"],
    },
    PlatformDef {
        slug: "neogeo",
        name: "Neo Geo",
        exts: &["neo"],
        aliases: &["neo geo", "neogeo"],
        cores: &["fbneo_libretro", "mame_libretro"],
    },
    PlatformDef {
        slug: "ngp",
        name: "Neo Geo Pocket",
        exts: &["ngp", "ngpc"],
        aliases: &["neo geo pocket", "ngp", "neogeo pocket"],
        cores: &["mednafen_ngp_libretro"],
    },
    PlatformDef {
        slug: "wonderswan",
        name: "WonderSwan",
        exts: &["ws", "wsc"],
        aliases: &["wonderswan"],
        cores: &["mednafen_wswan_libretro"],
    },
    PlatformDef {
        slug: "threedo",
        name: "3DO",
        exts: &[],
        aliases: &["3do", "panasonic 3do"],
        cores: &["opera_libretro"],
    },
    PlatformDef {
        slug: "msx",
        name: "MSX",
        exts: &["mx1", "mx2"],
        aliases: &["msx", "msx2"],
        cores: &["bluemsx_libretro", "fmsx_libretro"],
    },
    PlatformDef {
        slug: "c64",
        name: "Commodore 64",
        exts: &["d64", "t64", "prg", "crt", "tap"],
        aliases: &["c64", "commodore 64"],
        cores: &["vice_x64_libretro"],
    },
    PlatformDef {
        slug: "amiga",
        name: "Commodore Amiga",
        exts: &["adf", "ipf", "hdf", "lha"],
        aliases: &["amiga", "commodore amiga"],
        cores: &["puae_libretro", "uae4arm_libretro"],
    },
    PlatformDef {
        slug: "colecovision",
        name: "ColecoVision",
        exts: &["col"],
        aliases: &["colecovision", "coleco"],
        cores: &["gearcoleco_libretro", "bluemsx_libretro"],
    },
    PlatformDef {
        slug: "intellivision",
        name: "Intellivision",
        exts: &["int", "itv"],
        aliases: &["intellivision"],
        cores: &["freeintv_libretro"],
    },
    PlatformDef {
        slug: "dos",
        name: "MS-DOS",
        exts: &[],
        aliases: &["dos", "ms-dos", "msdos", "dosbox"],
        cores: &["dosbox_pure_libretro"],
    },
    PlatformDef {
        slug: "scummvm",
        name: "ScummVM",
        exts: &["scummvm", "svm"],
        aliases: &["scummvm", "scumm"],
        cores: &["scummvm_libretro"],
    },
    PlatformDef {
        slug: "arcade",
        name: "Arcade",
        exts: &[],
        aliases: &["arcade", "mame", "fbneo", "fba", "final burn"],
        cores: &["fbneo_libretro", "mame_libretro"],
    },
];

/// Extensions that several platforms share. These are never enough on their own
/// to identify a platform, so they get resolved by folder assignment or by a
/// parent directory name instead.
pub const AMBIGUOUS_EXTS: &[&str] = &[
    "cue", "bin", "iso", "chd", "img", "ccd", "mds", "m3u", "toc", "nrg", "rom", "dsk",
    // Compressed ISO: PSP, PS2, GameCube and Wii all use the name.
    "ciso",
];

/// Systems whose emulators cannot take an `.m3u`, so their multi-disc games
/// stay as separate discs rather than being folded into a playlist that
/// nothing would open. Xenia swaps discs itself when the game asks.
pub const NO_DISC_PLAYLISTS: &[&str] = &["ps3", "ps4", "ps5", "xbox", "xbox360", "wiiu", "switch"];

/// Archives we can look inside to find the real ROM.
pub const ARCHIVE_EXTS: &[&str] = &["zip", "7z"];

/// Archives we index but cannot read into (no bundled extractor), so the file
/// on disk gets hashed as-is and the platform has to come from its name.
pub const OPAQUE_ARCHIVE_EXTS: &[&str] = &["rar"];

pub fn by_slug(slug: &str) -> Option<&'static PlatformDef> {
    PLATFORMS.iter().find(|p| p.slug == slug)
}

pub fn display_name(slug: &str) -> String {
    by_slug(slug).map(|p| p.name.to_string()).unwrap_or_else(|| {
        if slug == "unknown" {
            "Unidentified".into()
        } else {
            slug.into()
        }
    })
}

/// Platforms that claim this extension.
pub fn candidates_for_ext(ext: &str) -> Vec<&'static PlatformDef> {
    let ext = ext.to_ascii_lowercase();
    PLATFORMS
        .iter()
        .filter(|p| p.exts.contains(&ext.as_str()))
        .collect()
}

/// Match a directory name (or any free text) against platform aliases.
/// The longest alias wins, so "super nintendo" beats "nintendo".
pub fn match_alias(text: &str) -> Option<&'static PlatformDef> {
    let hay = text.trim().to_ascii_lowercase();
    let words: Vec<&str> = hay
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();

    let mut best: Option<(&'static PlatformDef, usize)> = None;
    for p in PLATFORMS {
        for alias in p.aliases {
            let a = alias.to_ascii_lowercase();
            let hit = hay == a || hay.contains(&a) || words.iter().any(|w| *w == a);
            if hit && best.map_or(true, |(_, len)| a.len() > len) {
                best = Some((p, a.len()));
            }
        }
    }
    best.map(|(p, _)| p)
}

/// Match free text against platform aliases, but only on whole words.
///
/// `match_alias` accepts a bare substring, which is fine for a directory the
/// user named after a system but far too loose for a game title: "Legbreaker"
/// contains "gb", "Dosbox Adventures" contains "dos". Here an alias has to
/// line up with word boundaries, so only a title that genuinely names its
/// system matches. The longest alias still wins.
pub fn match_alias_word(text: &str) -> Option<&'static PlatformDef> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .collect();
    if words.is_empty() {
        return None;
    }
    // Pad so a single `contains` is a word-boundary test for multi-word
    // aliases ("nintendo 64") as well as single-word ones.
    let padded = format!(" {} ", words.join(" "));

    let mut best: Option<(&'static PlatformDef, usize)> = None;
    for p in PLATFORMS {
        for alias in p.aliases {
            let a = alias.to_ascii_lowercase();
            if padded.contains(&format!(" {a} ")) && best.map_or(true, |(_, len)| a.len() > len) {
                best = Some((p, a.len()));
            }
        }
    }
    best.map(|(p, _)| p)
}

/// True if we should even consider indexing this file.
pub fn is_indexable_ext(ext: &str) -> bool {
    let e = ext.to_ascii_lowercase();
    AMBIGUOUS_EXTS.contains(&e.as_str())
        || ARCHIVE_EXTS.contains(&e.as_str())
        || OPAQUE_ARCHIVE_EXTS.contains(&e.as_str())
        || PLATFORMS.iter().any(|p| p.exts.contains(&e.as_str()))
}

/// True if this extension alone identifies exactly one platform.
pub fn is_unique_ext(ext: &str) -> bool {
    candidates_for_ext(ext).len() == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_matching_ignores_letters_inside_other_words() {
        // The loose matcher is happy to find a system inside any old word —
        // "Legbreaker" reads as Game Boy — which is why titles get the
        // strict one.
        assert_eq!(match_alias("Legbreaker").map(|p| p.slug), Some("gb"));
        assert_eq!(match_alias_word("Legbreaker").map(|p| p.slug), None);

        // Punctuation still separates words, so a real mention is not lost.
        assert_eq!(
            match_alias_word("Metroid_Prime(GameCube).iso").map(|p| p.slug),
            Some("gamecube")
        );
    }

    #[test]
    fn word_matching_finds_a_system_the_title_actually_names() {
        assert_eq!(
            match_alias_word("New Super Mario Bros Wii [SMNE01].7z").map(|p| p.slug),
            Some("wii")
        );
        assert_eq!(
            match_alias_word("Sonic 3 (Mega Drive).bin").map(|p| p.slug),
            Some("genesis")
        );
        // Longest alias wins, so this is Super Nintendo rather than plain NES.
        assert_eq!(
            match_alias_word("Super Nintendo - Super Metroid").map(|p| p.slug),
            Some("snes")
        );
        assert_eq!(match_alias_word("Chrono Trigger").map(|p| p.slug), None);
    }

    #[test]
    fn dolphin_containers_belong_to_both_nintendo_discs() {
        // rvz/gcz/wia hold either disc, so neither system may claim them
        // outright — otherwise a Wii dump silently files itself as GameCube.
        for ext in ["rvz", "gcz", "wia"] {
            let slugs: Vec<&str> = candidates_for_ext(ext).iter().map(|p| p.slug).collect();
            assert_eq!(slugs, vec!["gamecube", "wii"], "{ext}");
            assert!(!is_unique_ext(ext), "{ext}");
        }
        // wbfs is Wii's alone and should still settle on sight.
        assert!(is_unique_ext("wbfs"));
    }

    /// The newer consoles share words with the older ones, and the longer
    /// alias has to win or every Xbox 360 folder files itself as an Xbox.
    #[test]
    fn newer_consoles_beat_the_ones_they_share_a_name_with() {
        assert_eq!(match_alias("Xbox 360").map(|p| p.slug), Some("xbox360"));
        assert_eq!(match_alias("Xbox360 Games").map(|p| p.slug), Some("xbox360"));
        assert_eq!(match_alias("Microsoft Xbox").map(|p| p.slug), Some("xbox"));
        assert_eq!(match_alias("Sony Playstation 3").map(|p| p.slug), Some("ps3"));
        assert_eq!(match_alias("PS4").map(|p| p.slug), Some("ps4"));
        assert_eq!(match_alias("Nintendo Wii U").map(|p| p.slug), Some("wiiu"));
        assert_eq!(match_alias("Nintendo Wii").map(|p| p.slug), Some("wii"));
        assert_eq!(
            match_alias_word("Mario Kart 8 (Wii U).wua").map(|p| p.slug),
            Some("wiiu")
        );
        assert_eq!(
            match_alias_word("Halo 3 (Xbox 360).iso").map(|p| p.slug),
            Some("xbox360")
        );
    }

    #[test]
    fn compressed_switch_dumps_are_switch_games() {
        assert_eq!(candidates_for_ext("nsz").first().map(|p| p.slug), Some("switch"));
        assert_eq!(candidates_for_ext("xcz").first().map(|p| p.slug), Some("switch"));
        assert!(is_unique_ext("wua"));
    }

    #[test]
    fn seven_zip_is_readable_rather_than_opaque() {
        assert!(ARCHIVE_EXTS.contains(&"7z"));
        assert!(!OPAQUE_ARCHIVE_EXTS.contains(&"7z"));
        assert!(is_indexable_ext("7z"));
        assert!(is_indexable_ext("rar"));
    }
}
