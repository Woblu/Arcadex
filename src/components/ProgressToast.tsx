import type { ScanProgress, ScrapeProgress, UnpackProgress } from "../types";
import { formatBytes } from "../update";

interface Props {
  scan: ScanProgress | null;
  scrape: ScrapeProgress | null;
  nowPlaying: string | null;
  unpacking: UnpackProgress | null;
  onCancelScrape: () => void;
}

export default function ProgressToast({
  scan,
  scrape,
  nowPlaying,
  unpacking,
  onCancelScrape,
}: Props) {
  if (nowPlaying) {
    return (
      <div className="toast">
        <div className="toast-head">
          <strong>Playing</strong>
          <span className="toast-msg">{nowPlaying}</span>
        </div>
        <div className="hint">
          Playtime is recorded when the emulator closes.
        </div>
      </div>
    );
  }

  // Unpacking a disc or a console package before it starts is the slow
  // part of pressing Play, and the only sign anything is happening.
  if (unpacking) {
    const total = unpacking.total ?? 0;
    const done = unpacking.done ?? 0;
    return (
      <div className="toast">
        <div className="toast-head">
          <strong>Unpacking</strong>
          <span className="toast-msg">{unpacking.title}</span>
          {total > 0 && (
            <span className="card-sub">
              {formatBytes(done)} of {formatBytes(total)}
            </span>
          )}
        </div>
        <div className="bar">
          <div className="bar-fill" style={{ width: `${total > 0 ? (done / total) * 100 : 0}%` }} />
        </div>
        <div className="hint" style={{ marginTop: 8 }}>
          Only the first time. The game starts when it is done.
        </div>
      </div>
    );
  }

  const active = scrape ?? scan;
  if (!active) return null;

  const isScrape = active === scrape;
  const pct = active.total > 0 ? (active.current / active.total) * 100 : 0;

  const label = isScrape
    ? scrape!.done
      ? "Metadata complete"
      : "Fetching metadata"
    : scan!.done
      ? "Scan complete"
      : scan!.phase === "discovering"
        ? "Finding ROMs"
        : "Reading ROMs";

  const detail = isScrape
    ? (scrape!.haltedReason ?? scrape!.title)
    : scan!.message;

  // A finished scan explains what it threw out, so a missing game is
  // traceable rather than mysterious.
  const ignoredNote =
    !isScrape && scan!.ignored > 0
      ? `Ignored ${scan!.ignored} non-game file${scan!.ignored === 1 ? "" : "s"}` +
        (scan!.ignoredSummary ? ` — ${scan!.ignoredSummary}` : "")
      : null;

  return (
    <div className="toast">
      <div className="toast-head">
        <strong>{label}</strong>
        <span className="toast-msg">{detail}</span>
        {active.total > 0 && (
          <span className="card-sub">
            {active.current}/{active.total}
          </span>
        )}
        {isScrape && !scrape!.done && (
          <button className="btn small" onClick={onCancelScrape}>
            Stop
          </button>
        )}
      </div>
      <div className="bar">
        <div
          className="bar-fill"
          style={{ width: `${active.done ? 100 : pct}%` }}
        />
      </div>
      {ignoredNote && (
        <div className="hint" style={{ marginTop: 8 }}>
          {ignoredNote}
        </div>
      )}
    </div>
  );
}
