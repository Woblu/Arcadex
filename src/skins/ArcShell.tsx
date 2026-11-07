/**
 * A centred-menu layout, in the shape of the first generation of console
 * dashboards.
 *
 * Drawn here rather than borrowed: no lifted artwork, icons or typefaces.
 * What makes the shape read is the arrangement, and the arrangement is a
 * single column of large slabs down the middle of a dark screen, lit from one
 * side, with everything else kept out of the way. A list you move down, not a
 * grid you move around.
 *
 * The column is the whole interface, so the system strip above it is a filter
 * on that one list rather than a second place to be.
 */

import { useEffect, useMemo, useRef } from "react";

import { artUrl, formatPlaytime } from "../api";
import SystemIcon from "../components/SystemIcon";
import type { ShellProps } from "./shell";
import { bringIntoView } from "../gamepad";

export default function ArcShell(props: ShellProps) {
  const {
    games,
    platforms,
    platform,
    search,
    selected,
    selectedId,
    favoritesOnly,
    error,
    isEmptyLibrary,
    padConnected,
    onSelectPlatform,
    onToggleFavorites,
    onSearch,
    onSelect,
    onLaunch,
    onOpenDetail,
    onOpenSettings,
    onOpenStats,
    onScan,
  } = props;

  const listRef = useRef<HTMLDivElement | null>(null);

  const systems = useMemo(
    () => platforms.filter((p) => p.gameCount > 0),
    [platforms],
  );

  useEffect(() => {
    if (selectedId == null || !listRef.current) return;
    const row = listRef.current.querySelector<HTMLElement>(
      `[data-game-id="${selectedId}"]`,
    );
    if (row) bringIntoView(row);
  }, [selectedId]);

  const art = artUrl(selected?.coverPath ?? null);
  const shot =
    artUrl(selected?.screenshotPath ?? null) ?? artUrl(selected?.coverPath ?? null);

  return (
    <div className="arc-shell">
      {/* The light source. Everything else sits in the dark it leaves. */}
      <div className="arc-glow" aria-hidden="true" />
      {shot && (
        <div className="arc-wash" aria-hidden="true">
          <img src={shot} alt="" />
        </div>
      )}

      <header className="arc-top">
        <div className="arc-brand">Playdex</div>

        <div className="arc-systems">
          <button
            className={`arc-chip ${platform === null && !favoritesOnly ? "on" : ""}`}
            onClick={() => onSelectPlatform(null)}
            data-nav
          >
            All
          </button>
          <button
            className={`arc-chip ${favoritesOnly ? "on" : ""}`}
            onClick={onToggleFavorites}
            data-nav
          >
            Favourites
          </button>
          {systems.map((p) => (
            <button
              key={p.slug}
              className={`arc-chip ${platform === p.slug ? "on" : ""}`}
              onClick={() => onSelectPlatform(p.slug)}
              data-nav
              title={`${p.name} · ${p.gameCount}`}
            >
              <SystemIcon platform={p.slug} size={15} />
              <span>{p.name}</span>
            </button>
          ))}
        </div>

        <input
          className="arc-search"
          value={search}
          placeholder="Search"
          aria-label="Search your library"
          onChange={(e) => onSearch(e.target.value)}
          data-nav
        />
      </header>

      {error && <div className="error-banner arc-error">{error}</div>}

      <div className="arc-body">
        <div className="arc-column" ref={listRef}>
          {isEmptyLibrary ? (
            <div className="arc-empty">
              <h1>Your library is empty</h1>
              <p>Add the folders where your ROMs live, then run a scan.</p>
              <button
                className="arc-slab"
                onClick={onOpenSettings}
                data-nav
                data-nav-default
              >
                <span className="arc-slab-title">Add a ROM folder</span>
              </button>
            </div>
          ) : games.length === 0 ? (
            <div className="arc-empty">
              <p>Nothing matches that search.</p>
            </div>
          ) : (
            games.map((game) => {
              const on = game.id === selectedId;
              return (
                <button
                  key={game.id}
                  className={`arc-slab ${on ? "on" : ""}`}
                  data-game-id={game.id}
                  data-nav
                  {...(on ? { "data-nav-default": "" } : {})}
                  onFocus={() => onSelect(game.id)}
                  onClick={() => onSelect(game.id)}
                  onDoubleClick={() => onLaunch(game.id)}
                >
                  <SystemIcon platform={game.platform} size={18} />
                  <span className="arc-slab-title">{game.title}</span>
                  {game.favorite && <span className="arc-star">★</span>}
                </button>
              );
            })
          )}
        </div>

        <aside className="arc-side">
          {selected ? (
            <>
              <div className="arc-art">
                {art ? (
                  <img src={art} alt="" />
                ) : (
                  <SystemIcon platform={selected.platform} size={54} />
                )}
              </div>
              <h2 className="arc-title">{selected.title}</h2>
              <p className="arc-meta">
                {[
                  selected.developer,
                  selected.releaseDate?.slice(0, 4),
                  selected.genre,
                ]
                  .filter(Boolean)
                  .join("  ·  ")}
              </p>
              <p className="arc-played">{formatPlaytime(selected.playSeconds)}</p>
              <div className="arc-actions">
                <button
                  className="arc-play"
                  onClick={() => onLaunch(selected.id)}
                  data-nav
                >
                  Play
                </button>
                <button
                  className="arc-secondary"
                  onClick={() => onOpenDetail(selected.id)}
                  data-nav
                >
                  Manage
                </button>
              </div>
            </>
          ) : (
            <p className="arc-meta">{games.length} games</p>
          )}

          <div className="arc-foot">
            <button className="arc-secondary" onClick={onScan} data-nav>
              Scan
            </button>
            <button className="arc-secondary" onClick={onOpenStats} data-nav>
              Stats
            </button>
            <button className="arc-secondary" onClick={onOpenSettings} data-nav>
              Settings
            </button>
          </div>
        </aside>
      </div>

      {padConnected && (
        <div className="arc-hints">
          <span>
            <b>A</b> Play
          </span>
          <span>
            <b>X</b> Manage
          </span>
          <span>
            <b>Y</b> Favourite
          </span>
          <span>
            <b>LB/RB</b> System
          </span>
        </div>
      )}
    </div>
  );
}
