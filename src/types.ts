export interface Game {
  id: number;
  path: string;
  filename: string;
  platform: string;
  size: number;
  crc32: string | null;
  md5: string | null;
  sha1: string | null;
  innerName: string | null;
  title: string;
  description: string | null;
  developer: string | null;
  publisher: string | null;
  genre: string | null;
  releaseDate: string | null;
  players: string | null;
  rating: number | null;
  region: string | null;
  coverPath: string | null;
  screenshotPath: string | null;
  logoPath: string | null;
  scrapeStatus: "pending" | "ok" | "notfound" | "error";
  scrapeSource: string | null;
  favorite: boolean;
  hidden: boolean;
  playCount: number;
  playSeconds: number;
  lastPlayed: number | null;
  addedAt: number;
  /** Set when this entry is a ROM hack built from another game's ROM. */
  baseGameId: number | null;
  patchPath: string | null;
  /** True when the cover was chosen by hand; metadata fetches leave it alone. */
  coverCustom: boolean;
  /** True when the title was typed by hand; metadata fetches leave it alone. */
  titleCustom: boolean;
  /** A console game's own disc or catalogue number, `BLUS30443`. */
  serial: string | null;
}

export interface HackPreview {
  format: string;
  suggestedTitle: string;
  expectedCrc: string | null;
  baseCrc: string | null;
  compatible: boolean;
  message: string;
}

export interface PlatformInfo {
  slug: string;
  name: string;
  extensions: string[];
  cores: string[];
  gameCount: number;
}

export interface LibraryFolder {
  id: number;
  path: string;
  platformOverride: string | null;
  addedAt: number;
}

export interface LibraryStats {
  totalGames: number;
  totalPlatforms: number;
  scraped: number;
  totalPlaySeconds: number;
}

export interface GameFilter {
  platform?: string | null;
  search?: string | null;
  favoritesOnly?: boolean;
  unscrapedOnly?: boolean;
  showHidden?: boolean;
  sort?: SortKey;
}

export type SortKey = "title" | "recent" | "played" | "added" | "rating";

export interface ScanProgress {
  phase: string;
  current: number;
  total: number;
  message: string;
  added: number;
  skipped: number;
  /** Entries already present that were re-detected and re-hashed. */
  corrected: number;
  /** Entries dropped because they are no longer recognised as games. */
  dropped: number;
  /** Files that turned out not to be games at all. */
  ignored: number;
  /** Why they were ignored, grouped by reason. */
  ignoredSummary: string;
  done: boolean;
}

/** How far a game has got being unpacked before it starts. */
export interface UnpackProgress {
  gameId: number;
  title: string;
  done?: number;
  total?: number;
  finished: boolean;
}

export interface ScrapeProgress {
  current: number;
  total: number;
  gameId: number;
  title: string;
  status: string;
  source: string | null;
  done: boolean;
  haltedReason: string | null;
}

export interface EmulatorConfig {
  platform: string;
  mode: "retroarch" | "custom";
  core: string | null;
  customCommand: string | null;
}

export type Settings = Record<string, string>;

export interface PatchEntry {
  id: number;
  path: string;
  name: string;
  format: string;
  sourceCrc: string | null;
  systemHint: string | null;
  targetHint: string | null;
  origin: string;
  addedAt: number;
}

export interface ImportProgress {
  scanned: number;
  imported: number;
  skipped: number;
  message: string;
  done: boolean;
}

export interface DetectedEmulator {
  path: string;
  coresDir: string | null;
  source: string;
  coreCount: number;
}

/** How far setting up has got. */
export interface SetupStatus {
  folders: number;
  games: number;
  /** Systems you have games for that nothing is set up to run yet. */
  systemsWithoutEmulator: string[];
  /** Whether descriptions can be fetched, beyond artwork. */
  descriptions: boolean;
}

/** What Arcadex is running on. */
export interface Runtime {
  /** Inside a Flatpak, as on a Steam Deck. */
  sandboxed: boolean;
  /** Started from Steam's Game Mode. */
  gameMode: boolean;
  steamDeck: boolean;
}

/** Saves and play time shared through a folder the user already syncs. */
export interface SyncStatus {
  enabled: boolean;
  folder: string | null;
  /** False when the folder is a drive or share that is not there right now. */
  reachable: boolean;
  /** What this computer is called on the others. */
  name: string;
  others: Array<{ name: string; lastSeen: number }>;
}

/** What reading the other machines' play time added. */
export interface SyncImported {
  sessions: number;
  seconds: number;
}

/** A standalone emulator found installed. */
export interface FoundEmulator {
  emulator: string;
  path: string;
  /** Ready to save as a system's command. */
  command: string;
  /** The systems it runs. */
  platforms: string[];
}

export interface ProviderStatus {
  provider: string;
  configured: boolean;
  ok: boolean;
  message: string;
  quota: string | null;
}

export interface Cheat {
  index: number;
  description: string;
  /** A Game Genie code, or several joined with "+". */
  code: string;
  enabled: boolean;
}

export interface HackBundle {
  name: string;
  size: number;
  url: string;
}

export interface RetroArchCheats {
  configPath: string | null;
  cheatDir: string | null;
  /** RetroArch only applies a cheat file on its own when this is true. */
  autoApply: boolean;
}

export interface SaveEntry {
  /** "save" for battery progress, "state" for an emulator snapshot. */
  kind: string;
  slot: number | null;
  name: string;
  path: string;
  size: number;
  modified: number;
  screenshot: string | null;
}

export interface LibraryInsights {
  totalGames: number;
  gamesPlayed: number;
  totalPlaySeconds: number;
  sessionCount: number;
  longestSession: number;
  recent: Game[];
  mostPlayed: Game[];
}

export interface CacheUsage {
  bytes: number;
  entries: number;
  limitBytes: number;
}

export interface DropTally {
  added: number;
  skipped: number;
  ignored: number;
  folders: number;
  lastId: number | null;
  reasons: string[];
}

/** What a DAT catalogue says about one dump. */
export type DatVerdict =
  | { verified: { name: string; datName: string } }
  | "notInDatabase"
  | "noDats"
  | "notHashed";

export interface DatImportSummary {
  datName: string;
  added: number;
  duplicates: number;
}

export interface DatStatus {
  total: number;
  loaded: Array<[string, number]>;
  library: Record<string, number>;
}
