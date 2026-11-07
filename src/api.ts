import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type {
  EmulatorConfig,
  Game,
  HackPreview,
  GameFilter,
  LibraryFolder,
  LibraryInsights,
  LibraryStats,
  Cheat,
  CacheUsage,
  DatImportSummary,
  DatStatus,
  DatVerdict,
  DropTally,
  ScanProgress,
  DetectedEmulator,
  FoundEmulator,
  HackBundle,
  PatchEntry,
  PlatformInfo,
  RetroArchCheats,
  SaveEntry,
  ProviderStatus,
  Settings,
  SetupStatus,
  SyncImported,
  SyncStatus,
} from "./types";

// ------------------------------------------------------------- library

export const listGames = (filter: GameFilter) =>
  invoke<Game[]>("list_games", { filter });

export const getGame = (id: number) => invoke<Game | null>("get_game", { id });

export const listPlatforms = () => invoke<PlatformInfo[]>("list_platforms");

export const knownPlatforms = () => invoke<PlatformInfo[]>("known_platforms");

export const libraryStats = () => invoke<LibraryStats>("library_stats");

export const libraryInsights = () =>
  invoke<LibraryInsights>("library_insights");

export const setFavorite = (id: number, value: boolean) =>
  invoke<void>("set_favorite", { id, value });

export const setHidden = (id: number, value: boolean) =>
  invoke<void>("set_hidden", { id, value });

export const setGamePlatform = (id: number, platform: string) =>
  invoke<void>("set_game_platform", { id, platform });

export const removeGame = (id: number) => invoke<void>("remove_game", { id });

// ------------------------------------------------------------- folders

export const listLibraryFolders = () =>
  invoke<LibraryFolder[]>("list_library_folders");

export const addLibraryFolder = (path: string, platform: string | null) =>
  invoke<void>("add_library_folder", { path, platform });

export const removeLibraryFolder = (id: number) =>
  invoke<void>("remove_library_folder", { id });

export const pickFolder = () => invoke<string | null>("pick_folder");

export const pickFile = () => invoke<string | null>("pick_file");

// ---------------------------------------------------------------- scan

export const scanLibrary = () => invoke<ScanProgress>("scan_library");

/** What cleaning up would remove, so it can be said before doing it. */
export const missingReport = () =>
  invoke<{ missing: number; unreachable: number }>("missing_report");

export const cleanMissing = () => invoke<number>("clean_missing");

/** How far setting up has got, for the first-run checklist. */
export const setupStatus = () => invoke<SetupStatus>("setup_status");

// ----------------------------------------------------------------- sync

/** Where sync stands: the folder, whether it is there, who else syncs. */
export const syncStatus = () => invoke<SyncStatus>("sync_status");

/** Read the other machines' play time in now, and write this one's. */
export const syncNow = () => invoke<SyncImported>("sync_now");

// -------------------------------------------------------------- scrape

/**
 * Fetch metadata across the library.
 *
 * `refetch` goes back over games that already have artwork. Left off, only
 * the ones with none are looked up, which is what should follow a scan.
 */
export const scrapeLibrary = (platform: string | null, refetch = false) =>
  invoke<string>("scrape_library", { platform, refetch });

export const scrapeOne = (id: number) => invoke<string>("scrape_one", { id });

/**
 * Pick a picture for a game by hand.
 *
 * Opens a file dialog and copies the chosen image into the game's own media
 * folder. Resolves to the new cover path, or null if the dialog was dismissed.
 */
export const chooseCustomCover = (id: number) =>
  invoke<string | null>("choose_custom_cover", { id });

/** Give up a hand-picked cover, so fetches can supply one again. */
export const clearCustomCover = (id: number) =>
  invoke<void>("clear_custom_cover", { id });

/** Rename a game. The new name survives later metadata fetches. */
export const renameGame = (id: number, title: string) =>
  invoke<void>("rename_game", { id, title });

/**
 * Give `to` the details and artwork of `from`, keeping `to`'s own name.
 * The artwork is copied, so removing `from` later does not take it away.
 */
export const borrowMetadata = (from: number, to: number) =>
  invoke<void>("borrow_metadata", { from, to });

export const cancelScrape = () => invoke<void>("cancel_scrape");

// -------------------------------------------------------------- launch

export const launchGame = (id: number) => invoke<void>("launch_game", { id });

export const previewLaunch = (id: number) =>
  invoke<string>("preview_launch", { id });

export const revealGame = (id: number) => invoke<void>("reveal_game", { id });

// --------------------------------------------------------------- hacks

export const inspectPatch = (baseGameId: number, patchPath: string) =>
  invoke<HackPreview>("inspect_patch", { baseGameId, patchPath });

export const addHack = (
  baseGameId: number,
  patchPath: string,
  title: string | null,
) => invoke<number>("add_hack", { baseGameId, patchPath, title });

export const applyCatalogPatch = (
  baseGameId: number,
  patchId: number,
  title: string | null,
) => invoke<number>("apply_catalog_patch", { baseGameId, patchId, title });

// -------------------------------------------------------- patch catalog

export const importPatches = (path: string) =>
  invoke<string>("import_patches", { path });

export const patchesForGame = (gameId: number) =>
  invoke<PatchEntry[]>("patches_for_game", { gameId });

export const listPatches = (search: string | null) =>
  invoke<PatchEntry[]>("list_patches", { search });

export const patchCatalogSize = () => invoke<number>("patch_catalog_size");

export const clearPatchCatalog = () => invoke<void>("clear_patch_catalog");

export const findCheats = (gameId: number) =>
  invoke<Cheat[]>("find_cheats", { gameId });

export const listCheats = (gameId: number) =>
  invoke<Cheat[]>("list_cheats", { gameId });

export const setCheat = (gameId: number, index: number, enabled: boolean) =>
  invoke<void>("set_cheat", { gameId, index, enabled });

export const setAllCheats = (gameId: number, enabled: boolean) =>
  invoke<Cheat[]>("set_all_cheats", { gameId, enabled });

export const saveCheats = (gameId: number) =>
  invoke<string>("save_cheats", { gameId });

export const retroarchCheatStatus = () =>
  invoke<RetroArchCheats>("retroarch_cheat_status");

export const enableAutoApplyCheats = () =>
  invoke<string>("enable_auto_apply_cheats");

// ------------------------------------------------------- hack bundles

export const listHackBundles = () =>
  invoke<HackBundle[]>("list_hack_bundles");

export const downloadHackBundle = (bundle: HackBundle) =>
  invoke<string>("download_hack_bundle", { bundle });

// --------------------------------------------------------------- saves

export const listSaves = (gameId: number) =>
  invoke<SaveEntry[]>("list_saves", { gameId });

export const backUpSaves = (gameId: number) =>
  invoke<string>("back_up_saves", { gameId });

/** Only a state this game's own listing found can be deleted. */
export const deleteSaveState = (gameId: number, path: string) =>
  invoke<void>("delete_save_state", { gameId, path });

// --------------------------------------------------------- diagnostics

export const detectRetroarch = () =>
  invoke<DetectedEmulator | null>("detect_retroarch");

/** Standalone emulators installed in the places people keep them. */
export const detectEmulators = () =>
  invoke<FoundEmulator[]>("detect_emulators");

/** The command to run games with the emulator at `exe`, with whatever
 *  arguments that emulator is known to need. */
export const suggestEmulatorCommand = (exe: string) =>
  invoke<{ command: string; emulator: string | null }>(
    "suggest_emulator_command",
    { exe },
  );

export const testCredentials = () =>
  invoke<ProviderStatus[]>("test_credentials");

// ------------------------------------------------------------ settings

export const getSettings = () => invoke<Settings>("get_settings");

export const saveSettings = (values: Settings) =>
  invoke<void>("save_settings", { values });

export const listEmulators = () => invoke<EmulatorConfig[]>("list_emulators");

export const saveEmulator = (config: EmulatorConfig) =>
  invoke<void>("save_emulator", { config });

export const effectiveEmulator = (platform: string) =>
  invoke<EmulatorConfig>("effective_emulator", { platform });

/** The discs a multi-disc entry stands for. Empty for an ordinary game. */
export const discMembers = (id: number) =>
  invoke<Game[]>("disc_members", { id });

/** One game's own emulator, or null when it just uses its system's. */
export const gameEmulator = (id: number) =>
  invoke<EmulatorConfig | null>("game_emulator", { id });

/** Pass null to hand the game back to its system's emulator. */
export const saveGameEmulator = (id: number, config: EmulatorConfig | null) =>
  invoke<void>("save_game_emulator", { id, config });

// --------------------------------------------------------------- media

/** Cached artwork is served over our own `media://` protocol. */
/**
 * Moved on whenever artwork may have changed on disk under the same path.
 *
 * A cover chosen again, or fetched again, is written over the old file, so
 * its address does not change and the webview kept showing the old picture
 * until a restart. A version on the end makes it a new address; the media
 * protocol reads only the path, so the query costs nothing.
 */
let artVersion = 0;

export function bumpArtVersion() {
  artVersion += 1;
}

export function artUrl(path: string | null): string | null {
  if (!path) return null;
  const url = convertFileSrc(path, "media");
  return artVersion > 0 ? `${url}?v=${artVersion}` : url;
}

// --------------------------------------------------------------- utils

export function formatPlaytime(seconds: number): string {
  if (!seconds) return "Never played";
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

export function formatSize(bytes: number): string {
  if (!bytes) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(value >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

export function formatDate(timestamp: number | null): string {
  if (!timestamp) return "—";
  return new Date(timestamp * 1000).toLocaleDateString();
}

/** Display name for a platform slug, falling back to the slug itself. */
export function platformName(
  platforms: PlatformInfo[],
  slug: string,
): string {
  return platforms.find((p) => p.slug === slug)?.name ?? slug;
}

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

// ------------------------------------------------------- unpacked cache

export const cacheUsage = () => invoke<CacheUsage>("cache_usage");
export const clearCache = () => invoke<number>("clear_cache");

export const addDropped = (paths: string[]) =>
  invoke<DropTally>("add_dropped", { paths });

export const unpackInPlace = (id: number) =>
  invoke<string>("unpack_in_place", { id });

// ------------------------------------------------------------------ DATs

export const importDat = (path: string) =>
  invoke<DatImportSummary>("import_dat", { path });

export const datStatus = () => invoke<DatStatus>("dat_status");
export const clearDats = () => invoke<void>("clear_dats");
export const verifyGame = (id: number) =>
  invoke<DatVerdict>("verify_game", { id });

/** Checksum a file a scan skipped as too large, and verify it. Reads the
 *  whole file, so it takes a while on a disc image. */
export const checksumGame = (id: number) =>
  invoke<DatVerdict>("checksum_game", { id });
