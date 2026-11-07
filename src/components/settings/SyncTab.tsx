import { useCallback, useEffect, useState } from "react";

import * as api from "../../api";
import type { SyncStatus } from "../../types";
import type { TabProps } from "./tab";

/** "3 minutes ago", for when another machine last synced. */
export function ago(timestamp: number, now = Date.now() / 1000): string {
  const seconds = Math.max(0, Math.round(now - timestamp));
  if (seconds < 90) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 90) return `${minutes} minutes ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 36) return `${hours} hours ago`;
  const days = Math.round(hours / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}

export default function SyncTab({ settings, setValue, setSettings, onError, active }: TabProps) {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(
    () =>
      api
        .syncStatus()
        .then(setStatus)
        .catch(() => setStatus(null)),
    [],
  );

  useEffect(() => {
    if (active) void refresh();
  }, [active, refresh]);

  // The folder is saved as soon as it is chosen, rather than with the Save
  // button, so the first sync can run straight away.
  const chooseFolder = async (folder: string) => {
    onError(null);
    setNote(null);
    try {
      await api.saveSettings({ sync_dir: folder });
      setSettings((prev) => ({ ...prev, sync_dir: folder }));
      await refresh();
      if (folder) await syncNow();
      else setNote("Sync is off. Nothing in the folder was touched.");
    } catch (err) {
      onError(api.errorMessage(err));
    }
  };

  const syncNow = async () => {
    setBusy(true);
    setNote(null);
    try {
      const got = await api.syncNow();
      setNote(
        got.sessions > 0
          ? `Added ${got.sessions} session${got.sessions === 1 ? "" : "s"} ` +
              `(${api.formatPlaytime(got.seconds)}) from your other machines.`
          : "Up to date.",
      );
      await refresh();
    } catch (err) {
      setNote(api.errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const folder = settings.sync_dir ?? "";

  return (
    <>
      <div className="notice">
        Keep saves and play time the same on every machine you play on, using
        a folder you already sync: Syncthing, OneDrive, Dropbox, or a share on
        your NAS. Arcadex has no server and uploads nothing itself; it writes
        into the folder, and whatever syncs the folder carries it across.
        <br />
        <br />
        Before a game starts, newer saves from another machine are brought in,
        and this machine's are backed up first. After it closes, its saves go
        into the folder if they changed, with the last ten versions kept.
      </div>

      <div className="section-title">Sync folder</div>
      <div className="row">
        <span className="hint" style={{ flex: 1, wordBreak: "break-all" }}>
          {folder || "Not syncing."}
        </span>
        <button
          className="btn"
          onClick={async () => {
            const picked = await api.pickFolder();
            if (picked) await chooseFolder(picked);
          }}
        >
          {folder ? "Change…" : "Choose a folder…"}
        </button>
        {folder && (
          <button className="btn small" onClick={() => void chooseFolder("")}>
            Stop syncing
          </button>
        )}
      </div>
      {status?.enabled && !status.reachable && (
        <div className="hint" style={{ marginTop: 6 }}>
          That folder is not there right now. Games still start; they sync
          again once it is back.
        </div>
      )}

      <div className="field" style={{ marginTop: 14 }}>
        <label>This computer's name</label>
        <input
          value={settings.sync_machine_name ?? ""}
          placeholder={status?.name ?? ""}
          onChange={(e) => setValue("sync_machine_name", e.target.value)}
        />
        <div className="hint">What your other machines call this one.</div>
      </div>

      <label className="toggle">
        <input
          type="checkbox"
          checked={(settings.sync_states ?? "1") !== "0"}
          onChange={(e) => setValue("sync_states", e.target.checked ? "1" : "0")}
        />
        <span>
          <strong>Save states too</strong>
          <span className="hint" style={{ marginTop: 2 }}>
            States are larger than saves, and only load in the emulator that
            made them, so keep the same RetroArch core on every machine.
          </span>
        </span>
      </label>

      {folder && (
        <>
          <div className="section-title">Machines</div>
          {status && status.others.length > 0 ? (
            <ul className="plain-list">
              {status.others.map((m) => (
                <li key={m.name + m.lastSeen}>
                  <strong>{m.name}</strong>{" "}
                  <span className="hint">synced {ago(m.lastSeen)}</span>
                </li>
              ))}
            </ul>
          ) : (
            <div className="hint">
              No other machine has synced into this folder yet. Choose the
              same folder in Arcadex on another machine.
            </div>
          )}
          <div className="row" style={{ marginTop: 10 }}>
            <button className="btn primary" disabled={busy} onClick={() => void syncNow()}>
              {busy ? "Syncing…" : "Sync play time now"}
            </button>
          </div>
        </>
      )}

      {note && (
        <div className="hint" style={{ marginTop: 8 }}>
          {note}
        </div>
      )}

      <div className="hint" style={{ marginTop: 18 }}>
        Saves sync for games that run in RetroArch. Standalone emulators keep
        saves in their own memory cards and folders; to share those, point
        each emulator's save folder inside the sync folder.
      </div>
    </>
  );
}
