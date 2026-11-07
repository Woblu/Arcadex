import { useEffect, useState, type ReactNode } from "react";

import * as api from "../api";
import type { Settings } from "../types";
import type { SkinName } from "../skins/shell";
import AppearanceTab from "./settings/AppearanceTab";
import DatsTab from "./settings/DatsTab";
import EmulatorsTab from "./settings/EmulatorsTab";
import FoldersTab from "./settings/FoldersTab";
import HacksTab from "./settings/HacksTab";
import MetadataTab from "./settings/MetadataTab";
import type { TabProps } from "./settings/tab";

export type Tab = "folders" | "emulators" | "metadata" | "hacks" | "dats" | "appearance";

const TABS: Array<{ id: Tab; label: string }> = [
  { id: "folders", label: "ROM folders" },
  { id: "emulators", label: "Emulators" },
  { id: "metadata", label: "Metadata" },
  { id: "hacks", label: "ROM hacks" },
  { id: "dats", label: "Verification" },
  { id: "appearance", label: "Appearance" },
];

interface Props {
  /** The tab to open on; the first-run checklist sends you to the one it means. */
  initialTab?: Tab;
  onClose: () => void;
  /** Applied at once, so the picker is its own preview. */
  onSkinChange: (skin: SkinName) => void;
  /**
   * Scan and fetch metadata, exactly as the button and a dropped folder do.
   * Adding a folder is a request to have its games; making that a second
   * thing to go and press was an oversight, not a decision.
   */
  onScanRequest: () => void;
  /** Go back over every game, including ones that already have artwork. */
  onRefetchAll: () => void;
}

/**
 * The Settings window: the tabs, the settings they share, and Save.
 *
 * Each tab is its own component in `settings/`. A tab is mounted the first
 * time it is opened and kept after that, so a patch import or a bundle
 * download carries on, and is still reporting, after a look at another tab.
 */
export default function SettingsModal({
  initialTab = "folders",
  onClose,
  onSkinChange,
  onScanRequest,
  onRefetchAll,
}: Props) {
  const [tab, setTab] = useState<Tab>(initialTab);
  const [opened, setOpened] = useState<Set<Tab>>(() => new Set<Tab>([initialTab]));
  const [settings, setSettings] = useState<Settings>({});
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .getSettings()
      .then(setSettings)
      .catch((err) => setError(api.errorMessage(err)));
  }, []);

  const open = (next: Tab) => {
    setTab(next);
    setOpened((prev) => (prev.has(next) ? prev : new Set(prev).add(next)));
  };

  const setValue = (key: string, value: string) =>
    setSettings((prev) => ({ ...prev, [key]: value }));

  const persist = async () => {
    try {
      await api.saveSettings(settings);
      setMessage("Saved");
      window.setTimeout(() => setMessage(null), 2000);
    } catch (err) {
      setError(api.errorMessage(err));
    }
  };

  const shared = (id: Tab): TabProps => ({
    settings,
    setValue,
    setSettings,
    onError: setError,
    onMessage: setMessage,
    active: tab === id,
  });

  const panel = (id: Tab, content: ReactNode) =>
    opened.has(id) && (
      <div hidden={tab !== id} key={id}>
        {content}
      </div>
    );

  return (
    <div className="overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal">
        <div className="modal-head">Settings</div>

        <div className="tabs">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={`tab ${tab === t.id ? "active" : ""}`}
              onClick={() => open(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>

        <div className="modal-body">
          {error && <div className="error-banner">{error}</div>}

          {panel("folders", <FoldersTab {...shared("folders")} onScanRequest={onScanRequest} />)}
          {panel("emulators", <EmulatorsTab {...shared("emulators")} />)}
          {panel(
            "metadata",
            <MetadataTab {...shared("metadata")} onClose={onClose} onRefetchAll={onRefetchAll} />,
          )}
          {panel("hacks", <HacksTab {...shared("hacks")} />)}
          {panel("dats", <DatsTab {...shared("dats")} />)}
          {panel("appearance", <AppearanceTab {...shared("appearance")} onSkinChange={onSkinChange} />)}
        </div>

        <div className="modal-foot">
          {message && (
            <span className="hint" style={{ marginRight: "auto" }}>
              {message}
            </span>
          )}
          <button className="btn" onClick={onClose}>
            Close
          </button>
          <button className="btn primary" onClick={persist}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
