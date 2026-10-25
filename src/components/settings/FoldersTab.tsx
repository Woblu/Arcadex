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
          onClick={async () => {
            const removed = await api.cleanMissing();
            onMessage(`Removed ${removed} missing entries`);
            await reload();
          }}
        >
          Clean missing entries
        </button>
      </div>
    </>
  );
}
