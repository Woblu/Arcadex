import { Fragment, useEffect, useState } from "react";

import * as api from "../api";
import { artUrl, errorMessage, formatDate, formatPlaytime, formatSize } from "../api";
import type { Game, PlatformInfo } from "../types";
import CheatsSection from "./detail/CheatsSection";
import EmulatorSection from "./detail/EmulatorSection";
import FileSection from "./detail/FileSection";
import HacksSection from "./detail/HacksSection";
import SavesSection from "./detail/SavesSection";
import SystemIcon from "./SystemIcon";

interface Props {
  game: Game;
  busy: boolean;
  onClose: () => void;
  onLaunch: () => void;
  onToggleFavorite: () => void;
  onScrape: () => Promise<string | undefined>;
  onRemove: () => void;
  onSetPlatform: (slug: string) => void;
  onHackAdded: () => void;
  /** The cover changed, so the grid and the shells need the new one. */
  onArtworkChanged: () => void;
}

/**
 * Everything about one game.
 *
 * The heading, the actions and the details live here; the longer sections -
 * the file, the emulator, hacks, cheats and saves - are components of their
 * own in `detail/`. Each is keyed by the game and its system, so opening a
 * different game, or moving this one to another system, starts every
 * section afresh rather than carrying a half-finished patch or a search over.
 */
export default function GameDetail({
  game,
  busy,
  onClose,
  onLaunch,
  onToggleFavorite,
  onScrape,
  onRemove,
  onSetPlatform,
  onHackAdded,
  onArtworkChanged,
}: Props) {
  const [allPlatforms, setAllPlatforms] = useState<PlatformInfo[]>([]);
  const [command, setCommand] = useState<string | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [artNote, setArtNote] = useState<string | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [draftTitle, setDraftTitle] = useState(game.title);
  const [borrowList, setBorrowList] = useState<Game[] | null>(null);
  const [borrowFrom, setBorrowFrom] = useState<number | "">("");
  /** The discs behind a multi-disc entry. Empty for an ordinary game. */
  const [discs, setDiscs] = useState<Game[]>([]);
  const [scrapeNote, setScrapeNote] = useState<string | null>(null);

  // A different game opened in the same panel must not inherit the half-typed
  // name or the open picker from the one before.
  useEffect(() => {
    setRenaming(false);
    setDraftTitle(game.title);
    setBorrowList(null);
    setBorrowFrom("");
  }, [game.id, game.title]);

  useEffect(() => {
    void api.knownPlatforms().then(setAllPlatforms);
  }, []);

  /// Show what will actually run, so a bad emulator path is visible up front.
  const refreshCommand = async () => {
    try {
      setCommandError(null);
      setCommand(await api.previewLaunch(game.id));
    } catch (e) {
      setCommandError(errorMessage(e));
      setCommand(null);
    }
  };

  useEffect(() => {
    setCommand(null);
    setScrapeNote(null);
    void refreshCommand();
    api.discMembers(game.id).then(setDiscs).catch(() => setDiscs([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [game.id, game.platform]);

  const hero = artUrl(game.screenshotPath) ?? artUrl(game.coverPath);
  const sectionKey = `${game.id}-${game.platform}`;

  const rows: Array<[string, string | null]> = [
    ["System", api.platformName(allPlatforms, game.platform)],
    ["Developer", game.developer],
    ["Publisher", game.publisher],
    ["Released", game.releaseDate],
    ["Genre", game.genre],
    ["Players", game.players],
    ["Region", game.region],
    ["Serial", game.serial],
    ["Rating", game.rating ? `${game.rating.toFixed(1)} / 5` : null],
    ["Playtime", formatPlaytime(game.playSeconds)],
    ["Last played", game.lastPlayed ? formatDate(game.lastPlayed) : "Never"],
    ["Size", formatSize(game.size)],
    ["Metadata", game.scrapeSource ?? "not fetched"],
  ];

  return (
    <aside className="detail">
      <div className="detail-hero">
        <button className="detail-close" onClick={onClose} aria-label="Close">
          ✕
        </button>
        {hero && <img src={hero} alt="" />}
      </div>

      <div className="detail-body">
        {renaming ? (
          <form
            className="detail-rename"
            onSubmit={async (e) => {
              e.preventDefault();
              setCommandError(null);
              try {
                await api.renameGame(game.id, draftTitle);
                setRenaming(false);
                // Refreshes the library, which is what carries the new name
                // back into this panel and every appearance.
                onArtworkChanged();
              } catch (err) {
                setCommandError(errorMessage(err));
              }
            }}
          >
            <input
              className="detail-rename-input"
              value={draftTitle}
              autoFocus
              onChange={(e) => setDraftTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.stopPropagation();
                  setRenaming(false);
                  setDraftTitle(game.title);
                }
              }}
              aria-label="Game name"
            />
            <button className="btn primary" type="submit" disabled={busy}>
              Save
            </button>
            <button
              className="btn"
              type="button"
              onClick={() => {
                setRenaming(false);
                setDraftTitle(game.title);
              }}
            >
              Cancel
            </button>
          </form>
        ) : (
          <div className="detail-title-row">
            <h1 className="detail-title">{game.title}</h1>
            <button className="btn small" onClick={() => setRenaming(true)} disabled={busy}>
              Rename
            </button>
          </div>
        )}

        <div className="detail-actions">
          <button className="btn primary" onClick={onLaunch} disabled={busy}>
            ▶ Play
          </button>
          <button className="btn" onClick={onToggleFavorite} disabled={busy}>
            {game.favorite ? "★ Favorited" : "☆ Favorite"}
          </button>
          <button
            className="btn"
            disabled={busy}
            onClick={async () => {
              const note = await onScrape();
              if (note) setScrapeNote(note);
            }}
          >
            Refetch
          </button>
        </div>

        {/* For the games no provider has ever heard of — a hack, an obscure
            dump, somebody's homebrew — fetching will never produce a cover, so
            the only way to have one is to supply it. A chosen picture also
            outranks anything fetched later, which is why "Fetch everything
            again" cannot quietly undo this. */}
        <div className="detail-actions">
          <button
            className="btn"
            disabled={busy}
            onClick={async () => {
              setArtNote(null);
              setCommandError(null);
              try {
                const path = await api.chooseCustomCover(game.id);
                if (path) {
                  setArtNote("Cover set from your own image.");
                  onArtworkChanged();
                }
              } catch (e) {
                setCommandError(errorMessage(e));
              }
            }}
          >
            {game.coverCustom ? "Replace image" : "Choose an image"}
          </button>

          {game.coverCustom && (
            <button
              className="btn"
              disabled={busy}
              onClick={async () => {
                setArtNote(null);
                setCommandError(null);
                try {
                  await api.clearCustomCover(game.id);
                  setArtNote("Back to fetched artwork. Refetch to look again.");
                  onArtworkChanged();
                } catch (e) {
                  setCommandError(errorMessage(e));
                }
              }}
            >
              Use fetched artwork
            </button>
          )}
        </div>

        {artNote && <div className="notice">{artNote}</div>}

        {/* For a game the providers do not know because it is a change to one
            they do: a hack of Super Mario 64 can look like Super Mario 64 and
            keep its own name. */}
        {borrowList === null ? (
          <div className="detail-actions">
            <button
              className="btn"
              disabled={busy}
              onClick={async () => {
                setCommandError(null);
                try {
                  const all = await api.listGames({ sort: "title" });
                  setBorrowList(all.filter((g) => g.id !== game.id));
                } catch (e) {
                  setCommandError(errorMessage(e));
                }
              }}
            >
              Copy details from another game
            </button>
          </div>
        ) : (
          <div className="detail-borrow">
            <select
              value={borrowFrom}
              onChange={(e) =>
                setBorrowFrom(e.target.value === "" ? "" : Number(e.target.value))
              }
              aria-label="Game to copy details from"
            >
              <option value="">Choose a game…</option>
              {borrowList.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.title}
                  {g.coverPath ? "" : "  (no artwork)"}
                </option>
              ))}
            </select>
            <button
              className="btn primary"
              disabled={busy || borrowFrom === ""}
              onClick={async () => {
                if (borrowFrom === "") return;
                setCommandError(null);
                try {
                  await api.borrowMetadata(borrowFrom, game.id);
                  const source = borrowList.find((g) => g.id === borrowFrom);
                  setArtNote(
                    `Details and artwork copied from ${source?.title ?? "that game"}. The name is still yours.`,
                  );
                  setBorrowList(null);
                  setBorrowFrom("");
                  onArtworkChanged();
                } catch (e) {
                  setCommandError(errorMessage(e));
                }
              }}
            >
              Copy
            </button>
            <button
              className="btn"
              onClick={() => {
                setBorrowList(null);
                setBorrowFrom("");
              }}
            >
              Cancel
            </button>
          </div>
        )}

        {scrapeNote && <div className="notice">{scrapeNote}</div>}
        {commandError && <div className="error-banner">{commandError}</div>}

        <div className="meta-grid">
          {rows
            .filter(([, value]) => value)
            .map(([key, value]) => (
              <Fragment key={key}>
                <div className="meta-key">{key}</div>
                <div className="meta-val">{value}</div>
              </Fragment>
            ))}
        </div>

        {game.description && (
          <>
            <div className="section-title">About</div>
            <div className="description">{game.description}</div>
          </>
        )}

        <div className="section-title">System</div>
        <div className="row">
          <SystemIcon platform={game.platform} size={20} className="system-mark" />
          <select value={game.platform} onChange={(e) => onSetPlatform(e.target.value)}>
            {!allPlatforms.some((p) => p.slug === game.platform) && (
              <option value={game.platform}>{game.platform}</option>
            )}
            {allPlatforms.map((p) => (
              <option key={p.slug} value={p.slug}>
                {p.name}
              </option>
            ))}
          </select>
        </div>
        <div className="hint" style={{ marginTop: 6 }}>
          Changing the system re-queues this game for metadata and picks a
          different core.
        </div>

        {discs.length > 0 && (
          <>
            <div className="section-title">Discs</div>
            <div className="hint">
              {discs.length} discs, played as one game. The emulator swaps
              between them itself when the game asks, so there is nothing to
              load by hand.
            </div>
            {discs.map((d, i) => (
              <div className="path-box" style={{ marginTop: 6 }} key={d.id}>
                {i + 1}. {d.filename}
              </div>
            ))}
          </>
        )}

        <FileSection key={`file-${sectionKey}`} game={game} onChanged={onHackAdded} />

        {command && (
          <>
            <div className="section-title">Launch command</div>
            <div className="path-box">{command}</div>
          </>
        )}

        <EmulatorSection
          key={`emu-${sectionKey}`}
          game={game}
          platforms={allPlatforms}
          onSaved={refreshCommand}
          onError={(message) => {
            setCommandError(message);
            setCommand(null);
          }}
        />
        <HacksSection key={`hacks-${sectionKey}`} game={game} onHackAdded={onHackAdded} />
        <CheatsSection key={`cheats-${sectionKey}`} game={game} />
        <SavesSection key={`saves-${sectionKey}`} game={game} />

        <div className="section-title">Manage</div>
        <div className="row">
          <button className="btn small" onClick={() => void api.revealGame(game.id)}>
            Show in folder
          </button>
          <button className="btn small danger" onClick={onRemove}>
            Remove from library
          </button>
        </div>
        <div className="hint" style={{ marginTop: 6 }}>
          Removing only forgets the entry here. The file on disk is untouched.
        </div>
      </div>
    </aside>
  );
}
