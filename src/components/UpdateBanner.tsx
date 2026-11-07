/**
 * The notice that a newer Arcadex exists.
 *
 * Deliberately a corner card rather than a modal: an update is news, not an
 * errand, and it should never stand between someone and the game they opened
 * the app to play. It can be dismissed, and stays dismissed for the session.
 */

import { useState } from "react";

import {
  formatBytes,
  installUpdate,
  type DownloadState,
  type UpdateInfo,
} from "../update";

interface Props {
  info: UpdateInfo;
  onDismiss: () => void;
}

export default function UpdateBanner({ info, onDismiss }: Props) {
  const [state, setState] = useState<DownloadState>({ phase: "idle" });

  const busy = state.phase === "downloading" || state.phase === "installing";

  const start = async () => {
    setState({ phase: "downloading", received: 0, total: null });
    try {
      await installUpdate(info, setState);
    } catch (e) {
      setState({
        phase: "failed",
        message: e instanceof Error ? e.message : String(e),
      });
    }
  };

  const pct =
    state.phase === "downloading" && state.total
      ? Math.min(100, Math.round((state.received / state.total) * 100))
      : null;

  return (
    <div className="update-card">
      <div className="update-head">
        <span className="update-dot" aria-hidden="true" />
        <strong>Arcadex {info.version} is available</strong>
        {!busy && (
          <button className="update-x" onClick={onDismiss} title="Not now">
            ×
          </button>
        )}
      </div>

      <div className="update-sub">You have {info.currentVersion}</div>

      {info.notes && <div className="update-notes">{info.notes}</div>}

      {state.phase === "failed" && (
        <div className="update-error">{state.message}</div>
      )}

      {busy ? (
        <div className="update-progress">
          <div className="update-bar">
            <div
              className={`update-fill ${pct === null ? "indeterminate" : ""}`}
              style={pct === null ? undefined : { width: `${pct}%` }}
            />
          </div>
          <div className="update-sub">
            {state.phase === "installing"
              ? "Installing — Arcadex will restart"
              : state.total
                ? `${formatBytes(state.received)} of ${formatBytes(state.total)}`
                : `${formatBytes(state.received)} downloaded`}
          </div>
        </div>
      ) : (
        <div className="update-actions">
          <button className="btn small primary" onClick={start}>
            {state.phase === "failed" ? "Try again" : "Update and restart"}
          </button>
          <button className="btn small" onClick={onDismiss}>
            Not now
          </button>
        </div>
      )}
    </div>
  );
}
