import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import * as api from "./api";
import { errorMessage } from "./api";
import type {
  Game,
  GameFilter,
  LibraryStats,
  PlatformInfo,
  ScanProgress,
  ScrapeProgress,
  SortKey,
  UnpackProgress,
} from "./types";

import GameDetail from "./components/GameDetail";
import SettingsModal from "./components/SettingsModal";
import StatsModal from "./components/StatsModal";
import ProgressToast from "./components/ProgressToast";
import UpdateBanner from "./components/UpdateBanner";
import DropZone from "./components/DropZone";
import { checkForUpdate, type UpdateInfo } from "./update";

import LaunchboxShell from "./skins/LaunchboxShell";
import SwitchShell from "./skins/SwitchShell";
import SteamShell from "./skins/SteamShell";
import ArcShell from "./skins/ArcShell";
import BrowserShell from "./skins/BrowserShell";
import WiiShell from "./skins/WiiShell";
import {
  DEFAULT_SKIN,
  isSkin,
  SKINS,
  type ShellProps,
  type SkinName,
} from "./skins/shell";
import {
  activateFocused,
  focusFirst,
  focusInDirection,
  resolveBindings,
  useGamepad,
  type PadBindings,
  type PadInfo,
  type PadLayout,
} from "./gamepad";

/**
 * Every skin, keyed by the value stored in settings.
 *
 * Registering one is this entry and its entry in `SKINS` — there is no branch
 * anywhere that has to learn about it, which is the point: the previous chain
 * of ternaries silently fell back to the desktop layout for anything it did
 * not recognise.
 */
const SHELLS: Record<SkinName, React.ComponentType<ShellProps>> = {
  launchbox: LaunchboxShell,
  switch: SwitchShell,
  steam: SteamShell,
  arc: ArcShell,
  wii: WiiShell,
  browser: BrowserShell,
};

export default function App() {
  const [games, setGames] = useState<Game[]>([]);
  const [platforms, setPlatforms] = useState<PlatformInfo[]>([]);
  const [stats, setStats] = useState<LibraryStats | null>(null);

  const [platform, setPlatform] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [sort, setSort] = useState<SortKey>("title");
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const [unscrapedOnly, setUnscrapedOnly] = useState(false);

  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [statsOpen, setStatsOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [skin, setSkin] = useState<SkinName>(DEFAULT_SKIN);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  const [detailOpen, setDetailOpen] = useState(false);
  const [padConnected, setPadConnected] = useState(false);
  const [dropNote, setDropNote] = useState<string | null>(null);
  // Set by whichever skin is running, so Back can step out of a view that
  // belongs to the skin before it starts closing panels.
  const skinBack = useRef<(() => boolean) | null>(null);
  const [padInfo, setPadInfo] = useState<PadInfo | null>(null);
  const [padLayout, setPadLayout] = useState<PadLayout>("auto");
  const [padCustom, setPadCustom] = useState<PadBindings | null>(null);

  const [scan, setScan] = useState<ScanProgress | null>(null);
  const [scrape, setScrape] = useState<ScrapeProgress | null>(null);
  const [nowPlaying, setNowPlaying] = useState<string | null>(null);
  /** A game being unpacked before it starts, which can take minutes. */
  const [unpacking, setUnpacking] = useState<UnpackProgress | null>(null);

  const hideTimer = useRef<number | null>(null);

  const filter: GameFilter = useMemo(
    () => ({
      platform,
      search: debouncedSearch,
      sort,
      favoritesOnly,
      unscrapedOnly,
      showHidden: false,
    }),
    [platform, debouncedSearch, sort, favoritesOnly, unscrapedOnly],
  );

  useEffect(() => {
    const t = window.setTimeout(() => setDebouncedSearch(search), 180);
    return () => window.clearTimeout(t);
  }, [search]);

  const refreshGames = useCallback(async () => {
    try {
      setGames(await api.listGames(filter));
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [filter]);

  const refreshSidebar = useCallback(async () => {
    try {
      const [p, s] = await Promise.all([api.listPlatforms(), api.libraryStats()]);
      setPlatforms(p);
      setStats(s);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    void refreshGames();
  }, [refreshGames]);

  useEffect(() => {
    void refreshSidebar();
  }, [refreshSidebar]);

  // The chosen skin lives in the same settings table as everything else, so
  // it survives a restart without a second store.
  const loadSkin = useCallback(async () => {
    try {
      const values = await api.getSettings();
      setSkin(isSkin(values.ui_skin) ? values.ui_skin : DEFAULT_SKIN);

      const layout = values.pad_layout;
      if (layout === "auto" || layout === "standard" || layout === "nintendo" || layout === "custom") {
        setPadLayout(layout);
      }
      try {
        setPadCustom(values.pad_bindings ? JSON.parse(values.pad_bindings) : null);
      } catch {
        setPadCustom(null);
      }
    } catch {
      setSkin(DEFAULT_SKIN);
    }
  }, []);

  useEffect(() => {
    void loadSkin();
  }, [loadSkin]);

  // Look for a new version once, a few seconds in — after the library has
  // drawn, so a slow or unreachable endpoint never delays the app opening.
  // A failed check is silent here; only the button in Settings reports why.
  useEffect(() => {
    const id = window.setTimeout(() => {
      void checkForUpdate().then((found) => found && setUpdate(found));
    }, 3000);
    return () => window.clearTimeout(id);
  }, []);

  // Backend progress and launch events.
  useEffect(() => {
    const unlisteners: Array<Promise<() => void>> = [
      listen<ScanProgress>("scan-progress", (e) => {
        setScan(e.payload);
        if (e.payload.done) {
          void refreshGames();
          void refreshSidebar();
          scheduleHide(() => setScan(null));
        }
      }),
      listen<ScrapeProgress>("scrape-progress", (e) => {
        setScrape(e.payload);
        if (e.payload.done) {
          void refreshGames();
          void refreshSidebar();
          scheduleHide(() => setScrape(null));
        }
      }),
      listen<UnpackProgress>("unpack-progress", (e) => {
        setUnpacking(e.payload.finished ? null : e.payload);
      }),
      listen<{ title: string }>("game-launched", (e) => {
        setNowPlaying(e.payload.title);
      }),
      listen<{ title: string }>("game-exited", () => {
        setNowPlaying(null);
        void refreshGames();
        void refreshSidebar();
      }),
    ];

    return () => {
      unlisteners.forEach((p) => void p.then((f) => f()));
    };
  }, [refreshGames, refreshSidebar]);

  function scheduleHide(fn: () => void) {
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(fn, 4000);
  }

  const selected = useMemo(
    () => games.find((g) => g.id === selectedId) ?? null,
    [games, selectedId],
  );

  async function run<T>(fn: () => Promise<T>): Promise<T | undefined> {
    setBusy(true);
    setError(null);
    try {
      return await fn();
    } catch (e) {
      setError(errorMessage(e));
      return undefined;
    } finally {
      setBusy(false);
    }
  }

  const handleScan = () =>
    run(async () => {
      const result = await api.scanLibrary();
      await refreshGames();
      await refreshSidebar();

      // A new game arrives with nothing but a filename, so go and get its
      // artwork and details straight away rather than making that a second
      // thing to remember. Only when the scan actually found something, and
      // only for entries that still need it — this re-runs no work. The
      // progress toast carries its own cancel button.
      if (result.added > 0) {
        try {
          const summary = await api.scrapeLibrary(null);
          setScrape((prev) =>
            prev ? { ...prev, done: true, status: summary } : prev,
          );
        } catch (e) {
          // Every provider switched off is a setting, not a failed scan.
          setScrape((prev) =>
            prev ? { ...prev, done: true, status: api.errorMessage(e) } : prev,
          );
        }
        await refreshGames();
      }
    });

  // Dropping a ROM on the window is the same act as scanning finds it, so it
  // goes through the same indexing and then the same automatic metadata fetch.
  const handleDrop = async (paths: string[]) => {
    const result = await run(async () => {
      const tally = await api.addDropped(paths);
      await refreshGames();
      await refreshSidebar();
      return tally;
    });
    if (!result) return;

    setDropNote(
      result.reasons.length > 0
        ? `${message(result)} — ${result.reasons[0]}`
        : message(result),
    );
    window.setTimeout(() => setDropNote(null), 6000);

    if (result.lastId !== null) setSelectedId(result.lastId);
    if (result.added > 0) {
      try {
        await api.scrapeLibrary(null);
      } catch {
        // Nothing configured, or cancelled. The games are still added.
      }
      await refreshGames();
    }
  };

  const handleScrape = () =>
    run(async () => {
      const summary = await api.scrapeLibrary(platform);
      setScrape((prev) =>
        prev ? { ...prev, done: true, status: summary } : prev,
      );
    });

  /**
   * Go back over everything, including games that already have artwork.
   *
   * Deliberately whole-library rather than honouring the current system
   * filter: it is reached from Settings, where the filter is out of sight, and
   * a button called "fetch everything" quietly doing one shelf would be a
   * worse surprise than doing what it says.
   */
  const handleRefetchAll = () =>
    run(async () => {
      const summary = await api.scrapeLibrary(null, true);
      setScrape((prev) =>
        prev ? { ...prev, done: true, status: summary } : prev,
      );
    });

  /**
   * One game at a time.
   *
   * The pad is stopped while a game runs, but that leaves the moment between
   * asking for a launch and the game reporting itself started, and it leaves
   * the mouse, which can be clicked twice. Starting a second copy of a game
   * is never what anyone meant, so it is refused here as well.
   */
  const launching = useRef(false);

  const handleLaunch = (id: number) =>
    run(async () => {
      if (launching.current || nowPlaying) return;
      launching.current = true;
      try {
        await api.launchGame(id);
      } finally {
        launching.current = false;
      }
    });

  const handleToggleFavorite = (game: Game) =>
    run(async () => {
      await api.setFavorite(game.id, !game.favorite);
      await refreshGames();
    });

  const handleScrapeOne = (id: number) =>
    run(async () => {
      const msg = await api.scrapeOne(id);
      await refreshGames();
      return msg;
    });

  const handleRemove = (id: number) =>
    run(async () => {
      await api.removeGame(id);
      setSelectedId(null);
      await refreshGames();
      await refreshSidebar();
    });

  const handleSetPlatform = (id: number, slug: string) =>
    run(async () => {
      await api.setGamePlatform(id, slug);
      await refreshGames();
      await refreshSidebar();
    });

  const isEmptyLibrary = stats?.totalGames === 0;

  // The console and living-room skins are built around a chosen game: their
  // hero art, caption and Start button have nothing to say without one. So
  // they always have a selection, and it follows the list when filtering
  // leaves the old choice behind. The desktop skin does the opposite — there,
  // selecting is what opens the panel, so it must stay deliberate.
  useEffect(() => {
    if (skin === "launchbox" || games.length === 0) return;
    if (selectedId !== null && games.some((g) => g.id === selectedId)) return;
    setSelectedId(games[0].id);
  }, [skin, games, selectedId]);

  // ------------------------------------------------------------ controller

  const anyModalOpen = settingsOpen || statsOpen || detailOpen;

  useEffect(() => {
    document.body.classList.toggle("pad-active", padConnected);
  }, [padConnected]);

  // The skin is announced on <body> so the shared panels - settings, stats,
  // a game's own page - can dress themselves to match. A desktop
  // dialog dropped into a console screen undoes the whole illusion, and these
  // are the same components either way; only their presentation changes.
  useEffect(() => {
    for (const known of SKINS) {
      document.body.classList.remove(`skin-${known.value}`);
    }
    document.body.classList.add(`skin-${skin}`);
  }, [skin]);

  // Land somewhere sensible when the view changes under the pad, so the first
  // press moves rather than having to first find a starting point.
  useEffect(() => {
    if (!padConnected) return;
    const active = document.activeElement as HTMLElement | null;
    if (!active || !active.hasAttribute("data-nav")) {
      const id = window.setTimeout(focusFirst, 60);
      return () => window.clearTimeout(id);
    }
  }, [padConnected, skin, games, selectedId, anyModalOpen]);

  const closeTopmost = useCallback(() => {
    if (statsOpen) return setStatsOpen(false);
    if (settingsOpen) return setSettingsOpen(false);
    if (detailOpen) return setDetailOpen(false);
    // The skin gets its own state back before the desktop rules apply.
    if (skinBack.current?.()) return;
    if (skin === "launchbox" && selectedId !== null) return setSelectedId(null);
  }, [statsOpen, settingsOpen, detailOpen, skin, selectedId]);

  // Escape does what Back does, so the same way out exists without a pad.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      const el = document.activeElement;
      // Leave a text field alone; Escape there means "stop typing".
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
        return;
      }
      closeTopmost();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [closeTopmost]);

  // Shoulder buttons page through systems, which is the one move that is
  // tedious with a stick and instant with a bumper.
  const stepPlatform = useCallback(
    (delta: number) => {
      const withGames = platforms.filter((p) => p.gameCount > 0);
      if (withGames.length === 0) return;
      const slugs: Array<string | null> = [null, ...withGames.map((p) => p.slug)];
      const at = slugs.indexOf(platform);
      const next = slugs[(at + delta + slugs.length) % slugs.length];
      setPlatform(next);
      setFavoritesOnly(false);
      setUnscrapedOnly(false);
    },
    [platforms, platform],
  );

  useGamepad(
    {
      onConnected: (connected, info) => {
        setPadConnected(connected);
        setPadInfo(info);
      },
      onDirection: (dir) => {
        if (!focusInDirection(dir)) focusFirst();
      },
      onConfirm: () => {
        // On a game tile, the obvious thing a pad should do is start the game;
        // anywhere else, press what is focused.
        const active = document.activeElement as HTMLElement | null;
        const id = active?.dataset.gameId;
        if (id && !anyModalOpen) {
          handleLaunch(Number(id));
          return;
        }
        activateFocused();
      },
      onBack: closeTopmost,
      onAlt: () => {
        const active = document.activeElement as HTMLElement | null;
        const raw = active?.dataset.gameId;
        const id = raw ? Number(raw) : selectedId;
        if (id !== null && id !== undefined && !anyModalOpen) {
          setSelectedId(id);
          setDetailOpen(true);
        }
      },
      onAux: () => {
        const active = document.activeElement as HTMLElement | null;
        const raw = active?.dataset.gameId;
        const id = raw ? Number(raw) : selectedId;
        const game = games.find((g) => g.id === id);
        if (game && !anyModalOpen) void handleToggleFavorite(game);
      },
      onStart: () => {
        if (selectedId !== null && !anyModalOpen) handleLaunch(selectedId);
      },
      onShoulder: (side) => {
        if (!anyModalOpen) stepPlatform(side === "left" ? -1 : 1);
      },
    },
    {
      bindings: resolveBindings(padLayout, padCustom, padInfo),
      // Hands off entirely while a game is running. The emulator has the pad,
      // and every press meant for the game was also arriving here - which,
      // with a game tile focused, started the game again on every press.
      enabled: !nowPlaying,
    },
  );

  // ------------------------------------------------------------ rendering

  const shell: ShellProps = {
    games,
    platforms,
    stats,
    platform,
    search,
    sort,
    favoritesOnly,
    unscrapedOnly,
    selectedId,
    selected,
    busy,
    error,
    isEmptyLibrary: !!isEmptyLibrary,
    padConnected,
    onSelectPlatform: (slug) => {
      setPlatform(slug);
      setFavoritesOnly(false);
      setUnscrapedOnly(false);
    },
    onToggleFavorites: () => {
      setFavoritesOnly((v) => !v);
      setUnscrapedOnly(false);
    },
    onToggleUnscraped: () => {
      setUnscrapedOnly((v) => !v);
      setFavoritesOnly(false);
    },
    onSearch: setSearch,
    onSort: setSort,
    onSelect: setSelectedId,
    onLaunch: handleLaunch,
    onToggleFavorite: handleToggleFavorite,
    onOpenDetail: (id) => {
      setSelectedId(id);
      setDetailOpen(true);
    },
    registerBack: (handler) => {
      skinBack.current = handler;
    },
    onScan: handleScan,
    onScrape: handleScrape,
    onOpenSettings: () => setSettingsOpen(true),
    onOpenStats: () => setStatsOpen(true),
  };

  // On the desktop skin, picking a game *is* opening its panel. The console
  // and living-room skins show their own game page instead, so there the
  // panel is the "Options" / "Manage" button and nothing else.
  const showDetail = selected && (skin === "launchbox" || detailOpen);

  const closeDetail = () => {
    setDetailOpen(false);
    if (skin === "launchbox") setSelectedId(null);
  };

  return (
    <>
      {(() => {
        const Shell = SHELLS[skin] ?? LaunchboxShell;
        return <Shell {...shell} />;
      })()}

      {showDetail && selected && (
        <GameDetail
          game={selected}
          onClose={closeDetail}
          onLaunch={() => handleLaunch(selected.id)}
          onToggleFavorite={() => handleToggleFavorite(selected)}
          onScrape={() => handleScrapeOne(selected.id)}
          onRemove={() => handleRemove(selected.id)}
          onSetPlatform={(slug) => handleSetPlatform(selected.id, slug)}
          onHackAdded={() => {
            void refreshGames();
            void refreshSidebar();
          }}
          onArtworkChanged={() => void refreshGames()}
          busy={busy}
        />
      )}

      {settingsOpen && (
        <SettingsModal
          onSkinChange={setSkin}
          onScanRequest={() => void handleScan()}
          onRefetchAll={() => void handleRefetchAll()}
          onClose={() => {
            setSettingsOpen(false);
            void loadSkin();
            void refreshSidebar();
            void refreshGames();
          }}
        />
      )}

      {statsOpen && (
        <StatsModal
          onClose={() => setStatsOpen(false)}
          onPick={(id) => {
            setStatsOpen(false);
            setSelectedId(id);
          }}
        />
      )}

      <DropZone onDrop={handleDrop} />

      {dropNote && <div className="drop-note">{dropNote}</div>}

      {update && !updateDismissed && (
        <UpdateBanner info={update} onDismiss={() => setUpdateDismissed(true)} />
      )}

      {(scan || scrape || nowPlaying || unpacking) && (
        <ProgressToast
          scan={scan}
          scrape={scrape}
          nowPlaying={nowPlaying}
          unpacking={unpacking}
          onCancelScrape={() => void api.cancelScrape()}
        />
      )}
    </>
  );
}

/** One line describing what a drop did. */
function message(t: {
  added: number;
  skipped: number;
  ignored: number;
  folders: number;
}): string {
  const parts: string[] = [];
  if (t.folders > 0) parts.push(`Added ${t.folders} folder${t.folders === 1 ? "" : "s"}`);
  if (t.added > 0) parts.push(`Added ${t.added} game${t.added === 1 ? "" : "s"}`);
  if (t.skipped > 0) parts.push(`${t.skipped} already in your library`);
  if (t.ignored > 0) parts.push(`${t.ignored} not a game`);
  return parts.length > 0 ? parts.join(", ") : "Nothing to add";
}
