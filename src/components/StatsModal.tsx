import { useEffect, useState } from "react";

import * as api from "../api";
import { artUrl, formatDate, formatPlaytime } from "../api";
import type { Game, LibraryInsights } from "../types";
import SystemIcon from "./SystemIcon";

interface Props {
  onClose: () => void;
  onPick: (gameId: number) => void;
}

/** A game row that doubles as a way back into it. */
function GameRow({
  game,
  detail,
  onPick,
}: {
  game: Game;
  detail: string;
  onPick: (id: number) => void;
}) {
  const cover = artUrl(game.coverPath);
  return (
    <button className="stat-game" onClick={() => onPick(game.id)}>
      {cover ? (
        <img src={cover} alt="" />
      ) : (
        <span className="stat-game-blank">
          <SystemIcon platform={game.platform} size={18} />
        </span>
      )}
      <span className="stat-game-text">
        <span className="stat-game-title">{game.title}</span>
        <span className="card-sub">{detail}</span>
      </span>
    </button>
  );
}

export default function StatsModal({ onClose, onPick }: Props) {
  const [data, setData] = useState<LibraryInsights | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .libraryInsights()
      .then(setData)
      .catch((e) => setError(api.errorMessage(e)));
  }, []);

  const tiles = data
    ? [
        { label: "Time played", value: formatPlaytime(data.totalPlaySeconds) },
        { label: "Sessions", value: data.sessionCount.toLocaleString() },
        {
          label: "Games played",
          value: `${data.gamesPlayed} of ${data.totalGames}`,
        },
        { label: "Longest session", value: formatPlaytime(data.longestSession) },
      ]
    : [];

  return (
    <div
      className="overlay"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        className="modal"
        style={{ maxWidth: 620 }}
        role="dialog"
        aria-modal="true"
        aria-labelledby="stats-title"
      >
        <div className="modal-head">
          <span id="stats-title">Your library</span>
          <span className="spacer" />
          <button className="btn small ghost" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>

        <div className="modal-body">
          {error && <div className="error-banner">{error}</div>}

          {!data ? (
            // An error is said above; "Loading" under it forever was not true.
            !error && <div className="hint">Loading…</div>
          ) : data.totalPlaySeconds === 0 ? (
            <div className="hint">
              Nothing played yet. Launch something and this fills in — playtime
              is recorded when the emulator closes.
            </div>
          ) : (
            <>
              <div className="stat-tiles">
                {tiles.map((t) => (
                  <div className="stat-tile" key={t.label}>
                    <div className="stat-value">{t.value}</div>
                    <div className="stat-label">{t.label}</div>
                  </div>
                ))}
              </div>

              {data.recent.length > 0 && (
                <>
                  <div className="section-title">Jump back in</div>
                  <div className="stat-games">
                    {data.recent.map((g) => (
                      <GameRow
                        key={g.id}
                        game={g}
                        detail={
                          g.lastPlayed
                            ? `Last played ${formatDate(g.lastPlayed)}`
                            : "Not played"
                        }
                        onPick={onPick}
                      />
                    ))}
                  </div>
                </>
              )}

              {data.mostPlayed.length > 0 && (
                <>
                  <div className="section-title">Most played</div>
                  <div className="stat-games">
                    {data.mostPlayed.map((g) => (
                      <GameRow
                        key={g.id}
                        game={g}
                        detail={`${formatPlaytime(g.playSeconds)} over ${g.playCount} session${
                          g.playCount === 1 ? "" : "s"
                        }`}
                        onPick={onPick}
                      />
                    ))}
                  </div>
                </>
              )}
            </>
          )}
        </div>

        <div className="modal-foot">
          <button className="btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
