/**
 * A browser layout: a column of cards standing in a slow-drifting field.
 *
 * The shape of an early-2000s disc console's front end, rebuilt in our own
 * CSS — no borrowed artwork or typefaces. Two things carry it: the field
 * behind, which moves just enough to stop the screen feeling static, and the
 * restraint in front of it. Thin letter-spaced type, one column, and a lot of
 * space doing nothing, because that is what made those menus feel unhurried.
 *
 * The drifting quads are decoration and nothing depends on them, so they are
 * dropped entirely for anyone who has asked for reduced motion.
 */

import { memo, useEffect, useMemo, useRef } from "react";

import { artUrl, formatPlaytime } from "../api";
import SystemIcon from "../components/SystemIcon";
import type { Game } from "../types";
import type { ShellProps } from "./shell";
import { bringIntoView } from "../gamepad";

/** Fixed so the field is the same every launch rather than jittering on each render. */
const QUADS = [
  { left: "8%", top: "14%", size: 120, delay: 0, dur: 26 },
  { left: "22%", top: "62%", size: 76, delay: 4, dur: 34 },
  { left: "39%", top: "28%", size: 156, delay: 9, dur: 30 },
  { left: "58%", top: "72%", size: 96, delay: 2, dur: 38 },
  { left: "71%", top: "18%", size: 132, delay: 12, dur: 28 },
  { left: "86%", top: "52%", size: 84, delay: 6, dur: 36 },
];

export default function BrowserShell(props: ShellProps) {
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

  const columnRef = useRef<HTMLDivElement | null>(null);

  const systems = useMemo(
    () => platforms.filter((p) => p.gameCount > 0),
    [platforms],
  );

  // Systems by their real names, in the facts list and on every card. The
  // cards used to show the internal slug, which in this skin's capitals read
  // as a code - XBOX360, PS3 - rather than as anything a person would say.
  const systemName = useMemo(() => {
    const bySlug = new Map(platforms.map((p) => [p.slug, p.name]));
    return (slug: string) => bySlug.get(slug) ?? slug;
  }, [platforms]);

  useEffect(() => {
    if (selectedId == null || !columnRef.current) return;
    const card = columnRef.current.querySelector<HTMLElement>(
      `[data-game-id="${selectedId}"]`,
    );
    if (card) bringIntoView(card);
  }, [selectedId]);

  return (
    <div className="br-shell">
      <div className="br-field" aria-hidden="true">
        {QUADS.map((q, i) => (
          <span
            key={i}
            className="br-quad"
            style={{
              left: q.left,
              top: q.top,
              width: q.size,
              height: q.size,
              animationDelay: `-${q.delay}s`,
              animationDuration: `${q.dur}s`,
            }}
          />
        ))}
      </div>

      <header className="br-top">
        <span className="br-brand">Playdex</span>
        <div className="br-filters">
          <button
            className={`br-filter ${platform === null && !favoritesOnly ? "on" : ""}`}
            onClick={() => onSelectPlatform(null)}
            data-nav
          >
            All
          </button>
          <button
            className={`br-filter ${favoritesOnly ? "on" : ""}`}
            onClick={onToggleFavorites}
            data-nav
          >
            Favourites
          </button>
          {systems.map((p) => (
            <button
              key={p.slug}
              className={`br-filter ${platform === p.slug ? "on" : ""}`}
              onClick={() => onSelectPlatform(p.slug)}
              data-nav
            >
              {p.name}
            </button>
          ))}
        </div>
        <input
          className="br-search"
          value={search}
          placeholder="Search"
          aria-label="Search your library"
          onChange={(e) => onSearch(e.target.value)}
          data-nav
        />
      </header>

      {error && <div className="error-banner br-error">{error}</div>}

      <div className="br-body">
        <div className="br-column" ref={columnRef}>
          {isEmptyLibrary ? (
            <div className="br-empty">
              <h1>Your library is empty</h1>
              <p>Add the folders where your ROMs live, then run a scan.</p>
              <button
                className="br-action"
                onClick={onOpenSettings}
                data-nav
                data-nav-default
              >
                Add a ROM folder
              </button>
            </div>
          ) : games.length === 0 ? (
            <div className="br-empty">Nothing matches that search.</div>
          ) : (
            games.map((game) => (
              <Card
                key={game.id}
                game={game}
                system={systemName(game.platform)}
                on={game.id === selectedId}
                onSelect={onSelect}
                onLaunch={onLaunch}
              />
            ))
          )}
        </div>

        <aside className="br-panel">
          {selected ? (
            <>
              <h2 className="br-title">{selected.title}</h2>
              <div className="br-rule" />
              <dl className="br-facts">
                <Fact label="System" value={systemName(selected.platform)} />
                <Fact label="Developer" value={selected.developer} />
                <Fact label="Released" value={selected.releaseDate?.slice(0, 4) ?? null} />
                <Fact label="Genre" value={selected.genre} />
                <Fact label="Players" value={selected.players} />
                <Fact label="Played" value={formatPlaytime(selected.playSeconds)} />
              </dl>
              <div className="br-actions">
                <button
                  className="br-action"
                  onClick={() => onLaunch(selected.id)}
                  data-nav
                >
                  Play
                </button>
                <button
                  className="br-action ghost"
                  onClick={() => onOpenDetail(selected.id)}
                  data-nav
                >
                  Manage
                </button>
              </div>
            </>
          ) : (
            <p className="br-facts">{games.length} games</p>
          )}

          <div className="br-foot">
            <button className="br-action ghost" onClick={onScan} data-nav>
              Scan
            </button>
            <button className="br-action ghost" onClick={onOpenStats} data-nav>
              Stats
            </button>
            <button className="br-action ghost" onClick={onOpenSettings} data-nav>
              Settings
            </button>
          </div>
        </aside>
      </div>

      {padConnected && (
        <div className="br-hints">
          <span>
            <b>A</b> Play
          </span>
          <span>
            <b>X</b> Manage
          </span>
          <span>
            <b>Y</b> Favourite
          </span>
        </div>
      )}
    </div>
  );
}

function Fact({ label, value }: { label: string; value: string | null }) {
  if (!value) return null;
  return (
    <>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </>
  );
}

/** Memoised: only the tiles whose game or selection changed re-render as the pad moves. */
const Card = memo(function Card({
  game,
  system,
  on,
  onSelect,
  onLaunch,
}: {
  game: Game;
  system: string;
  on: boolean;
  onSelect: (id: number) => void;
  onLaunch: (id: number) => void;
}) {
  const cover = artUrl(game.coverPath);
  return (
    <button
      className={`br-card ${on ? "on" : ""}`}
      data-game-id={game.id}
      data-nav
      {...(on ? { "data-nav-default": "" } : {})}
      onFocus={() => onSelect(game.id)}
      onClick={() => onSelect(game.id)}
      onDoubleClick={() => onLaunch(game.id)}
    >
      <span className="br-card-art">
        {cover ? (
          <img src={cover} alt="" loading="lazy" />
        ) : (
          <SystemIcon platform={game.platform} size={20} />
        )}
      </span>
      <span className="br-card-text">
        <span className="br-card-title">{game.title}</span>
        <span className="br-card-sub">{system}</span>
      </span>
      {game.favorite && <span className="br-star">★</span>}
    </button>
  );
});
