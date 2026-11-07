import type { SetupStatus } from "../types";
import type { Tab } from "./SettingsModal";

interface Props {
  status: SetupStatus;
  onAddFolder: () => void;
  onOpenSettings: (tab: Tab) => void;
  onDismiss: () => void;
}

/** "PlayStation 3, Xbox 360 and 2 more" */
export function listOf(names: string[]): string {
  if (names.length <= 3) {
    return names.length <= 1 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  }
  return `${names.slice(0, 2).join(", ")} and ${names.length - 2} more`;
}

/**
 * The three things between installing Arcadex and playing something.
 *
 * The first run used to ask for a ROM folder and nothing else. Most people
 * added one, scanned, pressed Play, and were told RetroArch was not set: the
 * second step was there, in Settings, but nothing said so. This says what is
 * done and what is not, by system, and goes straight to where each is fixed.
 * It stays until every system you have can start, or until hidden.
 */
export default function SetupChecklist({ status, onAddFolder, onOpenSettings, onDismiss }: Props) {
  const hasGames = status.folders > 0 || status.games > 0;
  const missing = status.systemsWithoutEmulator;
  const emulatorsReady = hasGames && missing.length === 0;

  const steps = [
    {
      done: hasGames,
      title: "Add your games",
      detail: hasGames
        ? `${status.games} game${status.games === 1 ? "" : "s"} in your library.`
        : "Point Arcadex at the folders you keep ROMs in. It reads them where they are.",
      action: hasGames ? null : { label: "Add a folder", run: onAddFolder },
    },
    {
      done: emulatorsReady,
      title: "Set up emulators",
      detail: !hasGames
        ? "Each system then needs something to run it: RetroArch, or a standalone emulator."
        : missing.length > 0
          ? `${listOf(missing)} can't start yet.`
          : "Every system you have can start.",
      action: emulatorsReady ? null : { label: "Set up emulators", run: () => onOpenSettings("emulators") },
    },
    {
      done: status.descriptions,
      optional: true,
      title: "Descriptions",
      detail: status.descriptions
        ? "Descriptions and release dates are on."
        : "Box art comes free. A ScreenScraper or TheGamesDB account adds descriptions too.",
      action: status.descriptions ? null : { label: "Add an account", run: () => onOpenSettings("metadata") },
    },
  ];

  return (
    <section className="setup-card" aria-label="Getting started">
      <div className="setup-head">
        <strong>Getting started</strong>
        <button className="btn small ghost" onClick={onDismiss}>
          Hide
        </button>
      </div>
      <ol className="setup-steps">
        {steps.map((step) => (
          <li key={step.title} className={step.done ? "done" : ""}>
            <span className="setup-mark" aria-hidden="true">
              {step.done ? "✓" : ""}
            </span>
            <span className="setup-text">
              <span className="setup-title">
                {step.title}
                {"optional" in step && step.optional && !step.done && (
                  <span className="setup-optional">optional</span>
                )}
                <span className="sr-only">{step.done ? " (done)" : " (to do)"}</span>
              </span>
              <span className="hint">{step.detail}</span>
              {step.action && (
                <button className="btn small" onClick={step.action.run}>
                  {step.action.label}
                </button>
              )}
            </span>
          </li>
        ))}
      </ol>
    </section>
  );
}
