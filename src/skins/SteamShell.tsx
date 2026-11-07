/**
 * A living-room layout.
 *
 * Big-Picture-shaped rather than Big-Picture-copied: a hero band that fills
 * with the selected game's own artwork, a left rail of collections, and rows
 * of wide capsules underneath. Sized for a screen you are sitting back from —
 * larger type, fewer things, and a focus ring you can see across a room.
 */

import { memo, useEffect, useMemo, useRef } from "react";

import { artUrl } from "../api";
import SystemIcon from "../components/SystemIcon";
import type { Game } from "../types";
import type { ShellProps } from "./shell";
import { bringIntoView } from "../gamepad";

export default function SteamShell(props: ShellProps) {
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

  const shelfRef = useRef<HTMLDivElement | null>(null);

  const systems = useMemo(
    () => platforms.filter((p) => p.gameCount > 0),
    [platforms],
  );

  useEffect(() => {
    if (selectedId == null || !shelfRef.current) return;
    const capsule = shelfRef.current.querySelector<HTMLElement>(
      `[data-game-id="${selectedId}"]`,
    );
    if (capsule) bringIntoView(capsule);
  }, [selectedId]);

  const hero =
    artUrl(selected?.screenshotPath ?? null) ?? artUrl(selected?.coverPath ?? null);

  return (
    <div className="steam-shell">
      <aside className="st-rail">
        <div className="st-brand">Playdex</div>

        <button
          className={`st-rail-btn ${platform === null && !favoritesOnly ? "on" : ""}`}
          onClick={() => onSelectPlatform(null)}
          data-nav
        >
          All games
        </button>
        <button
          className={`st-rail-btn ${favoritesOnly ? "on" : ""}`}
          onClick={onToggleFavorites}
          data-nav
        >
          Favourites
        </button>

        <div className="st-rail-head">Systems</div>
        <div className="st-rail-scroll">
          {systems.map((p) => (
            <button
              key={p.slug}
              className={`st-rail-btn ${platform === p.slug ? "on" : ""}`}
              onClick={() => onSelectPlatform(p.slug)}
              data-nav
            >
              <SystemIcon platform={p.slug} size={16} />
              <span className="st-rail-label">{p.name}</span>
              <span className="st-rail-count">{p.gameCount}</span>
            </button>
          ))}
        </div>

        <div className="st-rail-foot">
          <button className="st-rail-btn" onClick={onScan} data-nav>
            Scan
          </button>
          <button className="st-rail-btn" onClick={onOpenStats} data-nav>
            Stats
          </button>
          <button className="st-rail-btn" onClick={onOpenSettings} data-nav>
            Settings
          </button>
        </div>
      </aside>

      <main className="st-main">
        <div className="st-hero">
          {hero && <img className="st-hero-art" src={hero} alt="" />}
          <div className="st-hero-wash" />

          <div className="st-hero-body">
            {isEmptyLibrary ? (
              <>
                <h1>Your library is empty</h1>
                <p>Add the folders where your ROMs live, then run a scan.</p>
                <button
                  className="st-play"
                  onClick={onOpenSettings}
                  data-nav
                  data-nav-default
                >
                  Add a ROM folder
                </button>
              </>
            ) : selected ? (
              <>
                <h1>{selected.title}</h1>
                <p className="st-hero-meta">
                  {[
                    selected.developer,
                    selected.releaseDate?.slice(0, 4),
                    selected.genre,
                    selected.playSeconds > 0
                      ? `${Math.max(1, Math.round(selected.playSeconds / 60))} min played`
                      : "Never played",
                  ]
                    .filter(Boolean)
                    .join("  ·  ")}
                </p>
                <div className="st-hero-actions">
                  <button
                    className="st-play"
                    onClick={() => onLaunch(selected.id)}
                    data-nav
                  >
                    ▶  Play
                  </button>
                  <button
                    className="st-secondary"
                    onClick={() => onOpenDetail(selected.id)}
                    data-nav
                  >
                    Manage
                  </button>
                </div>
              </>
            ) : (
              <>
                <h1>{games.length} games</h1>
                <p>Pick something from the shelf below.</p>
              </>
            )}
          </div>

          <div className="st-search">
            <input
              value={search}
              placeholder="Search"
          aria-label="Search your library"
              onChange={(e) => onSearch(e.target.value)}
              data-nav
            />
          </div>
        </div>

        {error && <div className="error-banner st-error">{error}</div>}

        <div className="st-shelf" ref={shelfRef}>
          {games.length === 0 && !isEmptyLibrary ? (
            <div className="st-nothing">Nothing matches that search.</div>
          ) : (
            games.map((game) => (
              <Capsule
                key={game.id}
                game={game}
                on={game.id === selectedId}
                onSelect={onSelect}
                onLaunch={onLaunch}
              />
            ))
          )}
        </div>

        {padConnected && (
          <div className="st-hints">
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
              <b>B</b> Back
            </span>
            <span>
              <b>LB/RB</b> System
            </span>
          </div>
        )}
      </main>
    </div>
  );
}

/** Memoised: only the tiles whose game or selection changed re-render as the pad moves. */
const Capsule = memo(function Capsule({
  game,
  on,
  onSelect,
  onLaunch,
}: {
  game: Game;
  on: boolean;
  onSelect: (id: number) => void;
  onLaunch: (id: number) => void;
}) {
  const cover = artUrl(game.coverPath);
  return (
    <button
      className={`st-capsule ${on ? "on" : ""}`}
      data-game-id={game.id}
      data-nav
      {...(on ? { "data-nav-default": "" } : {})}
      onFocus={() => onSelect(game.id)}
      onClick={() => onSelect(game.id)}
      onDoubleClick={() => onLaunch(game.id)}
      title={game.title}
    >
      {cover ? (
        <img src={cover} alt="" loading="lazy" />
      ) : (
        <span className="st-capsule-fallback">
          <SystemIcon platform={game.platform} size={26} />
          <span>{game.title}</span>
        </span>
      )}
      {game.favorite && <span className="st-fav">★</span>}
    </button>
  );
});
