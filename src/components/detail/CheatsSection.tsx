import { useEffect, useState } from "react";

import * as api from "../../api";
import type { Cheat, Game, RetroArchCheats } from "../../types";

/** Cheats from libretro's database, switched on and off one at a time. */
export default function CheatsSection({ game }: { game: Game }) {
  const [cheats, setCheats] = useState<Cheat[]>([]);
  const [cheatBusy, setCheatBusy] = useState(false);
  const [cheatNote, setCheatNote] = useState<string | null>(null);
  const [cheatError, setCheatError] = useState<string | null>(null);
  const [raCheats, setRaCheats] = useState<RetroArchCheats | null>(null);
  const [cheatQuery, setCheatQuery] = useState("");

  useEffect(() => {
    api.listCheats(game.id).then(setCheats).catch(() => setCheats([]));
    api.retroarchCheatStatus().then(setRaCheats).catch(() => setRaCheats(null));
  }, [game.id]);

  // Some games carry several hundred cheats — Super Mario Bros has 711 — so
  // the ones in use are pinned above and the rest are searched, not scrolled.
  const activeCheats = cheats.filter((c) => c.enabled);
  const needle = cheatQuery.trim().toLowerCase();
  const matchingCheats = cheats.filter(
    (c) =>
      !c.enabled &&
      (needle === "" ||
        c.description.toLowerCase().includes(needle) ||
        c.code.toLowerCase().includes(needle)),
  );
  const visibleCheats = matchingCheats.slice(0, 60);

  const searchCheats = async () => {
    setCheatBusy(true);
    setCheatError(null);
    setCheatNote(null);
    try {
      const found = await api.findCheats(game.id);
      setCheats(found);
      if (found.length === 0) {
        setCheatNote("No cheats published for this ROM.");
      }
    } catch (e) {
      setCheatError(api.errorMessage(e));
    } finally {
      setCheatBusy(false);
    }
  };

  const toggleAll = async (enabled: boolean) => {
    setCheatBusy(true);
    setCheatError(null);
    try {
      setCheats(await api.setAllCheats(game.id, enabled));
      setCheatNote(null);
    } catch (e) {
      setCheatError(api.errorMessage(e));
    } finally {
      setCheatBusy(false);
    }
  };

  const toggleCheat = async (cheat: Cheat) => {
    const next = !cheat.enabled;
    setCheats((prev) =>
      prev.map((c) => (c.index === cheat.index ? { ...c, enabled: next } : c)),
    );
    try {
      await api.setCheat(game.id, cheat.index, next);
    } catch (e) {
      setCheatError(api.errorMessage(e));
    }
  };

  const writeCheats = async () => {
    setCheatBusy(true);
    setCheatError(null);
    try {
      const path = await api.saveCheats(game.id);
      setCheatNote(`Saved to ${path}`);
    } catch (e) {
      setCheatError(api.errorMessage(e));
    } finally {
      setCheatBusy(false);
    }
  };

  const row = (cheat: Cheat) => (
    <label className="cheat" key={cheat.index}>
      <input
        type="checkbox"
        checked={cheat.enabled}
        onChange={() => void toggleCheat(cheat)}
      />
      <span>
        <span className="cheat-desc">{cheat.description}</span>
        <span className="cheat-code">{cheat.code}</span>
      </span>
    </label>
  );

  return (
    <>
      <div className="section-title">Cheats</div>

      {cheats.length === 0 ? (
        <>
          <button className="btn small" onClick={searchCheats} disabled={cheatBusy}>
            {cheatBusy ? "Looking…" : "Find cheats"}
          </button>
          <div className="hint" style={{ marginTop: 6 }}>
            Searches libretro's cheat database for Game Genie codes matching
            this ROM. Free, no account needed.
          </div>
        </>
      ) : (
        <>
          <input
            value={cheatQuery}
            placeholder={"Search " + cheats.length + " cheats…"}
            onChange={(e) => setCheatQuery(e.target.value)}
          />

          <div className="cheat-summary">
            <span>
              <strong style={{ color: activeCheats.length ? "var(--accent)" : undefined }}>
                {activeCheats.length}
              </strong>{" "}
              on of {cheats.length}
            </span>
            <span className="spacer" />
            {activeCheats.length > 0 && (
              <button
                className="btn small danger"
                onClick={() => void toggleAll(false)}
                disabled={cheatBusy}
              >
                Turn all off
              </button>
            )}
          </div>

          {activeCheats.length > 0 && (
            <>
              <div className="cheat-group">On</div>
              <div className="cheat-list">{activeCheats.map(row)}</div>
            </>
          )}

          <div className="cheat-group">
            {needle ? "Matches (" + matchingCheats.length + ")" : "Available"}
          </div>

          {matchingCheats.length === 0 ? (
            <div className="hint">
              {needle ? "Nothing matches that search." : "Everything is switched on."}
            </div>
          ) : (
            <>
              <div className="cheat-list">{visibleCheats.map(row)}</div>
              {matchingCheats.length > visibleCheats.length && (
                <div className="hint" style={{ marginTop: 6 }}>
                  Showing {visibleCheats.length} of {matchingCheats.length}.
                  Search to narrow it down.
                </div>
              )}
            </>
          )}

          <div className="row" style={{ marginTop: 12 }}>
            <button className="btn small" onClick={searchCheats} disabled={cheatBusy}>
              Refresh
            </button>
            <button className="btn small" onClick={writeCheats} disabled={cheatBusy}>
              Write now
            </button>
          </div>

          <div className="hint" style={{ marginTop: 6 }}>
            Whatever is switched on here is written to RetroArch when you press
            Play, under every name RetroArch might use for this ROM, so it
            cannot load a stale file instead. "Write now" is only needed if you
            are launching from RetroArch directly rather than from here.
            {raCheats && !raCheats.autoApply && activeCheats.length > 0 && (
              <>
                {" "}
                RetroArch's "Auto-Apply Cheats During Game Load" is currently
                off; playing from here turns it on, since a cheat file it never
                reads would do nothing.
              </>
            )}
          </div>
        </>
      )}

      {cheatNote && (
        <div className="path-box" style={{ marginTop: 8 }}>
          {cheatNote}
        </div>
      )}
      {cheatError && <div className="error-banner">{cheatError}</div>}
    </>
  );
}
