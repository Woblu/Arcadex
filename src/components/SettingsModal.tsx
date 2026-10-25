import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import * as api from "../api";
import type {
  DetectedEmulator,
  EmulatorConfig,
  HackBundle,
  ImportProgress,
  ProviderStatus,
  LibraryFolder,
  PlatformInfo,
  Settings,
} from "../types";

import { SKINS, isSkin, DEFAULT_SKIN, type SkinName } from "../skins/shell";
import ControllerSetup from "./ControllerSetup";
import type { PadBindings, PadLayout } from "../gamepad";
import type { CacheUsage, DatStatus } from "../types";
import {
  checkForUpdate,
  currentVersion,
  formatBytes,
  installUpdate,
  type DownloadState,
  type UpdateInfo,
} from "../update";

type Tab = "folders" | "emulators" | "metadata" | "hacks" | "dats" | "appearance";

interface Props {
  onClose: () => void;
  /** Applied at once, so the picker is its own preview. */
  onSkinChange: (skin: SkinName) => void;
  /**
   * Scan and fetch metadata, exactly as the button and a dropped folder do.
   * Adding a folder is a request to have its games; making that a second
   * thing to go and press was an oversight, not a decision.
   */
  onScanRequest: () => void;
  /** Go back over every game, including ones that already have artwork. */
  onRefetchAll: () => void;
}

export default function SettingsModal({
  onClose,
  onSkinChange,
  onScanRequest,
  onRefetchAll,
}: Props) {
  const [tab, setTab] = useState<Tab>("folders");
  const [folders, setFolders] = useState<LibraryFolder[]>([]);
  const [platforms, setPlatforms] = useState<PlatformInfo[]>([]);
  const [known, setKnown] = useState<PlatformInfo[]>([]);
  const [emulators, setEmulators] = useState<EmulatorConfig[]>([]);
  const [settings, setSettings] = useState<Settings>({});
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [catalogSize, setCatalogSize] = useState(0);
  const [importing, setImporting] = useState(false);
  const [importLine, setImportLine] = useState<string | null>(null);
  const [bundles, setBundles] = useState<HackBundle[] | null>(null);
  const [bundlesLoading, setBundlesLoading] = useState(false);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [bundleFilter, setBundleFilter] = useState("");
  const [detected, setDetected] = useState<DetectedEmulator | null>(null);
  const [detectNote, setDetectNote] = useState<string | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [providers, setProviders] = useState<ProviderStatus[] | null>(null);
  const [testing, setTesting] = useState(false);
  const [version, setVersion] = useState("");
  const [cache, setCache] = useState<CacheUsage | null>(null);
  const [dats, setDats] = useState<DatStatus | null>(null);
  const [datNote, setDatNote] = useState<string | null>(null);
  const [datBusy, setDatBusy] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateNote, setUpdateNote] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [download, setDownload] = useState<DownloadState>({ phase: "idle" });

  const reload = async () => {
    try {
      const [f, p, k, e, s] = await Promise.all([
        api.listLibraryFolders(),
        api.listPlatforms(),
        api.knownPlatforms(),
        api.listEmulators(),
        api.getSettings(),
      ]);
      setFolders(f);
      setPlatforms(p);
      setKnown(k);
      setEmulators(e);
      setSettings(s);
      setCatalogSize(await api.patchCatalogSize());
    } catch (err) {
      setError(api.errorMessage(err));
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  // The importer streams progress while it walks a bundle.
  useEffect(() => {
    const sub = listen<ImportProgress>("import-progress", (e) => {
      const p = e.payload;
      setImportLine(
        p.done
          ? `${p.imported} catalogued, ${p.skipped} skipped`
          : `${p.scanned} scanned — ${p.message}`,
      );
    });
    return () => {
      void sub.then((f) => f());
    };
  }, []);

  /// Look for RetroArch and fill the fields in when they are still empty.
  const runDetect = async (explicit: boolean) => {
    setDetecting(true);
    setDetectNote(null);
    try {
      const found = await api.detectRetroarch();
      setDetected(found);
      if (!found) {
        setDetectNote(
          explicit
            ? "No RetroArch install found in the usual places. Set the path by hand below."
            : null,
        );
        return;
      }
      setSettings((prev) => {
        const next = { ...prev };
        if (explicit || !next.retroarch_path) next.retroarch_path = found.path;
        if ((explicit || !next.retroarch_cores_dir) && found.coresDir) {
          next.retroarch_cores_dir = found.coresDir;
        }
        return next;
      });
      setDetectNote(
        `Found via ${found.source}${
          found.coreCount > 0 ? ` — ${found.coreCount} cores installed` : " — no cores found yet"
        }. Press Save to keep it.`,
      );
    } catch (err) {
      setError(api.errorMessage(err));
    } finally {
      setDetecting(false);
    }
  };

  const runTest = async () => {
    setTesting(true);
    setError(null);
    try {
      // Test what is stored, so save first.
      await api.saveSettings(settings);
      setProviders(await api.testCredentials());
    } catch (err) {
      setError(api.errorMessage(err));
    } finally {
      setTesting(false);
    }
  };

  const loadBundles = async () => {
    setBundlesLoading(true);
    setError(null);
    try {
      setBundles(await api.listHackBundles());
    } catch (err) {
      setError(api.errorMessage(err));
    } finally {
      setBundlesLoading(false);
    }
  };

  const getBundle = async (bundle: HackBundle) => {
    setDownloading(bundle.name);
    setError(null);
    setImportLine(null);
    try {
      const summary = await api.downloadHackBundle(bundle);
      setImportLine(summary);
      setCatalogSize(await api.patchCatalogSize());
    } catch (err) {
      setError(api.errorMessage(err));
    } finally {
      setDownloading(null);
    }
  };

  const runImport = async (pick: () => Promise<string | null>) => {
    setError(null);
    setImportLine(null);
    try {
      const path = await pick();
      if (!path) return;
      setImporting(true);
      const summary = await api.importPatches(path);
      setImportLine(summary);
      setCatalogSize(await api.patchCatalogSize());
    } catch (err) {
      setError(api.errorMessage(err));
    } finally {
      setImporting(false);
    }
  };

  useEffect(() => {
    if (tab === "emulators") {
      void api.cacheUsage().then(setCache).catch(() => setCache(null));
    }
    if (tab === "dats") {
      void api.datStatus().then(setDats).catch(() => setDats(null));
    }
    if (tab === "emulators" && !settings.retroarch_path && detected === null) {
      void runDetect(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, settings.retroarch_path]);

  useEffect(() => {
    void currentVersion().then(setVersion);
  }, []);

  const runUpdateCheck = async () => {
    setChecking(true);
    setUpdateNote(null);
    setUpdate(null);
    try {
      const found = await checkForUpdate(true);
      if (found) setUpdate(found);
      else setUpdateNote("Playdex is up to date.");
    } catch (err) {
      // A button press deserves the real reason — no endpoint yet, no
      // network, a release without a manifest.
      setUpdateNote(`Could not check: ${api.errorMessage(err)}`);
    } finally {
      setChecking(false);
    }
  };

  const runUpdateInstall = async () => {
    if (!update) return;
    setDownload({ phase: "downloading", received: 0, total: null });
    try {
      await installUpdate(update, setDownload);
    } catch (err) {
      setDownload({ phase: "failed", message: api.errorMessage(err) });
    }
  };

  const setValue = (key: string, value: string) =>
    setSettings((prev) => ({ ...prev, [key]: value }));

  const persist = async () => {
    try {
      await api.saveSettings(settings);
      setMessage("Saved");
      window.setTimeout(() => setMessage(null), 2000);
    } catch (err) {
      setError(api.errorMessage(err));
    }
  };

  const addFolder = async () => {
    try {
      const path = await api.pickFolder();
      if (!path) return;
      await api.addLibraryFolder(path, null);
      await reload();
      // You added a folder because you want what is in it.
      onScanRequest();
    } catch (err) {
      setError(api.errorMessage(err));
    }
  };

  const emulatorFor = (slug: string): EmulatorConfig =>
    emulators.find((e) => e.platform === slug) ?? {
      platform: slug,
      // A system with no libretro core in existence starts on Standalone.
      // Offering RetroArch as its default is offering something that cannot
      // work, and leaves you to notice that yourself.
      mode:
        (known.find((k) => k.slug === slug)?.cores.length ?? 0) > 0
          ? "retroarch"
          : "custom",
      core: null,
      customCommand: null,
    };

  const updateEmulator = async (config: EmulatorConfig) => {
    setEmulators((prev) => {
      const rest = prev.filter((e) => e.platform !== config.platform);
      return [...rest, config];
    });
    try {
      await api.saveEmulator(config);
    } catch (err) {
      setError(api.errorMessage(err));
    }
  };

  return (
    <div className="overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal">
        <div className="modal-head">Settings</div>

        <div className="tabs">
          {(
            ["folders", "emulators", "metadata", "hacks", "dats", "appearance"] as Tab[]
          ).map((t) => (
            <button
              key={t}
              className={`tab ${tab === t ? "active" : ""}`}
              onClick={() => setTab(t)}
            >
              {t === "folders"
                ? "ROM folders"
                : t === "emulators"
                  ? "Emulators"
                  : t === "metadata"
                    ? "Metadata"
                    : t === "hacks"
                      ? "ROM hacks"
                      : t === "dats"
                        ? "Verification"
                        : "Appearance"}
            </button>
          ))}
        </div>

        <div className="modal-body">
          {error && <div className="error-banner">{error}</div>}

          {tab === "folders" && (
            <>
              <div className="notice">
                Playdex indexes ROMs already on your disk. Point it at the
                folders you keep them in. If a folder holds one system, set it
                below — that resolves formats like <code>.bin</code>,{" "}
                <code>.iso</code> and <code>.zip</code> that several consoles
                share.
              </div>

              {folders.length === 0 && (
                <div className="hint" style={{ marginBottom: 12 }}>
                  No folders added yet.
                </div>
              )}

              {folders.map((f) => (
                <div className="folder-row" key={f.id}>
                  <div className="folder-path">{f.path}</div>
                  <select
                    className="select-inline"
                    value={f.platformOverride ?? ""}
                    onChange={async (e) => {
                      await api.addLibraryFolder(f.path, e.target.value || null);
                      await reload();
                      // Assigning a system changes what everything beneath it
                      // is, so the entries under it are re-detected.
                      onScanRequest();
                    }}
                  >
                    <option value="">Detect</option>
                    {known.map((p) => (
                      <option key={p.slug} value={p.slug}>
                        {p.name}
                      </option>
                    ))}
                  </select>
                  <button
                    className="btn small danger"
                    onClick={async () => {
                      await api.removeLibraryFolder(f.id);
                      await reload();
                    }}
                  >
                    Remove
                  </button>
                </div>
              ))}

              <div className="row" style={{ marginTop: 14 }}>
                <button className="btn primary" onClick={addFolder}>
                  Add folder
                </button>
                <button
                  className="btn"
                  onClick={async () => {
                    const removed = await api.cleanMissing();
                    setMessage(`Removed ${removed} missing entries`);
                    await reload();
                  }}
                >
                  Clean missing entries
                </button>
              </div>
            </>
          )}

          {tab === "emulators" && (
            <>
              <div className="row" style={{ marginBottom: 12 }}>
                <button
                  className="btn"
                  disabled={detecting}
                  onClick={() => void runDetect(true)}
                >
                  {detecting ? "Looking…" : "Detect RetroArch"}
                </button>
                <span className="hint" style={{ flex: 1 }}>
                  {detectNote ??
                    "Checks the standard install folders, Steam, Scoop, Chocolatey and PATH."}
                </span>
              </div>

              <div className="field">
                <label>RetroArch executable</label>
                <div className="row">
                  <input
                    value={settings.retroarch_path ?? ""}
                    placeholder="C:\\RetroArch\\retroarch.exe"
                    onChange={(e) => setValue("retroarch_path", e.target.value)}
                  />
                  <button
                    className="btn"
                    onClick={async () => {
                      const p = await api.pickFile();
                      if (p) setValue("retroarch_path", p);
                    }}
                  >
                    Browse
                  </button>
                </div>
              </div>

              <div className="field">
                <label>Cores folder</label>
                <div className="row">
                  <input
                    value={settings.retroarch_cores_dir ?? ""}
                    placeholder="Leave blank to use the cores folder next to RetroArch"
                    onChange={(e) =>
                      setValue("retroarch_cores_dir", e.target.value)
                    }
                  />
                  <button
                    className="btn"
                    onClick={async () => {
                      const p = await api.pickFolder();
                      if (p) setValue("retroarch_cores_dir", p);
                    }}
                  >
                    Browse
                  </button>
                </div>
              </div>

              <div className="section-title">Per-system</div>
              {platforms.length === 0 && (
                <div className="hint">
                  Scan some ROMs first and the systems you own will show up here.
                </div>
              )}

              {platforms.map((p) => {
                const cfg = emulatorFor(p.slug);
                const defaultCore =
                  known.find((k) => k.slug === p.slug)?.cores[0] ?? "";
                // Some systems have no libretro core in existence at all.
                const noCore =
                  (known.find((k) => k.slug === p.slug)?.cores.length ?? 0) === 0;
                return (
                  <div className="field" key={p.slug}>
                    <label>
                      {p.name} · {p.gameCount} games
                      {noCore && (
                        <span className="tag-nocore">standalone only</span>
                      )}
                    </label>
                    <div className="row">
                      <select
                        className="select-inline"
                        value={cfg.mode}
                        onChange={(e) =>
                          void updateEmulator({
                            ...cfg,
                            mode: e.target.value as "retroarch" | "custom",
                          })
                        }
                      >
                        <option value="retroarch">RetroArch</option>
                        <option value="custom">Standalone</option>
                      </select>

                      {cfg.mode === "retroarch" ? (
                        <input
                          value={cfg.core ?? ""}
                          placeholder={defaultCore || "core name"}
                          onChange={(e) =>
                            void updateEmulator({ ...cfg, core: e.target.value })
                          }
                        />
                      ) : (
                        <>
                          <input
                            value={cfg.customCommand ?? ""}
                            placeholder={'"C:\\Emu\\emulator.exe" "{rom}"'}
                            onChange={(e) =>
                              void updateEmulator({
                                ...cfg,
                                customCommand: e.target.value,
                              })
                            }
                          />
                          <button
                            className="btn small"
                            onClick={async () => {
                              const exe = await api.pickFile();
                              if (!exe) return;
                              // Build the command instead of making someone
                              // get the quoting and the {rom} placeholder
                              // right by hand, along with any arguments the
                              // emulator is known to need - RPCS3 opens its
                              // game list without --no-gui. Whatever else it
                              // wants can be typed in after.
                              const { command, emulator } =
                                await api.suggestEmulatorCommand(exe);
                              setMessage(
                                emulator
                                  ? `Recognised ${emulator}. Its command is filled in.`
                                  : null,
                              );
                              void updateEmulator({
                                ...cfg,
                                customCommand: command,
                              });
                            }}
                          >
                            Browse
                          </button>
                        </>
                      )}
                    </div>
                    <div className="hint">
                      {cfg.mode === "retroarch"
                        ? `Core file name without extension. Default: ${defaultCore || "none known"}`
                        : noCore
                          ? "No libretro core exists for this system, so it needs a standalone emulator. Browse to its .exe and the command is filled in for you; {rom} is where the game's path goes."
                          : "Browse to the emulator's .exe, or type the command yourself. {rom} is where the game's path goes."}
                    </div>
                  </div>
                );
              })}

              <div className="section-title">Unpacked games</div>
              <div className="hint">
                Disc emulators cannot open an archive, so a game kept as a
                <code> .7z </code> or <code> .zip </code> is unpacked once and
                the real ROM handed over. Those copies are kept so the next
                launch is instant. The originals are never touched, and
                anything cleared here is simply rebuilt next time you play.
              </div>
              <div className="about-row">
                <div>
                  <div className="about-version">
                    {cache
                      ? `${formatBytes(cache.bytes)} across ${cache.entries} game${cache.entries === 1 ? "" : "s"}`
                      : "…"}
                  </div>
                  {cache && (
                    <div className="hint">
                      Past the limit, the game you have not played in longest
                      is dropped.
                    </div>
                  )}
                </div>
                <button
                  className="btn small"
                  disabled={!cache || cache.entries === 0 || clearing}
                  onClick={async () => {
                    setClearing(true);
                    try {
                      await api.clearCache();
                      setCache(await api.cacheUsage());
                    } catch (err) {
                      setError(api.errorMessage(err));
                    } finally {
                      setClearing(false);
                    }
                  }}
                >
                  {clearing ? "Clearing…" : "Clear"}
                </button>
              </div>
              <div className="field" style={{ marginTop: 10 }}>
                <label>Keep at most</label>
                <div className="row">
                  <input
                    type="number"
                    min={1}
                    style={{ maxWidth: 120 }}
                    value={settings.cache_limit_gb ?? "64"}
                    onChange={(e) => setValue("cache_limit_gb", e.target.value)}
                  />
                  <span className="hint">GB</span>
                </div>
                <div className="hint">
                  One disc game can be most of a small limit on its own, so
                  this is generous by default. Press Save to apply.
                </div>
              </div>
            </>
          )}

          {tab === "metadata" && (
            <>
              <div className="section-title" style={{ marginTop: 0 }}>
                Artwork
              </div>
              <label className="toggle">
                <input
                  type="checkbox"
                  checked={(settings.libretro_enabled ?? "1") !== "0"}
                  onChange={(e) =>
                    setValue("libretro_enabled", e.target.checked ? "1" : "0")
                  }
                />
                <span>
                  <strong>libretro thumbnails</strong>
                  <span className="hint" style={{ marginTop: 2 }}>
                    RetroArch's own artwork server — box art, screenshots and
                    title screens. No account, no key, no quota, and it works
                    the moment you scan. Matches on your ROM's filename, so
                    No-Intro naming gets the best results.
                  </span>
                </span>
              </label>

              <div className="notice" style={{ marginTop: 18 }}>
                Everything below is optional and adds <em>descriptions</em>,
                developer, genre and release dates — the things artwork alone
                cannot give you.
                <br />
                <br />
                Cover art and details come from community metadata databases.
                Use your own accounts — ScreenScraper needs a registered
                developer key plus your personal login, and TheGamesDB issues a
                free API key. Neither is used to download games; they only
                describe ROMs you already have.
              </div>

              <div className="section-title">ScreenScraper</div>
              <div className="field">
                <label>Developer ID</label>
                <input
                  value={settings.ss_devid ?? ""}
                  onChange={(e) => setValue("ss_devid", e.target.value)}
                />
              </div>
              <div className="field">
                <label>Developer password</label>
                <input
                  type="password"
                  value={settings.ss_devpassword ?? ""}
                  onChange={(e) => setValue("ss_devpassword", e.target.value)}
                />
                <div className="hint">
                  Requested from screenscraper.fr once you have an account.
                  Without it the API only answers a trickle of requests.
                </div>
              </div>
              <div className="field">
                <label>Your username</label>
                <input
                  value={settings.ss_user ?? ""}
                  onChange={(e) => setValue("ss_user", e.target.value)}
                />
              </div>
              <div className="field">
                <label>Your password</label>
                <input
                  type="password"
                  value={settings.ss_password ?? ""}
                  onChange={(e) => setValue("ss_password", e.target.value)}
                />
                <div className="hint">
                  Your personal login raises the per-minute and daily quota.
                </div>
              </div>

              <div className="section-title">TheGamesDB (fallback)</div>
              <div className="field">
                <label>API key</label>
                <input
                  value={settings.tgdb_key ?? ""}
                  onChange={(e) => setValue("tgdb_key", e.target.value)}
                />
                <div className="hint">
                  Used when ScreenScraper has no match or its quota runs out.
                  Matches on name rather than hash, so it is less precise.
                </div>
              </div>

              <div className="row" style={{ margin: "6px 0 14px" }}>
                <button className="btn" disabled={testing} onClick={runTest}>
                  {testing ? "Checking…" : "Test credentials"}
                </button>
                <span className="hint" style={{ flex: 1 }}>
                  Saves, then asks each provider whether your keys work.
                </span>
              </div>

              <div className="row" style={{ margin: "6px 0 14px" }}>
                <button
                  className="btn"
                  onClick={() => {
                    onClose();
                    onRefetchAll();
                  }}
                >
                  Fetch everything again
                </button>
                <span className="hint" style={{ flex: 1 }}>
                  Looks up every game in the library, including ones that
                  already have artwork, and replaces what it finds. Normally
                  only games with no metadata are fetched. On a large library
                  this takes a while and uses a good deal of your ScreenScraper
                  allowance — the progress card can cancel it.
                </span>
              </div>

              {providers && (
                <div style={{ marginBottom: 16 }}>
                  {providers.map((p) => (
                    <div
                      className="folder-row"
                      key={p.provider}
                      style={{
                        borderLeft: `3px solid ${
                          !p.configured
                            ? "var(--border-strong)"
                            : p.ok
                              ? "var(--success)"
                              : "var(--danger)"
                        }`,
                      }}
                    >
                      <div className="folder-path">
                        <div style={{ color: "var(--text)" }}>
                          {p.provider}
                          {" — "}
                          {!p.configured ? "not set up" : p.ok ? "working" : "not working"}
                        </div>
                        <div className="card-sub">{p.message}</div>
                        {p.quota && <div className="card-sub">{p.quota}</div>}
                      </div>
                    </div>
                  ))}
                </div>
              )}

              <div className="section-title">Preferences</div>
              <div className="field">
                <label>Preferred region</label>
                <select
                  value={settings.preferred_region ?? "us"}
                  onChange={(e) => setValue("preferred_region", e.target.value)}
                >
                  <option value="us">North America</option>
                  <option value="eu">Europe</option>
                  <option value="jp">Japan</option>
                  <option value="wor">World</option>
                </select>
                <div className="hint">
                  Chooses which region's box art and title are preferred.
                </div>
              </div>
              <div className="field">
                <label>Preferred language</label>
                <select
                  value={settings.preferred_lang ?? "en"}
                  onChange={(e) => setValue("preferred_lang", e.target.value)}
                >
                  <option value="en">English</option>
                  <option value="fr">French</option>
                  <option value="de">German</option>
                  <option value="es">Spanish</option>
                  <option value="pt">Portuguese</option>
                </select>
              </div>
            </>
          )}
          {tab === "dats" && (
            <>
              <div className="notice">
                A DAT is a catalogue of known-good dumps: for every release of a
                system, its exact size and checksums. Import one and Playdex can
                tell you which of your files are correct dumps and which are
                not, on the hashes it already computes for everything.
                <br />
                <br />
                No-Intro covers cartridge systems and Redump covers discs.
                Neither offers an API, so the files come from you: download the
                DAT for a system once and point Playdex at it.
              </div>

              <div className="row" style={{ marginTop: 12 }}>
                <button
                  className="btn primary"
                  disabled={datBusy}
                  onClick={async () => {
                    setDatNote(null);
                    const path = await api.pickFile();
                    if (!path) return;
                    setDatBusy(true);
                    try {
                      const summary = await api.importDat(path);
                      setDatNote(
                        `${summary.datName}: ${summary.added} dumps catalogued` +
                          (summary.duplicates > 0
                            ? `, ${summary.duplicates} already known`
                            : ""),
                      );
                      setDats(await api.datStatus());
                    } catch (err) {
                      setDatNote(api.errorMessage(err));
                    } finally {
                      setDatBusy(false);
                    }
                  }}
                >
                  {datBusy ? "Reading…" : "Import a DAT file"}
                </button>
                {dats && dats.total > 0 && (
                  <button
                    className="btn small"
                    disabled={datBusy}
                    onClick={async () => {
                      await api.clearDats();
                      setDats(await api.datStatus());
                      setDatNote("Catalogue emptied.");
                    }}
                  >
                    Clear catalogue
                  </button>
                )}
              </div>

              {datNote && <div className="hint" style={{ marginTop: 8 }}>{datNote}</div>}

              {dats && dats.total > 0 && (
                <>
                  <div className="section-title">Loaded</div>
                  {dats.loaded.map(([name, n]) => (
                    <div className="hint" key={name}>
                      {name} — {n.toLocaleString()} dumps
                    </div>
                  ))}

                  <div className="section-title">Your library</div>
                  <div className="pad-legend">
                    <span>
                      <b>{dats.library.verified ?? 0}</b> verified good dumps
                    </span>
                    <span>
                      <b>{dats.library.unknown ?? 0}</b> not in the catalogue
                    </span>
                    <span>
                      <b>{dats.library.notHashed ?? 0}</b> too large to have been
                      hashed
                    </span>
                  </div>
                  <div className="hint" style={{ marginTop: 8 }}>
                    "Not in the catalogue" is not necessarily bad. It means no
                    DAT you have loaded lists those checksums, which covers bad
                    dumps and modified ROMs but also systems you have not
                    imported a DAT for.
                  </div>
                </>
              )}
            </>
          )}

          {tab === "appearance" && (
            <>
              <div className="notice">
                The same library, laid out for wherever you are using it. All
                three show the same games and share the same settings — only
                the shape changes.
              </div>

              <div className="skin-picker">
                {SKINS.map((option) => {
                  const current = isSkin(settings.ui_skin)
                    ? settings.ui_skin
                    : DEFAULT_SKIN;
                  return (
                    <button
                      key={option.value}
                      className={`skin-option ${current === option.value ? "on" : ""}`}
                      onClick={async () => {
                        const next = { ...settings, ui_skin: option.value };
                        setSettings(next);
                        onSkinChange(option.value);
                        try {
                          await api.saveSettings(next);
                        } catch (err) {
                          setError(api.errorMessage(err));
                        }
                      }}
                    >
                      <SkinPreview skin={option.value} />
                      <div className="skin-name">{option.label}</div>
                      <div className="skin-blurb">{option.blurb}</div>
                    </button>
                  );
                })}
              </div>

              <div className="section-title">About</div>
              <div className="about-row">
                <div>
                  <div className="about-version">Playdex {version || "…"}</div>
                  <div className="hint">
                    Updates are downloaded from the project&apos;s releases and
                    checked against a signature before anything is installed.
                  </div>
                </div>
                <button
                  className="btn small"
                  onClick={() => void runUpdateCheck()}
                  disabled={checking || download.phase !== "idle"}
                >
                  {checking ? "Checking…" : "Check for updates"}
                </button>
              </div>

              {updateNote && <div className="hint">{updateNote}</div>}

              {update && (
                <div className="notice" style={{ marginTop: 10 }}>
                  <strong>Playdex {update.version} is available.</strong>
                  {update.notes && (
                    <div style={{ marginTop: 6, whiteSpace: "pre-wrap" }}>
                      {update.notes}
                    </div>
                  )}

                  {download.phase === "failed" && (
                    <div className="error-banner" style={{ marginTop: 8 }}>
                      {download.message}
                    </div>
                  )}

                  {download.phase === "downloading" ||
                  download.phase === "installing" ? (
                    <div className="hint" style={{ marginTop: 8 }}>
                      {download.phase === "installing"
                        ? "Installing — Playdex will restart."
                        : download.total
                          ? `Downloading ${formatBytes(download.received)} of ${formatBytes(download.total)}…`
                          : `Downloading ${formatBytes(download.received)}…`}
                    </div>
                  ) : (
                    <button
                      className="btn small primary"
                      style={{ marginTop: 10 }}
                      onClick={() => void runUpdateInstall()}
                    >
                      Update and restart
                    </button>
                  )}
                </div>
              )}

              <div className="section-title">Controller</div>
              <div className="hint">
                A pad is picked up on its own, with no pairing step. The D-pad
                or left stick moves between whatever is on screen, including
                this window, and the focus ring thickens while a controller is
                connected so it can be seen from a sofa.
              </div>

              <ControllerSetup
                layout={
                  (["auto", "standard", "nintendo", "custom"].includes(
                    settings.pad_layout ?? "",
                  )
                    ? settings.pad_layout
                    : "auto") as PadLayout
                }
                custom={parseBindings(settings.pad_bindings)}
                onChange={(layout, custom) => {
                  const next = {
                    ...settings,
                    pad_layout: layout,
                    pad_bindings: custom ? JSON.stringify(custom) : "",
                  };
                  setSettings(next);
                  void api.saveSettings(next).catch((err) =>
                    setError(api.errorMessage(err)),
                  );
                }}
              />

              <div className="pad-legend">
                <span><b>A</b> Play, or press what is focused</span>
                <span><b>B</b> Back / close</span>
                <span><b>X</b> Open a game&apos;s panel</span>
                <span><b>Y</b> Favourite</span>
                <span><b>LB</b> / <b>RB</b> Previous / next system</span>
                <span><b>Start</b> Play the selected game</span>
              </div>
            </>
          )}

          {tab === "hacks" && (
            <>
              <div className="notice">
                Import ROM hack patches in bulk and Playdex indexes them by
                the checksum of the ROM each one was built for. A game's detail
                panel then shows exactly which hacks your dump can run — no
                guessing about revisions or regions.
                <br />
                <br />
                Point it at a folder of patches, or straight at a{" "}
                <code>.7z</code> bundle from a community archive. Only the
                patch files are read out of a bundle; everything else inside is
                skipped rather than unpacked.
              </div>

              <div className="field">
                <label>Catalog</label>
                <div className="row">
                  <div style={{ flex: 1 }}>
                    {catalogSize.toLocaleString()} patches indexed
                  </div>
                  {catalogSize > 0 && (
                    <button
                      className="btn small danger"
                      disabled={importing}
                      onClick={async () => {
                        await api.clearPatchCatalog();
                        setCatalogSize(await api.patchCatalogSize());
                        setImportLine(null);
                      }}
                    >
                      Clear catalog
                    </button>
                  )}
                </div>
              </div>

              <div className="row">
                <button
                  className="btn primary"
                  disabled={importing}
                  onClick={() => runImport(api.pickFolder)}
                >
                  Import folder…
                </button>
                <button
                  className="btn"
                  disabled={importing}
                  onClick={() => runImport(api.pickFile)}
                >
                  Import .7z or patch…
                </button>
              </div>

              {importLine && (
                <div className="hint" style={{ marginTop: 10 }}>
                  {importing ? "Importing — " : ""}
                  {importLine}
                </div>
              )}

              <div className="hint" style={{ marginTop: 14 }}>
                Patches in a folder are catalogued where they are; nothing is
                copied. Patches pulled out of a bundle are stored in the app
                data directory. Applying one never modifies your ROM.
              </div>

              <div className="section-title">Download from the Internet Archive</div>

              {bundles === null ? (
                <>
                  <button
                    className="btn"
                    onClick={loadBundles}
                    disabled={bundlesLoading}
                  >
                    {bundlesLoading ? "Loading…" : "Browse patch bundles"}
                  </button>
                  <div className="hint" style={{ marginTop: 6 }}>
                    The ROM Hack Patch Archive, the corpus ROMhacking.net
                    released when it closed. Bundles are grouped by system and
                    by game, and run to roughly a gigabyte each — pick the one
                    that matches your library rather than taking the lot.
                  </div>
                </>
              ) : (
                <>
                  <input
                    value={bundleFilter}
                    placeholder="Filter bundles… e.g. SNES, Pokemon, Fire Emblem"
                    onChange={(e) => setBundleFilter(e.target.value)}
                    style={{ marginBottom: 10 }}
                  />
                  <div className="bundle-list">
                    {bundles
                      .filter((b) =>
                        b.name
                          .toLowerCase()
                          .includes(bundleFilter.trim().toLowerCase()),
                      )
                      .map((b) => (
                        <div className="folder-row" key={b.name}>
                          <div className="folder-path">
                            <div style={{ color: "var(--text)" }}>
                              {b.name.replace(/\.7z$/i, "")}
                            </div>
                            <div className="card-sub">
                              {(b.size / 1e9).toFixed(2)} GB
                            </div>
                          </div>
                          <button
                            className="btn small"
                            disabled={downloading !== null}
                            onClick={() => void getBundle(b)}
                          >
                            {downloading === b.name ? "Downloading…" : "Download"}
                          </button>
                        </div>
                      ))}
                  </div>
                  <div className="hint" style={{ marginTop: 8 }}>
                    Only the patch files are kept — the rest of each archive is
                    skipped rather than unpacked.
                  </div>
                </>
              )}
            </>
          )}
        </div>

        <div className="modal-foot">
          {message && (
            <span className="hint" style={{ marginRight: "auto" }}>
              {message}
            </span>
          )}
          <button className="btn" onClick={onClose}>
            Close
          </button>
          <button className="btn primary" onClick={persist}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * A wireframe of each layout, so the choice can be made by eye rather than by
 * reading three descriptions and guessing. Drawn rather than screenshotted —
 * a thumbnail stays right when the skin changes.
 */
function SkinPreview({ skin }: { skin: SkinName }) {
  const box = (
    x: number,
    y: number,
    w: number,
    h: number,
    fill: string,
    rx = 1.5,
  ) => <rect key={`${x}-${y}-${w}`} x={x} y={y} width={w} height={h} rx={rx} fill={fill} />;

  return (
    <svg className="skin-thumb" viewBox="0 0 100 60" role="img" aria-hidden="true">
      {skin === "launchbox" && (
        <>
          {box(0, 0, 100, 60, "#151920", 3)}
          {box(4, 4, 22, 52, "#1b2028", 2)}
          {box(30, 4, 66, 7, "#1b2028")}
          {[0, 1, 2, 3].map((i) =>
            box(30 + (i % 4) * 17, 15, 14, 16, "#2b3441"),
          )}
          {[0, 1, 2, 3].map((i) =>
            box(30 + (i % 4) * 17, 34, 14, 16, "#2b3441"),
          )}
        </>
      )}

      {skin === "switch" && (
        <>
          {box(0, 0, 100, 60, "#2a2a2e", 3)}
          {box(4, 4, 92, 6, "#3a3a40")}
          {box(6, 16, 12, 3, "#a0a0a8", 1)}
          {[0, 1, 2, 3].map((i) => box(6 + i * 22, 24, 18, 18, "#3a3a40", 2))}
          <rect
            x={27}
            y={22}
            width={22}
            height={22}
            rx={2.5}
            fill="#3a3a40"
            stroke="#00c3e3"
            strokeWidth={2}
          />
          {box(4, 50, 92, 6, "#3a3a40")}
        </>
      )}

      {skin === "steam" && (
        <>
          {box(0, 0, 100, 60, "#10161f", 3)}
          {box(0, 0, 24, 60, "#0a0f16", 3)}
          {box(24, 0, 76, 26, "#22405f")}
          {box(29, 17, 20, 5, "#4c9be8", 1.5)}
          {[0, 1, 2, 3].map((i) => box(29 + i * 17, 32, 13, 20, "#182231"))}
          {box(24, 55, 76, 5, "#0d131b", 0)}
        </>
      )}
    </svg>
  );
}

/** Stored as JSON in the settings table; a corrupt value is simply ignored. */
function parseBindings(raw: string | undefined): PadBindings | null {
  if (!raw) return null;
  try {
    const v = JSON.parse(raw);
    return typeof v?.confirm === "number" ? (v as PadBindings) : null;
  } catch {
    return null;
  }
}
