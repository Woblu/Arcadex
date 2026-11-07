import { useEffect, useState } from "react";

import * as api from "../../api";
import type { PadBindings, PadLayout } from "../../gamepad";
import { SKINS, isSkin, DEFAULT_SKIN, type SkinName } from "../../skins/shell";
import type { Settings } from "../../types";
import {
  checkForUpdate,
  currentVersion,
  formatBytes,
  installUpdate,
  type DownloadState,
  type UpdateInfo,
} from "../../update";
import ControllerSetup from "../ControllerSetup";
import SkinPreview from "./SkinPreview";
import type { TabProps } from "./tab";

interface Props extends TabProps {
  /** Applied at once, so the picker is its own preview. */
  onSkinChange: (skin: SkinName) => void;
}

export default function AppearanceTab({ settings, setSettings, onError, onSkinChange }: Props) {
  const [version, setVersion] = useState("");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateNote, setUpdateNote] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [download, setDownload] = useState<DownloadState>({ phase: "idle" });

  useEffect(() => {
    void currentVersion().then(setVersion);
  }, []);

  /** Choices made here take effect at once, so they are saved at once. */
  const saveNow = (next: Settings) => {
    setSettings(next);
    void api.saveSettings(next).catch((err) => onError(api.errorMessage(err)));
  };

  const runUpdateCheck = async () => {
    setChecking(true);
    setUpdateNote(null);
    setUpdate(null);
    try {
      const found = await checkForUpdate(true);
      if (found) setUpdate(found);
      else setUpdateNote("Arcadex is up to date.");
    } catch (err) {
      // A button press deserves the real reason — no endpoint yet, no
      // network, a release without a manifest.
      setUpdateNote(`Could not check: ${api.errorMessage(err)}`);
    } finally {
      setChecking(false);
    }
  };

  const runUpdateInstall = async () => {
    if (!update) return;
    setDownload({ phase: "downloading", received: 0, total: null });
    try {
      await installUpdate(update, setDownload);
    } catch (err) {
      setDownload({ phase: "failed", message: api.errorMessage(err) });
    }
  };

  const current = isSkin(settings.ui_skin) ? settings.ui_skin : DEFAULT_SKIN;

  return (
    <>
      <div className="notice">
        The same library, laid out for wherever you are using it. Every layout
        shows the same games and shares the same settings — only the shape
        changes.
      </div>

      <div className="skin-picker">
        {SKINS.map((option) => (
          <button
            key={option.value}
            className={`skin-option ${current === option.value ? "on" : ""}`}
            onClick={() => {
              onSkinChange(option.value);
              saveNow({ ...settings, ui_skin: option.value });
            }}
          >
            <SkinPreview skin={option.value} />
            <div className="skin-name">{option.label}</div>
            <div className="skin-blurb">{option.blurb}</div>
          </button>
        ))}
      </div>

      <div className="section-title">About</div>
      <div className="about-row">
        <div>
          <div className="about-version">Arcadex {version || "…"}</div>
          <div className="hint">
            Updates are downloaded from the project&apos;s releases and checked
            against a signature before anything is installed.
          </div>
        </div>
        <button
          className="btn small"
          onClick={() => void runUpdateCheck()}
          disabled={checking || download.phase !== "idle"}
        >
          {checking ? "Checking…" : "Check for updates"}
        </button>
      </div>

      {updateNote && <div className="hint">{updateNote}</div>}

      {update && (
        <div className="notice" style={{ marginTop: 10 }}>
          <strong>Arcadex {update.version} is available.</strong>
          {update.notes && (
            <div style={{ marginTop: 6, whiteSpace: "pre-wrap" }}>{update.notes}</div>
          )}

          {download.phase === "failed" && (
            <div className="error-banner" style={{ marginTop: 8 }}>
              {download.message}
            </div>
          )}

          {download.phase === "downloading" || download.phase === "installing" ? (
            <div className="hint" style={{ marginTop: 8 }}>
              {download.phase === "installing"
                ? "Installing — Arcadex will restart."
                : download.total
                  ? `Downloading ${formatBytes(download.received)} of ${formatBytes(download.total)}…`
                  : `Downloading ${formatBytes(download.received)}…`}
            </div>
          ) : (
            <button
              className="btn small primary"
              style={{ marginTop: 10 }}
              onClick={() => void runUpdateInstall()}
            >
              Update and restart
            </button>
          )}
        </div>
      )}

      <div className="section-title">Controller</div>
      <div className="hint">
        A pad is picked up on its own, with no pairing step. The D-pad or left
        stick moves between whatever is on screen, including this window, and
        the focus ring thickens while a controller is connected so it can be
        seen from a sofa.
      </div>

      <ControllerSetup
        layout={
          (["auto", "standard", "nintendo", "custom"].includes(settings.pad_layout ?? "")
            ? settings.pad_layout
            : "auto") as PadLayout
        }
        custom={parseBindings(settings.pad_bindings)}
        onChange={(layout, custom) =>
          saveNow({
            ...settings,
            pad_layout: layout,
            pad_bindings: custom ? JSON.stringify(custom) : "",
          })
        }
      />

      <div className="pad-legend">
        <span>
          <b>A</b> Play, or press what is focused
        </span>
        <span>
          <b>B</b> Back / close
        </span>
        <span>
          <b>X</b> Open a game&apos;s panel
        </span>
        <span>
          <b>Y</b> Favourite
        </span>
        <span>
          <b>LB</b> / <b>RB</b> Previous / next system
        </span>
        <span>
          <b>Start</b> Play the selected game
        </span>
      </div>
    </>
  );
}

/** Stored as JSON in the settings table; a corrupt value is simply ignored. */
export function parseBindings(raw: string | undefined): PadBindings | null {
  if (!raw) return null;
  try {
    const v = JSON.parse(raw);
    return typeof v?.confirm === "number" ? (v as PadBindings) : null;
  } catch {
    return null;
  }
}
