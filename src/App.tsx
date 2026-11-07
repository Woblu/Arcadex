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
  SetupStatus,
  SortKey,
  UnpackProgress,
} from "./types";

import GameDetail from "./components/GameDetail";
import SettingsModal, { type Tab } from "./components/SettingsModal";
import SetupChecklist from "./components/SetupChecklist";
import StatsModal from "./components/StatsModal";
import ProgressToast from "./components/ProgressToast";
import UpdateBanner from "./components/UpdateBanner";
import DropZone from "./components/DropZone";
import { checkForUpdate, setGameRunning, type UpdateInfo } from "./update";

import LaunchboxShell from "./skins/LaunchboxShell";
import SwitchShell from "./skins/SwitchShell";
import SteamShell from "./skins/SteamShell";
import ArcShell from "./skins/ArcShell";
import BrowserShell from "./skins/BrowserShell";
import WiiShell from "./skins/WiiShell";
import { DEFAULT_SKIN, pickSkin, SKINS, type ShellProps, type SkinName } from "./skins/shell";
import {
  activateFocused,
  focusFirst,
  focusInDirection,
  resolveBindings,
  useGamepad,
  usePadCapture,
  type NavDirection,
  type PadBindings,
  type PadInfo,
  type PadLayout,
} from "./gamepad";

const ARROW_KEYS: Record<string, NavDirection | undefined> = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
};

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

/**
 * A callback whose identity never changes but which always runs the latest
 * version of `fn`.
 *
 * The tiles in every skin are memoised, which only helps if what they are
 * handed stays the same object from render to render; a fresh closure each
 * time made every tile in the library re-render on every press of the pad.
 */
function useStable<A extends unknown[], R>(fn: (...args: A) => R): (...args: A) => R {
  const ref = useRef(fn);
  ref.current = fn;
  return useCallback((...args: A) => ref.current(...args), []);
}

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
  const [setupStatus, setSetupStatus] = useState<SetupStatus | null>(null);
  const [setupDismissed, setSetupDismissed] = useState(false);
  const [settingsTab, setSettingsTab] = useState<Tab>("folders");
  const capturing = usePadCapture();

  // One timer each. A shared one meant a fetch finishing within a few seconds
  // of the scan cancelled the scan card's hiding, and it stayed up for good.
  const scanHide = useRef<number | null>(null);
  const scrapeHide = useRef<number | null>(null);

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
    // Whatever changed the library may have changed what is left to set up.
    void api.setupStatus().then(setSetupStatus).catch(() => setSetupStatus(null));
  }, []);

  useEffect(() => {
    void refreshGames();
  }, [refreshGames]);

  // A short note in the corner, for sync. Its own timer, so an older note's
  // never clears a newer one early.
  const noteHide = useRef<number | null>(null);
  const flashNote = useCallback((text: string) => {
    setDropNote(text);
    if (noteHide.current) window.clearTimeout(noteHide.current);
    noteHide.current = window.setTimeout(() => setDropNote(null), 8000);
  }, []);

  // Play time from other machines, once at startup. Sync being off, or its
  // folder not being there, are both ordinary, so neither is reported here.
  const refreshAfterSync = useStable(() => {
    void refreshGames();
    void refreshSidebar();
  });
  useEffect(() => {
    api
      .syncNow()
      .then((got) => {
        if (got.sessions === 0) return;
        refreshAfterSync();
        flashNote(
          `Added ${got.sessions} session${got.sessions === 1 ? "" : "s"} ` +
            `(${api.formatPlaytime(got.seconds)}) from your other machines.`,
        );
      })
      .catch(() => {});
  }, [refreshAfterSync, flashNote]);

  useEffect(() => {
    void refreshSidebar();
  }, [refreshSidebar]);

  // The chosen skin lives in the same settings table as everything else, so
  // it survives a restart without a second store.
  const loadSkin = useCallback(async () => {
    try {
      const [values, runtime] = await Promise.all([api.getSettings(), api.runtimeInfo()]);
      setSkin(pickSkin(values, runtime.gameMode));
      setSetupDismissed(values.setup_dismissed === "1");

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
          scheduleHide(scanHide, () => setScan(null));
        }
      }),
      listen<ScrapeProgress>("scrape-progress", (e) => {
        setScrape(e.payload);
        if (e.payload.done) {
          // Fetched artwork is written over any older file of the same name.
          api.bumpArtVersion();
          void refreshGames();
          void refreshSidebar();
          scheduleHide(scrapeHide, () => setScrape(null));
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
      listen<{ message: string }>("sync-note", (e) => flashNote(e.payload.message)),
    ];

    return () => {
      unlisteners.forEach((p) => void p.then((f) => f()));
    };
  }, [refreshGames, refreshSidebar, flashNote]);

  function scheduleHide(timer: React.MutableRefObject<number | null>, fn: () => void) {
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(fn, 4000);
  }

  // Installing an update restarts the app, which would lose a running game's
  // session; the updater refuses while this is set.
  useEffect(() => {
    setGameRunning(!!nowPlaying || !!unpacking);
  }, [nowPlaying, unpacking]);

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

  /**
   * Fetch metadata, outside `run`.
   *
   * A fetch can take hours on a big first import, and `run` holds the app busy
   * for as long as it goes - which disabled Play, Favourite and Rename the
   * whole time. The progress card carries its own Stop button, and its
   * progress is reported by event, so nothing needs to wait on it.
   */
  const fetchMetadata = async (from: string | null, refetch: boolean, quiet: boolean) => {
    try {
      const summary = await api.scrapeLibrary(from, refetch);
      setScrape((prev) => (prev ? { ...prev, done: true, status: summary } : prev));
    } catch (e) {
      // After a scan, every provider switched off is a setting, not a
      // failure worth a banner.
      if (quiet) {
        setScrape((prev) => (prev ? { ...prev, done: true, status: api.errorMessage(e) } : prev));
      } else {
        setError(errorMessage(e));
      }
    }
    await refreshGames();
  };

  const handleScan = async () => {
    const result = await run(async () => {
      const found = await api.scanLibrary();
      await refreshGames();
      await refreshSidebar();
      return found;
    });

    // A new game arrives with nothing but a filename, so go and get its
    // artwork and details straight away rather than making that a second
    // thing to remember. Only when the scan actually found something, and
    // only for entries that still need it — this re-runs no work.
    if (result && result.added > 0) await fetchMetadata(null, false, true);
  };

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

    flashNote(
      result.reasons.length > 0
        ? `${message(result)} — ${result.reasons[0]}`
        : message(result),
    );

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

  const handleScrape = () => fetchMetadata(platform, false, false);

  /**
   * Go back over everything, including games that already have artwork.
   *
   * Deliberately whole-library rather than honouring the current system
   * filter: it is reached from Settings, where the filter is out of sight, and
   * a button called "fetch everything" quietly doing one shelf would be a
   * worse surprise than doing what it says.
   */
  const handleRefetchAll = () => fetchMetadata(null, true, false);

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
      // Changed where it stands rather than fetching the whole library again,
      // which on a big one re-rendered every tile for one star. Only a list
      // of favourites actually changes shape.
      if (favoritesOnly) {
        await refreshGames();
      } else {
        setGames((all) => all.map((g) => (g.id === game.id ? { ...g, favorite: !game.favorite } : g)));
      }
      void refreshSidebar();
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

  // The keyboard does what a pad does: Escape is Back, and the arrows move
  // as the D-pad does. That is also what Steam Input sends from a Steam
  // Deck's controls under a keyboard layout. F11 is full screen.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F11") {
        e.preventDefault();
        void api.toggleFullscreen();
        return;
      }
      const el = document.activeElement;
      // Leave a text field alone; Escape there means "stop typing", and the
      // arrows move the cursor.
      if (
        el instanceof HTMLInputElement ||
        el instanceof HTMLTextAreaElement ||
        el instanceof HTMLSelectElement
      ) {
        return;
      }
      if (e.key === "Escape") {
        closeTopmost();
        return;
      }
      const dir = ARROW_KEYS[e.key];
      if (dir && !e.altKey && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        if (!focusInDirection(dir)) focusFirst();
      }
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
      // Likewise while the controller setup is listening for a button.
      enabled: !nowPlaying && !capturing,
    },
  );

  // ------------------------------------------------------------ rendering

  const stableLaunch = useStable(handleLaunch);
  const stableToggleFavorite = useStable(handleToggleFavorite);
  const stableOpenDetail = useStable((id: number) => {
    setSelectedId(id);
    setDetailOpen(true);
  });

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
    onLaunch: stableLaunch,
    onToggleFavorite: stableToggleFavorite,
    onOpenDetail: stableOpenDetail,
    registerBack: (handler) => {
      skinBack.current = handler;
    },
    onScan: handleScan,
    onScrape: handleScrape,
    onOpenSettings: () => openSettings("folders"),
    onOpenStats: () => setStatsOpen(true),
  };

  // On the desktop skin, picking a game *is* opening its panel. The console
  // and living-room skins show their own game page instead, so there the
  // panel is the "Options" / "Manage" button and nothing else.
  const showDetail = selected && (skin === "launchbox" || detailOpen);

  const openSettings = (tab: Tab) => {
    setSettingsTab(tab);
    setSettingsOpen(true);
  };

  const addFirstFolder = async () => {
    try {
      const path = await api.pickFolder();
      if (!path) return;
      await api.addLibraryFolder(path, null);
      await handleScan();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const showSetup =
    !!setupStatus &&
    !setupDismissed &&
    ((setupStatus.folders === 0 && setupStatus.games === 0) ||
      setupStatus.systemsWithoutEmulator.length > 0);

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
          onArtworkChanged={() => {
            api.bumpArtVersion();
            void refreshGames();
          }}
          busy={busy}
        />
      )}

      {settingsOpen && (
        <SettingsModal
          initialTab={settingsTab}
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
            // The game may be outside whatever the library is filtered to, so
            // the filters go first, then its panel opens. Selecting it alone
            // closed the window and did nothing visible.
            setStatsOpen(false);
            setPlatform(null);
            setSearch("");
            setFavoritesOnly(false);
            setUnscrapedOnly(false);
            setSelectedId(id);
            setDetailOpen(true);
          }}
        />
      )}

      <DropZone onDrop={handleDrop} />

      {dropNote && <div className="drop-note">{dropNote}</div>}

      <div className="corner-stack">
        {showSetup && setupStatus && (
          <SetupChecklist
            status={setupStatus}
            onAddFolder={() => void addFirstFolder()}
            onOpenSettings={openSettings}
            onDismiss={() => {
              setSetupDismissed(true);
              void api.saveSettings({ setup_dismissed: "1" });
            }}
          />
        )}
        {update && !updateDismissed && (
          <UpdateBanner info={update} onDismiss={() => setUpdateDismissed(true)} />
        )}
      </div>

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
