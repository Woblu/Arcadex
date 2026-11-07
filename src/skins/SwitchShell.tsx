/**
 * A handheld-console home screen.
 *
 * Drawn from scratch rather than copied — no borrowed artwork, icons or
 * typefaces — but the proportions are the ones that make a console home
 * screen read as one: a thin status bar, icons large enough to be the whole
 * point of the screen, and a row of small round system buttons along the
 * bottom. Nothing else competes with the games.
 *
 * Two views. The row is the home screen. "All software" is the grid you drop
 * into when the row gets long, which is also why the bottom buttons are not
 * five ways of opening the same panel: each goes somewhere different.
 */

import { memo, useEffect, useMemo, useRef, useState } from "react";

import { artUrl } from "../api";
import Logo from "../components/Logo";
import SystemIcon from "../components/SystemIcon";
import type { Game } from "../types";
import type { ShellProps } from "./shell";
import { bringIntoView } from "../gamepad";

export default function SwitchShell(props: ShellProps) {
  const {
    games,
    platforms,
    platform,
    search,
    selected,
    selectedId,
    error,
    isEmptyLibrary,
    padConnected,
    onSelectPlatform,
    onSearch,
    onSelect,
    onLaunch,
    onToggleFavorite,
    onOpenDetail,
    onOpenSettings,
    onOpenStats,
    onScan,
    registerBack,
  } = props;

  const [view, setView] = useState<"home" | "all">("home");
  const [searchOpen, setSearchOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement | null>(null);
  const clock = useClock();

  const systems = useMemo(
    () => platforms.filter((p) => p.gameCount > 0),
    [platforms],
  );

  // Keep the chosen icon in view wherever the selection came from.
  useEffect(() => {
    if (selectedId == null) return;
    const tile = document.querySelector<HTMLElement>(
      `.switch-shell [data-game-id="${selectedId}"]`,
    );
    // Not scrollIntoView: that walks up the tree scrolling any ancestor it
    // can, which is what used to drag the clock and the background sideways.
    if (tile) bringIntoView(tile);
  }, [selectedId, view]);

  // A search box that is always open would be one more thing on a screen that
  // is supposed to be only games, so it unfolds from its button.
  useEffect(() => {
    if (search) setSearchOpen(true);
  }, [search]);

  // Back leaves All software, and closes the search box, before it means
  // anything else. Both are views inside this skin that App cannot see, and
  // "go over and click Home" is not an answer when you are holding a pad.
  useEffect(() => {
    registerBack(() => {
      if (view === "all") {
        setView("home");
        return true;
      }
      if (searchOpen && !search) {
        setSearchOpen(false);
        return true;
      }
      return false;
    });
    return () => registerBack(null);
  }, [registerBack, view, searchOpen, search]);

  /**
   * The backdrop holds the last picture it had.
   *
   * A game with no artwork used to drop it to flat grey, which reads as the
   * screen having broken rather than as that one game having no cover. So the
   * room keeps the colour of the last game that had one, until another game
   * with artwork replaces it.
   *
   * Stored in state from an effect rather than written during render, and the
   * live value is preferred over the stored one, so arriving at a game that
   * does have art shows it on the same frame rather than a frame late.
   */
  const current = artUrl(selected?.coverPath ?? null);
  const [lastArt, setLastArt] = useState<string | null>(null);
  useEffect(() => {
    if (current) setLastArt(current);
  }, [current]);
  const backdrop = current ?? lastArt;

  return (
    <div className="switch-shell">
      {/* The art of whatever is selected, blown up and blurred past reading.
          A home screen that is only flat grey has nothing to look at between
          the icons, and this fills it with something that belongs to the game
          you are actually on. Keyed so it cross-fades rather than snapping. */}
      <div className="sw-backdrop" aria-hidden="true">
        {backdrop && (
          <img key={backdrop} src={backdrop} alt="" />
        )}
      </div>
      <div className="sw-vignette" aria-hidden="true" />

      <header className="sw-top">
        {/* Where a console puts your profile picture. Arcadex has no accounts,
            so it carries the app's own mark rather than an initial taken from
            whichever game happens to sort first. */}
        <div className="sw-profile">
          <Logo size={26} />
          <span className="sw-wordmark">Arcadex</span>
        </div>

        <div className="sw-top-right">
          {searchOpen ? (
            <input
              className="sw-search-input"
              value={search}
              placeholder="Search"
          aria-label="Search your library"
              autoFocus
              onChange={(e) => onSearch(e.target.value)}
              onBlur={() => !search && setSearchOpen(false)}
            />
          ) : (
            <button
              className="sw-status-btn"
              title="Search"
              aria-label="Search"
              onClick={() => setSearchOpen(true)}
            >
              <IconSearch />
            </button>
          )}

          <button
            className={`sw-status-btn ${platform === null ? "on" : ""}`}
            title="All systems"
            aria-label="All systems"
            onClick={() => onSelectPlatform(null)}
          >
            <IconAll />
          </button>
          {systems.map((p) => (
            <button
              key={p.slug}
              className={`sw-status-btn ${platform === p.slug ? "on" : ""}`}
              onClick={() => onSelectPlatform(p.slug)}
              title={p.name}
              aria-label={p.name}
            >
              <SystemIcon platform={p.slug} size={17} />
            </button>
          ))}

          <span className="sw-clock">{clock}</span>
        </div>
      </header>

      {error && <div className="error-banner sw-error">{error}</div>}

      {isEmptyLibrary ? (
        <div className="sw-stage">
          <div className="sw-row">
            <button className="sw-tile add" onClick={onOpenSettings} data-nav-default>
              <span className="sw-plus">+</span>
            </button>
          </div>
          <div className="sw-label">Add a ROM folder</div>
        </div>
      ) : view === "all" ? (
        <div className="sw-all">
          <div className="sw-all-head">
            <button className="sw-back" onClick={() => setView("home")}>
              ‹ Home
            </button>
            <span>All software · {games.length}</span>
          </div>
          <div className="sw-grid">
            {games.map((game) => (
              <Tile
                key={game.id}
                game={game}
                on={game.id === selectedId}
                onSelect={onSelect}
                onLaunch={onLaunch}
                onOpenDetail={onOpenDetail}
              />
            ))}
          </div>
        </div>
      ) : (
        <div className="sw-stage">
          <div className="sw-row" ref={rowRef}>
            {games.length === 0 ? (
              <div className="sw-nothing">Nothing matches that search.</div>
            ) : (
              games.map((game) => (
                <Tile
                  key={game.id}
                  game={game}
                  on={game.id === selectedId}
                  onSelect={onSelect}
                  onLaunch={onLaunch}
                  onOpenDetail={onOpenDetail}
                />
              ))
            )}
          </div>

          {/* The name sits under the icons as a caption, not a headline —
              the artwork is meant to be what you read. */}
          <div className="sw-label" key={`t-${selectedId ?? "none"}`}>
            {selected?.title ?? " "}
          </div>
          <div className="sw-sub" key={`s-${selectedId ?? "none"}`}>
            {selected
              ? [
                  selected.developer || selected.publisher,
                  selected.releaseDate?.slice(0, 4),
                  selected.playSeconds > 0
                    ? `Played ${Math.max(1, Math.round(selected.playSeconds / 60))} min`
                    : "Never played",
                ]
                  .filter(Boolean)
                  .join("   ·   ")
              : " "}
          </div>
        </div>
      )}

      <footer className="sw-bottom">
        <div className="sw-tray">
          <TrayButton
            label="All software"
            active={view === "all"}
            onClick={() => setView(view === "all" ? "home" : "all")}
          >
            <IconGrid />
          </TrayButton>
          <TrayButton label="Play history" onClick={onOpenStats}>
            <IconChart />
          </TrayButton>
          <TrayButton label="Scan folders" onClick={onScan}>
            <IconRefresh />
          </TrayButton>
          <TrayButton label="Settings" onClick={onOpenSettings}>
            <IconGear />
          </TrayButton>
        </div>

        {/* Either a legend or controls, never both. With a pad connected these
            are labels for buttons your thumb is already on, so they are inert
            text: hoverable prompts invite a click that the glyph is telling
            you to make on the controller, and as real buttons they also sat in
            the pad's own focus order, where the D-pad could land on the legend
            describing the D-pad. Without a pad the same corner carries actual
            buttons, since a mouse otherwise has no way to reach Options. */}
        {padConnected ? (
          <div className="sw-hints" aria-hidden="true">
            {selected && (
              <>
                <span className="sw-hint">
                  <span className="sw-glyph">A</span> Start
                </span>
                <span className="sw-hint">
                  <span className="sw-glyph">X</span> Options
                </span>
                <span className="sw-hint">
                  <span className="sw-glyph">Y</span>
                  {selected.favorite ? "Unfavourite" : "Favourite"}
                </span>
              </>
            )}
            <span className="sw-hint">
              <span className="sw-glyph">B</span> Back
            </span>
          </div>
        ) : (
          selected && (
            <div className="sw-hints">
              <button
                className="sw-action primary"
                onClick={() => onLaunch(selected.id)}
              >
                Start
              </button>
              <button
                className="sw-action"
                onClick={() => onOpenDetail(selected.id)}
              >
                Options
              </button>
              <button
                className="sw-action"
                onClick={() => onToggleFavorite(selected)}
                title={selected.favorite ? "Remove from favourites" : "Add to favourites"}
              >
                {selected.favorite ? "★" : "☆"}
              </button>
            </div>
          )
        )}
      </footer>
    </div>
  );
}

/** Memoised: only the tiles whose game or selection changed re-render as the pad moves. */
const Tile = memo(function Tile({
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
      className={`sw-tile ${on ? "on" : ""}`}
      data-game-id={game.id}
      {...(on ? { "data-nav-default": "" } : {})}
      onFocus={() => onSelect(game.id)}
      onClick={() => onSelect(game.id)}
      onDoubleClick={() => onLaunch(game.id)}
      // A game's options were reachable from a pad (X) and from the Options
      // button in the footer, but not from the tile itself, which is the one
      // thing a mouse user actually points at. Right-click is where anyone
      // looks for "more about this thing".
      onContextMenu={(e) => {
        e.preventDefault();
        onSelect(game.id);
        onOpenDetail(game.id);
      }}
      title={`${game.title}\nDouble-click to play, right-click for options`}
    >
      {cover ? (
        <img src={cover} alt="" loading="lazy" />
      ) : (
        <span className="sw-fallback">
          <SystemIcon platform={game.platform} size={40} />
          <span>{game.title}</span>
        </span>
      )}
      {game.favorite && <span className="sw-fav">★</span>}
    </button>
  );
});

function TrayButton({
  label,
  active,
  onClick,
  children,
}: {
  label: string;
  active?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      className={`sw-tray-btn ${active ? "on" : ""}`}
      onClick={onClick}
      title={label}
      aria-label={label}
    >
      {children}
      <span className="sw-tray-label">{label}</span>
    </button>
  );
}

/** The clock a console home screen always carries, to the minute. */
function useClock(): string {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = window.setInterval(() => setNow(new Date()), 30_000);
    return () => window.clearInterval(id);
  }, []);
  return now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/* Monochrome line icons, drawn here so the tray reads as one set. */

const stroke = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.7,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

function IconSearch() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" {...stroke}>
      <circle cx="11" cy="11" r="6.5" />
      <path d="M16 16l4.5 4.5" />
    </svg>
  );
}
function IconAll() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" {...stroke}>
      <circle cx="12" cy="12" r="8" />
    </svg>
  );
}
function IconGrid() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" {...stroke}>
      <rect x="4" y="4" width="6.5" height="6.5" rx="1.4" />
      <rect x="13.5" y="4" width="6.5" height="6.5" rx="1.4" />
      <rect x="4" y="13.5" width="6.5" height="6.5" rx="1.4" />
      <rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.4" />
    </svg>
  );
}
function IconChart() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" {...stroke}>
      <path d="M4 19.5h16" />
      <path d="M7 19.5V11" />
      <path d="M12 19.5V5.5" />
      <path d="M17 19.5v-5.5" />
    </svg>
  );
}
function IconRefresh() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" {...stroke}>
      <path d="M19 12a7 7 0 1 1-2.2-5.1" />
      <path d="M19.5 4.5V9H15" />
    </svg>
  );
}
function IconGear() {
  // Teeth kept short and tucked against the rim, or the spokes read as a
  // brightness control rather than a cog.
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" {...stroke}>
      <circle cx="12" cy="12" r="8.2" />
      <circle cx="12" cy="12" r="3.1" />
      <path d="M12 3.8v2.1M12 18.1v2.1M20.2 12h-2.1M5.9 12H3.8M17.8 6.2l-1.5 1.5M7.7 16.3l-1.5 1.5M17.8 17.8l-1.5-1.5M7.7 7.7L6.2 6.2" />
    </svg>
  );
}
