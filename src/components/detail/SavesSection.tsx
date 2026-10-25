import { useEffect, useState } from "react";

import * as api from "../../api";
import { artUrl, formatDate, formatSize } from "../../api";
import type { Game, SaveEntry } from "../../types";

/** Battery saves and save states, with RetroArch's thumbnails. */
export default function SavesSection({ game }: { game: Game }) {
  const [saves, setSaves] = useState<SaveEntry[]>([]);
  const [saveBusy, setSaveBusy] = useState(false);
  const [saveNote, setSaveNote] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  const refreshSaves = () =>
    api.listSaves(game.id).then(setSaves).catch(() => setSaves([]));

  useEffect(() => {
    void refreshSaves();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [game.id]);

  const backUpSaves = async () => {
    setSaveBusy(true);
    setSaveError(null);
    try {
      setSaveNote(await api.backUpSaves(game.id));
    } catch (e) {
      setSaveError(api.errorMessage(e));
    } finally {
      setSaveBusy(false);
    }
  };

  const removeState = async (entry: SaveEntry) => {
    setSaveBusy(true);
    setSaveError(null);
    try {
      await api.deleteSaveState(entry.path);
      setSaveNote(`Deleted ${entry.name}`);
      await refreshSaves();
    } catch (e) {
      setSaveError(api.errorMessage(e));
    } finally {
      setSaveBusy(false);
    }
  };

  const count = (kind: "save" | "state") => saves.filter((s) => s.kind === kind).length;

  return (
    <>
      <div className="section-title">Saves</div>

      {saves.length === 0 ? (
        <div className="hint">
          No saves or save states found yet. They appear here once the game has
          written one.
        </div>
      ) : (
        <>
          <div className="cheat-summary">
            <span>
              {count("save")} save{count("save") === 1 ? "" : "s"}
              {", "}
              {count("state")} state{count("state") === 1 ? "" : "s"}
            </span>
            <span className="spacer" />
            <button className="btn small" onClick={backUpSaves} disabled={saveBusy}>
              Back up all
            </button>
          </div>

          <div className="save-list">
            {saves.map((entry) => (
              <div className="save-row" key={entry.path}>
                {entry.screenshot ? (
                  <img className="save-shot" src={artUrl(entry.screenshot) ?? undefined} alt="" />
                ) : (
                  <span className={"save-kind " + (entry.kind === "save" ? "battery" : "snap")}>
                    {entry.kind === "save" ? "SAV" : "ST"}
                    {entry.slot !== null ? entry.slot : ""}
                  </span>
                )}

                <span className="save-meta">
                  <span className="save-name">{entry.name}</span>
                  <span className="card-sub">
                    {formatSize(entry.size)} · {formatDate(entry.modified)}
                    {entry.kind === "save" ? " · game progress" : " · snapshot"}
                  </span>
                </span>

                {entry.kind === "state" && (
                  <button
                    className="btn small danger"
                    disabled={saveBusy}
                    onClick={() => void removeState(entry)}
                  >
                    Delete
                  </button>
                )}
              </div>
            ))}
          </div>

          <div className="hint" style={{ marginTop: 6 }}>
            Back up copies everything into the app data folder, dated. Only save
            states can be deleted here — battery saves hold your actual
            progress.
          </div>
        </>
      )}

      {saveNote && (
        <div className="path-box" style={{ marginTop: 8 }}>
          {saveNote}
        </div>
      )}
      {saveError && <div className="error-banner">{saveError}</div>}
    </>
  );
}
