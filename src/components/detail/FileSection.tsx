import { useEffect, useState } from "react";

import * as api from "../../api";
import type { DatVerdict, Game } from "../../types";

interface Props {
  game: Game;
  /** The library entry changed on disk or in the database. */
  onChanged: () => void;
}

/** Where the game is, whether it is a good dump, and the archive it is in. */
export default function FileSection({ game, onChanged }: Props) {
  const [verdict, setVerdict] = useState<DatVerdict | null>(null);
  const [checking, setChecking] = useState(false);
  const [checkNote, setCheckNote] = useState<string | null>(null);
  const [confirmUnpack, setConfirmUnpack] = useState(false);
  const [unpacking, setUnpacking] = useState(false);
  const [unpackNote, setUnpackNote] = useState<string | null>(null);

  useEffect(() => {
    api.verifyGame(game.id).then(setVerdict).catch(() => setVerdict(null));
  }, [game.id]);

  // Windows paths use backslashes, so both separators have to split.
  const fileName = game.path.split(/[\\/]/).pop() ?? game.path;
  // Only archives have a second copy to collapse. `.rar` is excluded because
  // Arcadex cannot open one in the first place.
  const isArchived = /\.(zip|7z)$/i.test(game.path);
  // A console game kept as a folder is listed under the folder's name and
  // launched from the file inside it, so the two names differ. One zipped up
  // whole carries the serial it was named from. Either way there is no one
  // file to checksum.
  const isFolderGame = fileName !== game.filename || (isArchived && game.serial !== null);

  const runChecksum = async () => {
    setChecking(true);
    setCheckNote(null);
    try {
      const next = await api.checksumGame(game.id);
      setVerdict(next);
      setCheckNote(
        next === "noDats"
          ? "Checksummed. Refetch to look it up by checksum, or import a DAT to verify it."
          : "Checksummed. Refetch to look it up by checksum.",
      );
      onChanged();
    } catch (e) {
      setCheckNote(api.errorMessage(e));
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <div className="section-title">File</div>
      <div className="path-box">{game.path}</div>

      {verdict && typeof verdict === "object" && (
        <div className={`dat-verdict ${verdictClass(verdict)}`}>
          <strong>Verified good dump</strong>
          <div className="hint">
            Matches {verdict.verified.name} in {verdict.verified.datName}.
          </div>
        </div>
      )}
      {verdict === "notInDatabase" && (
        <div className={`dat-verdict ${verdictClass(verdict)}`}>
          <strong>Not in the database</strong>
          <div className="hint">
            No catalogued dump has these checksums. It may be a bad dump, a
            modified ROM, or simply a release the DATs you have loaded do not
            cover.
          </div>
        </div>
      )}

      {!game.crc32 && !isFolderGame && (
        <div className="dat-verdict quiet">
          <strong>Not checksummed</strong>
          <div className="hint">
            Scans skip files over 512 MB so a library of disc images stays quick
            to scan. Checking this one reads it once, which takes a minute or so
            for a full disc. After that it can be verified against your DATs and
            looked up by checksum when you Refetch.
          </div>
          <button
            className="btn small"
            style={{ marginTop: 8 }}
            disabled={checking}
            onClick={() => void runChecksum()}
          >
            {checking ? "Reading the whole file…" : "Checksum this file"}
          </button>
        </div>
      )}
      {checkNote && <div className="hint">{checkNote}</div>}
      {/* Once unpacked, the game is no longer an archive and the box below
          goes, so what happened is said here instead of vanishing with it. */}
      {unpackNote && !isArchived && <div className="notice">{unpackNote}</div>}

      {isArchived && (
        <div className="unpack-box">
          {unpackNote ? (
            <div className="hint">{unpackNote}</div>
          ) : confirmUnpack ? (
            <>
              <div className="hint">
                This deletes <strong>{fileName}</strong> and keeps what is
                inside it, in the same folder. One copy instead of two. It
                cannot be undone, and the archive is only removed once the
                unpacked game is safely in place.
              </div>
              <div className="row" style={{ marginTop: 8 }}>
                <button
                  className="btn small primary"
                  disabled={unpacking}
                  onClick={async () => {
                    setUnpacking(true);
                    try {
                      setUnpackNote(await api.unpackInPlace(game.id));
                      setConfirmUnpack(false);
                      onChanged();
                    } catch (e) {
                      setUnpackNote(api.errorMessage(e));
                    } finally {
                      setUnpacking(false);
                    }
                  }}
                >
                  {unpacking ? "Unpacking…" : "Unpack and delete the archive"}
                </button>
                <button
                  className="btn small"
                  disabled={unpacking}
                  onClick={() => setConfirmUnpack(false)}
                >
                  Cancel
                </button>
              </div>
            </>
          ) : (
            <>
              <div className="hint">
                Stored compressed, so playing it keeps a second, unpacked copy
                as well.
              </div>
              <button
                className="btn small"
                style={{ marginTop: 8 }}
                onClick={() => setConfirmUnpack(true)}
              >
                Unpack and keep only the game
              </button>
            </>
          )}
        </div>
      )}
    </>
  );
}

/** Colour the verdict by how much it should worry you. */
function verdictClass(v: DatVerdict): string {
  if (typeof v === "object") return "good";
  return v === "notInDatabase" ? "warn" : "quiet";
}
