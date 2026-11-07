/**
 * The original Arcadex layout: sidebar, top bar, grid.
 *
 * Lifted out of `App` unchanged so it is one skin among three rather than the
 * only thing the app can look like.
 */

import Sidebar from "../components/Sidebar";
import TopBar from "../components/TopBar";
import GameGrid from "../components/GameGrid";
import type { ShellProps } from "./shell";

export default function LaunchboxShell(props: ShellProps) {
  const {
    games,
    platforms,
    stats,
    platform,
    search,
    sort,
    favoritesOnly,
    unscrapedOnly,
    selectedId,
    busy,
    error,
    isEmptyLibrary,
    onSelectPlatform,
    onToggleFavorites,
    onToggleUnscraped,
    onSearch,
    onSort,
    onSelect,
    onLaunch,
    onScan,
    onScrape,
    onOpenSettings,
    onOpenStats,
  } = props;

  return (
    <div className="app">
      <Sidebar
        platforms={platforms}
        stats={stats}
        selected={platform}
        favoritesOnly={favoritesOnly}
        unscrapedOnly={unscrapedOnly}
        onSelectPlatform={onSelectPlatform}
        onToggleFavorites={onToggleFavorites}
        onToggleUnscraped={onToggleUnscraped}
        onOpenSettings={onOpenSettings}
        onOpenStats={onOpenStats}
      />

      <div className="main">
        <TopBar
          search={search}
          onSearch={onSearch}
          sort={sort}
          onSort={onSort}
          busy={busy}
          onScan={onScan}
          onScrape={onScrape}
          count={games.length}
        />

        <div className="content">
          {error && <div className="error-banner">{error}</div>}

          {isEmptyLibrary ? (
            <div className="empty">
              <div className="empty-inner">
                <h2>Your library is empty</h2>
                <p>
                  Add the folders where your ROMs live, then run a scan. Arcadex
                  reads what is already on your disk — it identifies each file
                  by hash and fetches cover art and details for it.
                </p>
                <button
                  className="btn primary"
                  data-nav
                  data-nav-default
                  onClick={onOpenSettings}
                >
                  Add a ROM folder
                </button>
              </div>
            </div>
          ) : (
            <GameGrid
              games={games}
              selectedId={selectedId}
              onSelect={onSelect}
              onLaunch={onLaunch}
            />
          )}
        </div>
      </div>
    </div>
  );
}
