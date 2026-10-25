use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub platform: String,
    pub size: i64,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    /// Name used inside the archive, when the ROM is zipped. Scrapers want this,
    /// not the archive's own name.
    pub inner_name: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub release_date: Option<String>,
    pub players: Option<String>,
    pub rating: Option<f64>,
    pub region: Option<String>,
    pub cover_path: Option<String>,
    pub screenshot_path: Option<String>,
    pub logo_path: Option<String>,
    /// pending | ok | notfound | error
    pub scrape_status: String,
    pub scrape_source: Option<String>,
    pub favorite: bool,
    pub hidden: bool,
    pub play_count: i64,
    pub play_seconds: i64,
    pub last_played: Option<i64>,
    pub added_at: i64,
    /// Set when this entry is a ROM hack produced from another game's ROM.
    pub base_game_id: Option<i64>,
    pub patch_path: Option<String>,
    /// True when the cover was chosen by hand, which protects it from fetches.
    pub cover_custom: bool,
    /// True when the title was typed by hand, which protects it from fetches.
    pub title_custom: bool,
    /// A console game's own disc or catalogue number, `BLUS30443`.
    pub serial: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFolder {
    pub id: i64,
    pub path: String,
    /// When set, every ROM found under this folder is assigned this platform,
    /// which resolves ambiguous extensions like .bin/.cue/.iso/.zip.
    pub platform_override: Option<String>,
    pub added_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub slug: String,
    pub name: String,
    pub extensions: Vec<String>,
    pub cores: Vec<String>,
    pub game_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub message: String,
    pub added: usize,
    pub skipped: usize,
    /// Entries already present that were re-detected and re-hashed.
    pub corrected: usize,
    /// Entries dropped because they are no longer recognised as games.
    pub dropped: usize,
    /// Files that turned out not to be games at all.
    pub ignored: usize,
    /// Why they were ignored, grouped by reason.
    pub ignored_summary: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeProgress {
    pub current: usize,
    pub total: usize,
    pub game_id: i64,
    pub title: String,
    pub status: String,
    pub source: Option<String>,
    pub done: bool,
    /// Set when the run stopped early, e.g. ScreenScraper quota exhausted.
    pub halted_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFilter {
    pub platform: Option<String>,
    pub search: Option<String>,
    pub favorites_only: Option<bool>,
    pub unscraped_only: Option<bool>,
    pub show_hidden: Option<bool>,
    /// title | recent | played | added | rating
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub total_games: i64,
    pub total_platforms: i64,
    pub scraped: i64,
    pub total_play_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmulatorConfig {
    pub platform: String,
    /// libretro core filename (no extension), used with RetroArch
    pub core: Option<String>,
    /// Full command template for a standalone emulator, {rom} is substituted
    pub custom_command: Option<String>,
    /// retroarch | custom
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HackPreview {
    /// IPS, UPS or BPS
    pub format: String,
    pub suggested_title: String,
    /// CRC32 the patch expects, when the format records one.
    pub expected_crc: Option<String>,
    pub base_crc: Option<String>,
    /// False when we know the base ROM is the wrong one.
    pub compatible: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchEntry {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub format: String,
    /// CRC32 of the ROM this patch targets, uppercase hex.
    pub source_crc: Option<String>,
    pub system_hint: Option<String>,
    /// Folder the patch sat in, usually a shortened No-Intro ROM name.
    pub target_hint: Option<String>,
    pub origin: String,
    pub added_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub scanned: usize,
    pub imported: usize,
    pub skipped: usize,
    pub message: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cheat {
    /// Position in the source cheat file, used as its stable identity.
    pub index: i64,
    pub description: String,
    /// A Game Genie code, or several joined with "+".
    pub code: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HackBundle {
    pub name: String,
    pub size: i64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEntry {
    /// "save" for battery progress, "state" for an emulator snapshot.
    pub kind: String,
    /// Slot number for a numbered save state.
    pub slot: Option<i64>,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub modified: i64,
    /// RetroArch writes a thumbnail beside a state when configured to.
    pub screenshot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInsights {
    pub total_games: i64,
    pub games_played: i64,
    pub total_play_seconds: i64,
    pub session_count: i64,
    pub longest_session: i64,
    pub recent: Vec<Game>,
    pub most_played: Vec<Game>,
}
