import { useEffect, useState } from "react";

import * as api from "../../api";
import type { DatStatus } from "../../types";
import type { TabProps } from "./tab";

export default function DatsTab({ active }: TabProps) {
  const [dats, setDats] = useState<DatStatus | null>(null);
  const [datNote, setDatNote] = useState<string | null>(null);
  const [datBusy, setDatBusy] = useState(false);

  // Read again each time the tab is shown: a game checked from its panel
  // changes the counts.
  useEffect(() => {
    if (active) void api.datStatus().then(setDats).catch(() => setDats(null));
  }, [active]);

  return (
    <>
      <div className="notice">
        A DAT is a catalogue of known-good dumps: for every release of a
        system, its exact size and checksums. Import one and Arcadex can tell
        you which of your files are correct dumps and which are not, on the
        hashes it already computes for everything.
        <br />
        <br />
        No-Intro covers cartridge systems and Redump covers discs. Neither
        offers an API, so the files come from you: download the DAT for a
        system once and point Arcadex at it.
      </div>

      <div className="row" style={{ marginTop: 12 }}>
        <button
          className="btn primary"
          disabled={datBusy}
          onClick={async () => {
            setDatNote(null);
            const path = await api.pickFile();
            if (!path) return;
            setDatBusy(true);
            try {
              const summary = await api.importDat(path);
              setDatNote(
                `${summary.datName}: ${summary.added} dumps catalogued` +
                  (summary.duplicates > 0 ? `, ${summary.duplicates} already known` : ""),
              );
              setDats(await api.datStatus());
            } catch (err) {
              setDatNote(api.errorMessage(err));
            } finally {
              setDatBusy(false);
            }
          }}
        >
          {datBusy ? "Reading…" : "Import a DAT file"}
        </button>
        {dats && dats.total > 0 && (
          <button
            className="btn small"
            disabled={datBusy}
            onClick={async () => {
              await api.clearDats();
              setDats(await api.datStatus());
              setDatNote("Catalogue emptied.");
            }}
          >
            Clear catalogue
          </button>
        )}
      </div>

      {datNote && (
        <div className="hint" style={{ marginTop: 8 }}>
          {datNote}
        </div>
      )}

      {dats && dats.total > 0 && (
        <>
          <div className="section-title">Loaded</div>
          {dats.loaded.map(([name, n]) => (
            <div className="hint" key={name}>
              {name} — {n.toLocaleString()} dumps
            </div>
          ))}

          <div className="section-title">Your library</div>
          <div className="pad-legend">
            <span>
              <b>{dats.library.verified ?? 0}</b> verified good dumps
            </span>
            <span>
              <b>{dats.library.unknown ?? 0}</b> not in the catalogue
            </span>
            <span>
              <b>{dats.library.notHashed ?? 0}</b> too large to have been hashed
            </span>
          </div>
          <div className="hint" style={{ marginTop: 8 }}>
            "Not in the catalogue" is not necessarily bad. It means no DAT you
            have loaded lists those checksums, which covers bad dumps and
            modified ROMs but also systems you have not imported a DAT for.
            A file too large to have been hashed during a scan can be checked
            from its own panel.
          </div>
        </>
      )}
    </>
  );
}
