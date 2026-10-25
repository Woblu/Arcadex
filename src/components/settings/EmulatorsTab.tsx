import { useEffect, useState } from "react";

import * as api from "../../api";
import type {
  CacheUsage,
  DetectedEmulator,
  EmulatorConfig,
  FoundEmulator,
  PlatformInfo,
} from "../../types";
import { formatBytes } from "../../update";
import CommitField from "../CommitField";
import type { TabProps } from "./tab";

export default function EmulatorsTab({
  settings,
  setValue,
  setSettings,
  onError,
  onMessage,
  active,
}: TabProps) {
  const [platforms, setPlatforms] = useState<PlatformInfo[] | null>(null);
  const [known, setKnown] = useState<PlatformInfo[]>([]);
  const [emulators, setEmulators] = useState<EmulatorConfig[] | null>(null);
  const [detected, setDetected] = useState<DetectedEmulator | null>(null);
  const [detectNote, setDetectNote] = useState<string | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [found, setFound] = useState<FoundEmulator[] | null>(null);
  const [findNote, setFindNote] = useState<string | null>(null);
  const [finding, setFinding] = useState(false);
  const [cache, setCache] = useState<CacheUsage | null>(null);
  const [clearing, setClearing] = useState(false);

  useEffect(() => {
    Promise.all([api.listPlatforms(), api.knownPlatforms(), api.listEmulators()])
      .then(([p, k, e]) => {
        setPlatforms(p);
        setKnown(k);
        setEmulators(e);
      })
      .catch((err) => onError(api.errorMessage(err)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Systems with no libretro core in existence at all. */
  const needsStandalone = (slug: string) =>
    (known.find((k) => k.slug === slug)?.cores.length ?? 0) === 0;

  const emulatorFor = (slug: string): EmulatorConfig =>
    emulators?.find((e) => e.platform === slug) ?? {
      platform: slug,
      // A system with no libretro core in existence starts on Standalone.
      // Offering RetroArch as its default is offering something that cannot
      // work, and leaves you to notice that yourself.
      mode: needsStandalone(slug) ? "custom" : "retroarch",
      core: null,
      customCommand: null,
    };

  const updateEmulator = async (config: EmulatorConfig) => {
    setEmulators((prev) => [
      ...(prev ?? []).filter((e) => e.platform !== config.platform),
      config,
    ]);
    try {
      await api.saveEmulator(config);
    } catch (err) {
      onError(api.errorMessage(err));
    }
  };

  /// Look for RetroArch and fill the fields in when they are still empty.
  const runDetect = async (explicit: boolean) => {
    setDetecting(true);
    setDetectNote(null);
    try {
      const hit = await api.detectRetroarch();
      setDetected(hit);
      if (!hit) {
        setDetectNote(
          explicit
            ? "No RetroArch install found in the usual places. Set the path by hand below."
            : null,
        );
        return;
      }
      setSettings((prev) => {
        const next = { ...prev };
        if (explicit || !next.retroarch_path) next.retroarch_path = hit.path;
        if ((explicit || !next.retroarch_cores_dir) && hit.coresDir) {
          next.retroarch_cores_dir = hit.coresDir;
        }
        return next;
      });
      setDetectNote(
        `Found via ${hit.source}${
          hit.coreCount > 0 ? ` — ${hit.coreCount} cores installed` : " — no cores found yet"
        }. Press Save to keep it.`,
      );
    } catch (err) {
      onError(api.errorMessage(err));
    } finally {
      setDetecting(false);
    }
  };

  /// The systems you have games for that can only run on a standalone
  /// emulator and have not been pointed at one yet.
  const unset = () =>
    (platforms ?? []).filter((p) => {
      const cfg = emulatorFor(p.slug);
      return needsStandalone(p.slug) && cfg.mode === "custom" && !cfg.customCommand;
    });

  /// Look for standalone emulators, and point every system that has none at
  /// the first one found for it. A system already set up is never changed:
  /// that was a choice, and finding something is not a reason to undo it.
  const runFind = async (explicit: boolean) => {
    setFinding(true);
    setFindNote(null);
    try {
      const list = await api.detectEmulators();
      setFound(list);
      const filled: string[] = [];
      for (const p of unset()) {
        const hit = list.find((f) => f.platforms.includes(p.slug));
        if (hit) {
          await updateEmulator({
            platform: p.slug,
            mode: "custom",
            core: null,
            customCommand: hit.command,
          });
          filled.push(p.name);
        }
      }
      if (list.length === 0) {
        setFindNote(
          explicit
            ? "No standalone emulators found in the usual places. Browse to one below."
            : null,
        );
      } else {
        const names = [...new Set(list.map((f) => f.emulator))].join(", ");
        setFindNote(
          `Found ${names}.` +
            (filled.length ? ` ${filled.join(", ")} now use${filled.length === 1 ? "s" : ""} it.` : ""),
        );
      }
    } catch (err) {
      onError(api.errorMessage(err));
    } finally {
      setFinding(false);
    }
  };

  useEffect(() => {
    if (!active) return;
    void api.cacheUsage().then(setCache).catch(() => setCache(null));
    if (!settings.retroarch_path && detected === null) {
      void runDetect(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, settings.retroarch_path]);

  // A system you own that cannot run at all until it has an emulator is
  // worth one look on your behalf, the way RetroArch is.
  useEffect(() => {
    if (active && found === null && !finding && platforms && emulators && unset().length > 0) {
      void runFind(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, platforms, emulators, known]);

  const ownedSlugs = new Set((platforms ?? []).map((p) => p.slug));
  const nameOf = (slug: string) => api.platformName(known, slug);

  return (
    <>
      <div className="row" style={{ marginBottom: 12 }}>
        <button className="btn" disabled={detecting} onClick={() => void runDetect(true)}>
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
            onChange={(e) => setValue("retroarch_cores_dir", e.target.value)}
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

      <div className="section-title">Standalone emulators</div>
      <div className="row" style={{ marginBottom: 8 }}>
        <button className="btn" disabled={finding} onClick={() => void runFind(true)}>
          {finding ? "Looking…" : "Find emulators"}
        </button>
        <span className="hint" style={{ flex: 1 }}>
          {findNote ??
            "Looks for RPCS3, Xenia, Cemu, PCSX2, Dolphin and the rest in your emulator folders, Program Files, Downloads, EmuDeck and ES-DE. Fills in any system that has none."}
        </span>
      </div>
      {found && found.length > 0 && (
        <div style={{ marginBottom: 12 }}>
          {found.map((f) => (
            <div className="folder-row" key={f.path}>
              <div className="folder-path">
                <div style={{ color: "var(--text)" }}>{f.emulator}</div>
                <div className="card-sub">{f.path}</div>
              </div>
              {f.platforms
                .filter((slug) => ownedSlugs.has(slug))
                .map((slug) => {
                  const cfg = emulatorFor(slug);
                  const inUse = cfg.mode === "custom" && cfg.customCommand === f.command;
                  return (
                    <button
                      key={slug}
                      className="btn small"
                      disabled={inUse}
                      onClick={() =>
                        void updateEmulator({
                          platform: slug,
                          mode: "custom",
                          core: null,
                          customCommand: f.command,
                        })
                      }
                    >
                      {inUse ? `Used for ${nameOf(slug)}` : `Use for ${nameOf(slug)}`}
                    </button>
                  );
                })}
            </div>
          ))}
        </div>
      )}

      <div className="section-title">Per-system</div>
      {platforms?.length === 0 && (
        <div className="hint">
          Scan some ROMs first and the systems you own will show up here.
        </div>
      )}

      {(platforms ?? []).map((p) => {
        const cfg = emulatorFor(p.slug);
        const defaultCore = known.find((k) => k.slug === p.slug)?.cores[0] ?? "";
        const noCore = needsStandalone(p.slug);
        return (
          <div className="field" key={p.slug}>
            <label>
              {p.name} · {p.gameCount} games
              {noCore && <span className="tag-nocore">standalone only</span>}
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
                <CommitField
                  value={cfg.core ?? ""}
                  placeholder={defaultCore || "core name"}
                  onCommit={(core) => void updateEmulator({ ...cfg, core })}
                />
              ) : (
                <>
                  <CommitField
                    value={cfg.customCommand ?? ""}
                    placeholder={'"C:\\Emu\\emulator.exe" "{rom}"'}
                    onCommit={(customCommand) => void updateEmulator({ ...cfg, customCommand })}
                  />
                  <button
                    className="btn small"
                    onClick={async () => {
                      const exe = await api.pickFile();
                      if (!exe) return;
                      // Build the command instead of making someone get the
                      // quoting and the {rom} placeholder right by hand,
                      // along with any arguments the emulator is known to
                      // need - RPCS3 opens its game list without --no-gui.
                      // Whatever else it wants can be typed in after.
                      const { command, emulator } = await api.suggestEmulatorCommand(exe);
                      onMessage(
                        emulator ? `Recognised ${emulator}. Its command is filled in.` : null,
                      );
                      void updateEmulator({ ...cfg, customCommand: command });
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
        <code> .7z </code> or <code> .zip </code> is unpacked once and the real
        ROM handed over. Those copies are kept so the next launch is instant.
        The originals are never touched, and anything cleared here is simply
        rebuilt next time you play.
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
              Past the limit, the game you have not played in longest is dropped.
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
              onError(api.errorMessage(err));
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
          One disc game can be most of a small limit on its own, so this is
          generous by default. Press Save to apply.
        </div>
      </div>
    </>
  );
}
