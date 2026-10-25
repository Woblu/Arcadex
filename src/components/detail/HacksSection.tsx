import { useEffect, useState } from "react";

import * as api from "../../api";
import type { Game, HackPreview, PatchEntry } from "../../types";

interface Props {
  game: Game;
  /** A hack joined the library as a game of its own. */
  onHackAdded: () => void;
}

/** The patches that fit this ROM, and applying one to a copy of it. */
export default function HacksSection({ game, onHackAdded }: Props) {
  const [patchPath, setPatchPath] = useState<string | null>(null);
  const [preview, setPreview] = useState<HackPreview | null>(null);
  const [hackTitle, setHackTitle] = useState("");
  const [hackError, setHackError] = useState<string | null>(null);
  const [hackBusy, setHackBusy] = useState(false);
  const [catalogHacks, setCatalogHacks] = useState<PatchEntry[]>([]);

  useEffect(() => {
    api
      .patchesForGame(game.id)
      .then(setCatalogHacks)
      .catch(() => setCatalogHacks([]));
  }, [game.id]);

  const clearHack = () => {
    setPatchPath(null);
    setPreview(null);
    setHackTitle("");
    setHackError(null);
  };

  const choosePatch = async () => {
    setHackError(null);
    try {
      const path = await api.pickFile();
      if (!path) return;
      const info = await api.inspectPatch(game.id, path);
      setPatchPath(path);
      setPreview(info);
      setHackTitle(info.suggestedTitle);
    } catch (e) {
      setHackError(api.errorMessage(e));
    }
  };

  const applyCatalogHack = async (entry: PatchEntry) => {
    setHackBusy(true);
    setHackError(null);
    try {
      await api.applyCatalogPatch(game.id, entry.id, entry.name);
      onHackAdded();
    } catch (e) {
      setHackError(api.errorMessage(e));
    } finally {
      setHackBusy(false);
    }
  };

  const applyPatch = async () => {
    if (!patchPath) return;
    setHackBusy(true);
    setHackError(null);
    try {
      await api.addHack(game.id, patchPath, hackTitle);
      clearHack();
      onHackAdded();
    } catch (e) {
      setHackError(api.errorMessage(e));
    } finally {
      setHackBusy(false);
    }
  };

  if (game.baseGameId) {
    return (
      <>
        <div className="section-title">ROM hack</div>
        <div className="hint">
          Built from another ROM in your library. The original was not modified.
        </div>
        {game.patchPath && (
          <div className="path-box" style={{ marginTop: 6 }}>
            {game.patchPath}
          </div>
        )}
      </>
    );
  }

  return (
    <>
      <div className="section-title">ROM hacks</div>

      {catalogHacks.length > 0 && !preview && (
        <>
          <div className="hint" style={{ marginBottom: 8 }}>
            {catalogHacks.length} patch
            {catalogHacks.length === 1 ? "" : "es"} in your catalog match this
            ROM's checksum exactly.
          </div>
          {catalogHacks.map((entry) => (
            <div className="folder-row" key={entry.id}>
              <div className="folder-path">
                <div style={{ color: "var(--text)" }}>{entry.name}</div>
                <div className="card-sub">
                  {entry.format}
                  {entry.targetHint ? ` · ${entry.targetHint}` : ""}
                </div>
              </div>
              <button
                className="btn small"
                disabled={hackBusy}
                onClick={() => applyCatalogHack(entry)}
              >
                Apply
              </button>
            </div>
          ))}
        </>
      )}

      {!preview && (
        <>
          <button className="btn small" onClick={choosePatch}>
            Add ROM hack…
          </button>
          <div className="hint" style={{ marginTop: 6 }}>
            Pick an IPS, UPS or BPS patch. It is applied to a copy — this ROM
            is never modified — and the result joins your library as its own
            game.
          </div>
        </>
      )}

      {preview && (
        <div
          className="notice"
          style={{
            borderLeftColor: preview.compatible ? "var(--success)" : "var(--danger)",
          }}
        >
          <strong>{preview.format} patch</strong>
          <div style={{ marginTop: 4 }}>{preview.message}</div>

          <div className="field" style={{ margin: "12px 0 8px" }}>
            <label>Name in your library</label>
            <input value={hackTitle} onChange={(e) => setHackTitle(e.target.value)} />
          </div>

          <div className="row">
            <button
              className="btn small primary"
              onClick={applyPatch}
              disabled={hackBusy || !preview.compatible}
            >
              {hackBusy ? "Applying…" : "Apply patch"}
            </button>
            <button className="btn small" onClick={clearHack}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {hackError && <div className="error-banner">{hackError}</div>}
    </>
  );
}
