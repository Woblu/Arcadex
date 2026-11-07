import { useEffect, useState } from "react";

import * as api from "../../api";
import type { LibraryFolder, PlatformInfo } from "../../types";
import type { TabProps } from "./tab";

interface Props extends TabProps {
  /** Scan and fetch metadata, exactly as the button and a dropped folder do. */
  onScanRequest: () => void;
}

export default function FoldersTab({ onError, onMessage, onScanRequest }: Props) {
  const [folders, setFolders] = useState<LibraryFolder[]>([]);
  const [known, setKnown] = useState<PlatformInfo[]>([]);
  /** What cleaning up would do, shown before it is done. */
  const [missing, setMissing] = useState<{ missing: number; unreachable: number } | null>(null);
  const [checking, setChecking] = useState(false);

  const reload = async () => {
    try {
      const [f, k] = await Promise.all([
        api.listLibraryFolders(),
        api.knownPlatforms(),
      ]);
      setFolders(f);
      setKnown(k);
    } catch (err) {
      onError(api.errorMessage(err));
    }
  };

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const addFolder = async () => {
    try {
      const path = await api.pickFolder();
      if (!path) return;
      await api.addLibraryFolder(path, null);
      await reload();
      // You added a folder because you want what is in it.
      onScanRequest();
    } catch (err) {
      onError(api.errorMessage(err));
    }
  };

  return (
    <>
      <div className="notice">
        Playdex indexes ROMs already on your disk. Point it at the folders you
        keep them in. If a folder holds one system, set it below — that
        resolves formats like <code>.bin</code>, <code>.iso</code> and{" "}
        <code>.zip</code> that several consoles share.
      </div>

      {folders.length === 0 && (
        <div className="hint" style={{ marginBottom: 12 }}>
          No folders added yet.
        </div>
      )}

      {folders.map((f) => (
        <div className="folder-row" key={f.id}>
          <div className="folder-path">{f.path}</div>
          <select
            className="select-inline"
            value={f.platformOverride ?? ""}
            onChange={async (e) => {
              await api.addLibraryFolder(f.path, e.target.value || null);
              await reload();
              // Assigning a system changes what everything beneath it is, so
              // the entries under it are re-detected.
              onScanRequest();
            }}
          >
            <option value="">Detect</option>
            {known.map((p) => (
              <option key={p.slug} value={p.slug}>
                {p.name}
              </option>
            ))}
          </select>
          <button
            className="btn small danger"
            onClick={async () => {
              await api.removeLibraryFolder(f.id);
              await reload();
            }}
          >
            Remove
          </button>
        </div>
      ))}

      <div className="row" style={{ marginTop: 14 }}>
        <button className="btn primary" onClick={addFolder}>
          Add folder
        </button>
        <button
          className="btn"
          disabled={checking}
          onClick={async () => {
            setChecking(true);
            try {
              setMissing(await api.missingReport());
            } catch (err) {
              onError(api.errorMessage(err));
            } finally {
              setChecking(false);
            }
          }}
        >
          {checking ? "Checking…" : "Clean missing entries"}
        </button>
      </div>

      {/* Counted and said first. It used to remove at once, and with a drive
          unplugged that was everything on the drive. Games on a drive or
          share that is not there are now left out entirely. */}
      {missing && (
        <div className="unpack-box" style={{ marginTop: 12 }}>
          <div className="hint">
            {missing.missing === 0
              ? "Nothing is missing."
              : `${missing.missing} entr${missing.missing === 1 ? "y points" : "ies point"} at files that are gone. Remove ${missing.missing === 1 ? "it" : "them"} from the library? The files were already gone; nothing on disk changes.`}
            {missing.unreachable > 0 &&
              ` ${missing.unreachable} more ${missing.unreachable === 1 ? "is" : "are"} on a drive or share that is not connected, and ${missing.unreachable === 1 ? "is" : "are"} kept.`}
          </div>
          <div className="row" style={{ marginTop: 8 }}>
            {missing.missing > 0 && (
              <button
                className="btn small danger"
                onClick={async () => {
                  try {
                    const removed = await api.cleanMissing();
                    onMessage(`Removed ${removed} missing entr${removed === 1 ? "y" : "ies"}`);
                    setMissing(null);
                    await reload();
                  } catch (err) {
                    onError(api.errorMessage(err));
                  }
                }}
              >
                Remove {missing.missing}
              </button>
            )}
            <button className="btn small" onClick={() => setMissing(null)}>
              {missing.missing > 0 ? "Keep them" : "OK"}
            </button>
          </div>
        </div>
      )}
    </>
  );
}
