/**
 * A channel grid, in the shape of the 2006 living-room console menu.
 *
 * Everything here is drawn in our own CSS — no borrowed artwork, icons or
 * typefaces. What makes the shape read is a short list of things, and they
 * are worth naming because each one is load-bearing:
 *
 *   - It is *light*. Every other skin in this app is dark, and that menu's
 *     most memorable quality is that it was a bright white room.
 *   - A fixed 4x3 grid, paged. Not a scrolling wall of everything you own.
 *     The page never reflows, so the shelf you learned stays where it was.
 *   - Empty slots are drawn. A half-full page shows the sockets the missing
 *     channels would sit in, which is the single strongest cue of the lot and
 *     the thing a plain responsive grid throws away.
 *   - Selection is a soft blue glow that sits *outside* the tile, and the
 *     tile lifts a little. Not a border, not a colour swap.
 *   - A silver tray along the bottom holding a round brand button, the clock
 *     in the middle, and one squarer button at each end.
 *   - Big round page arrows outside the grid, vertically centred.
 *
 * The system filter and search have no equivalent on the hardware, so they
 * are kept deliberately quiet along the top: the grid is supposed to be the
 * whole screen.
 *
 * Paging is a view inside the skin, so Back returns to the first page before
 * the app starts closing anything.
 */

import { memo, useEffect, useMemo, useRef, useState } from "react";

import { artUrl, formatPlaytime } from "../api";
import SystemIcon from "../components/SystemIcon";
import type { Game } from "../types";
import type { ShellProps } from "./shell";

/** 4 across, 3 down. The number is the layout, so it is not configurable. */
const PER_PAGE = 12;

export default function WiiShell(props: ShellProps) {
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
    registerBack,
  } = props;

  const [page, setPage] = useState(0);
  const clock = useClock();
  const gridRef = useRef<HTMLDivElement | null>(null);

  const systems = useMemo(
    () => platforms.filter((p) => p.gameCount > 0),
    [platforms],
  );

  const pageCount = Math.max(1, Math.ceil(games.length / PER_PAGE));
  const safePage = Math.min(page, pageCount - 1);

  // A filter that shortens the library should not leave you stranded on a
  // page that no longer exists.
  useEffect(() => {
    setPage(0);
  }, [platform, favoritesOnly, search]);

  /**
   * Turn to a page, and take the selection with you.
   *
   * The two have to move together. Paging without moving the selection leaves
   * the tray describing a channel that is not on screen, and — worse — leaves
   * the effect below with a disagreement to resolve, which it resolves by
   * turning the page straight back. Selecting the first channel of the page
   * you asked for means there is never a disagreement in the first place.
   */
  const turnTo = (next: number) => {
    const target = Math.min(Math.max(next, 0), pageCount - 1);
    setPage(target);
    const first = games[target * PER_PAGE];
    if (first) onSelect(first.id);
  };

  // Follow the selection when it lands on another page, which happens when the
  // pad walks off the end of a row, or when a search leaves the selected game
  // somewhere else entirely.
  //
  // Keyed on the selection actually changing, not on the page: an earlier
  // version listed the page in its dependencies, so turning a page re-ran this,
  // found the selection still on the page before, and turned it back. The
  // arrows appeared to do nothing at all.
  //
  // The list changing counts too. A new filter turns back to the first page,
  // and a selection that survived the filter on a later page was left there,
  // with the tray and Start acting on a channel that was not on screen.
  const followed = useRef<number | null>(null);
  const followedGames = useRef(games);
  useEffect(() => {
    if (selectedId == null) return;
    if (followed.current === selectedId && followedGames.current === games) return;
    followed.current = selectedId;
    followedGames.current = games;
    const index = games.findIndex((g) => g.id === selectedId);
    if (index < 0) return;
    setPage(Math.floor(index / PER_PAGE));
  }, [selectedId, games]);

  useEffect(() => {
    registerBack(() => {
      if (safePage > 0) {
        turnTo(0);
        return true;
      }
      return false;
    });
    return () => registerBack(null);
  }, [registerBack, safePage]);

  const start = safePage * PER_PAGE;
  const shown = games.slice(start, start + PER_PAGE);
  // The sockets a full page would have. Drawing them is the point.
  const blanks = Math.max(0, PER_PAGE - shown.length);

  return (
    <div className="wii-shell">
      <div className="wii-room" aria-hidden="true" />

      <header className="wii-top">
        <div className="wii-pills">
          <button
            className={`wii-pill ${platform === null && !favoritesOnly ? "on" : ""}`}
            onClick={() => onSelectPlatform(null)}
            data-nav
          >
            All
          </button>
          <button
            className={`wii-pill ${favoritesOnly ? "on" : ""}`}
            onClick={onToggleFavorites}
            data-nav
          >
            Favourites
          </button>
          {systems.map((p) => (
            <button
              key={p.slug}
              className={`wii-pill ${platform === p.slug ? "on" : ""}`}
              onClick={() => onSelectPlatform(p.slug)}
              data-nav
            >
              <SystemIcon platform={p.slug} size={14} />
              <span>{p.name}</span>
            </button>
          ))}
        </div>
        <input
          className="wii-search"
          value={search}
          placeholder="Search"
          aria-label="Search your library"
          onChange={(e) => onSearch(e.target.value)}
          data-nav
        />
      </header>

      {error && <div className="error-banner wii-error">{error}</div>}

      <div className="wii-stage">
        <button
          className="wii-arrow left"
          onClick={() => turnTo(safePage - 1)}
          disabled={safePage === 0}
          aria-label="Previous page"
          data-nav
        >
          <Chevron dir="left" />
        </button>

        <div className="wii-grid-wrap">
          {isEmptyLibrary ? (
            <div className="wii-empty">
              <h1>Your library is empty</h1>
              <p>Add the folders where your ROMs live, then run a scan.</p>
              <button
                className="wii-cta"
                onClick={onOpenSettings}
                data-nav
                data-nav-default
              >
                Add a ROM folder
              </button>
            </div>
          ) : games.length === 0 ? (
            <div className="wii-empty">
              <p>Nothing matches that search.</p>
            </div>
          ) : (
            <div className="wii-grid" ref={gridRef}>
              {shown.map((game) => (
                <Channel
                  key={game.id}
                  game={game}
                  on={game.id === selectedId}
                  onSelect={onSelect}
                  onLaunch={onLaunch}
                  onOpenDetail={onOpenDetail}
                />
              ))}
              {Array.from({ length: blanks }, (_, i) => (
                <div className="wii-channel empty" key={`blank-${i}`} aria-hidden="true">
                  <span className="wii-socket" />
                </div>
              ))}
            </div>
          )}

          <div className="wii-pager">
            {Array.from({ length: pageCount }, (_, i) => (
              <span key={i} className={`wii-dot ${i === safePage ? "on" : ""}`} />
            ))}
          </div>
        </div>

        <button
          className="wii-arrow right"
          onClick={() => turnTo(safePage + 1)}
          disabled={safePage >= pageCount - 1}
          aria-label="Next page"
          data-nav
        >
          <Chevron dir="right" />
        </button>
      </div>

      {padConnected && (
        <div className="wii-hints">
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
            <b>B</b> First page
          </span>
        </div>
      )}

      {/* The tray. The clock sits dead centre, which is most of why the bar
          reads the way it does, so the two side groups take equal width
          rather than the clock simply following whatever is to its left. */}
      <footer className="wii-tray">
        <div className="wii-tray-side">
          <button
            className="wii-orb"
            onClick={onOpenSettings}
            data-nav
            title="Settings"
          >
            <span>Playdex</span>
          </button>
          <button className="wii-slot" onClick={onScan} data-nav>
            Scan
          </button>
        </div>

        <div className="wii-clock">
          <span className="wii-time">{clock.time}</span>
          <span className="wii-date">{clock.date}</span>
        </div>

        <div className="wii-tray-side right">
          <div className="wii-now">
            {selected ? (
              <>
                <span className="wii-now-title">{selected.title}</span>
                <span className="wii-now-sub">
                  {formatPlaytime(selected.playSeconds)}
                </span>
              </>
            ) : (
              <span className="wii-now-sub">{games.length} channels</span>
            )}
          </div>
          <button className="wii-slot" onClick={onOpenStats} data-nav>
            History
          </button>
        </div>
      </footer>

    </div>
  );
}

/** Memoised: only the tiles whose game or selection changed re-render as the pad moves. */
const Channel = memo(function Channel({
  game,
  on,
  onSelect,
  onLaunch,
  onOpenDetail,
}: {
  game: Game;
  on: boolean;
  onSelect: (id: number) => void;
  onLaunch: (id: number) => void;
  onOpenDetail: (id: number) => void;
}) {
  const cover = artUrl(game.coverPath);
  return (
    <button
      className={`wii-channel ${on ? "on" : ""}`}
      data-game-id={game.id}
      data-nav
      {...(on ? { "data-nav-default": "" } : {})}
      onFocus={() => onSelect(game.id)}
      onClick={() => onSelect(game.id)}
      onDoubleClick={() => onLaunch(game.id)}
      onContextMenu={(e) => {
        e.preventDefault();
        onOpenDetail(game.id);
      }}
      title={game.title}
    >
      <span className="wii-face">
        {cover ? (
          <img src={cover} alt="" loading="lazy" />
        ) : (
          <span className="wii-fallback">
            <SystemIcon platform={game.platform} size={28} />
          </span>
        )}
        {/* The gloss. A flat white square reads as a card; this reads as a
            moulded plastic face, which is the whole difference. */}
        <span className="wii-gloss" aria-hidden="true" />
      </span>
      <span className="wii-name">{game.title}</span>
      {game.favorite && <span className="wii-fav" aria-hidden="true">★</span>}
    </button>
  );
});

function Chevron({ dir }: { dir: "left" | "right" }) {
  return (
    <svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true">
      <path
        d={dir === "left" ? "M15 5 L8 12 L15 19" : "M9 5 L16 12 L9 19"}
        fill="none"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function useClock(): { time: string; date: string } {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = window.setInterval(() => setNow(new Date()), 30_000);
    return () => window.clearInterval(id);
  }, []);
  return {
    time: now.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }),
    date: now.toLocaleDateString([], { weekday: "short", month: "numeric", day: "numeric" }),
  };
}
