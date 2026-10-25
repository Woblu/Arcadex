import { useState } from "react";

import * as api from "../../api";
import type { ProviderStatus } from "../../types";
import type { TabProps } from "./tab";

interface Props extends TabProps {
  onClose: () => void;
  /** Go back over every game, including ones that already have artwork. */
  onRefetchAll: () => void;
}

export default function MetadataTab({
  settings,
  setValue,
  onError,
  onClose,
  onRefetchAll,
}: Props) {
  const [providers, setProviders] = useState<ProviderStatus[] | null>(null);
  const [testing, setTesting] = useState(false);

  const runTest = async () => {
    setTesting(true);
    onError(null);
    try {
      // Test what is stored, so save first.
      await api.saveSettings(settings);
      setProviders(await api.testCredentials());
    } catch (err) {
      onError(api.errorMessage(err));
    } finally {
      setTesting(false);
    }
  };

  return (
    <>
      <div className="section-title" style={{ marginTop: 0 }}>
        Artwork
      </div>
      <label className="toggle">
        <input
          type="checkbox"
          checked={(settings.libretro_enabled ?? "1") !== "0"}
          onChange={(e) => setValue("libretro_enabled", e.target.checked ? "1" : "0")}
        />
        <span>
          <strong>libretro thumbnails</strong>
          <span className="hint" style={{ marginTop: 2 }}>
            RetroArch's own artwork server — box art, screenshots and title
            screens. No account, no key, no quota, and it works the moment you
            scan. Matches on your ROM's filename, so No-Intro naming gets the
            best results.
          </span>
        </span>
      </label>

      <div className="notice" style={{ marginTop: 18 }}>
        Everything below is optional and adds <em>descriptions</em>, developer,
        genre and release dates — the things artwork alone cannot give you.
        <br />
        <br />
        Cover art and details come from community metadata databases. Use your
        own accounts — ScreenScraper needs a registered developer key plus your
        personal login, and TheGamesDB issues a free API key. Neither is used to
        download games; they only describe ROMs you already have.
        <br />
        <br />
        Passwords and keys are kept in your system's credential store, not in
        the library.
      </div>

      <div className="section-title">ScreenScraper</div>
      <div className="field">
        <label>Developer ID</label>
        <input
          value={settings.ss_devid ?? ""}
          onChange={(e) => setValue("ss_devid", e.target.value)}
        />
      </div>
      <div className="field">
        <label>Developer password</label>
        <input
          type="password"
          value={settings.ss_devpassword ?? ""}
          onChange={(e) => setValue("ss_devpassword", e.target.value)}
        />
        <div className="hint">
          Requested from screenscraper.fr once you have an account. Without it
          the API only answers a trickle of requests.
        </div>
      </div>
      <div className="field">
        <label>Your username</label>
        <input
          value={settings.ss_user ?? ""}
          onChange={(e) => setValue("ss_user", e.target.value)}
        />
      </div>
      <div className="field">
        <label>Your password</label>
        <input
          type="password"
          value={settings.ss_password ?? ""}
          onChange={(e) => setValue("ss_password", e.target.value)}
        />
        <div className="hint">
          Your personal login raises the per-minute and daily quota.
        </div>
      </div>

      <div className="section-title">TheGamesDB (fallback)</div>
      <div className="field">
        <label>API key</label>
        <input
          type="password"
          value={settings.tgdb_key ?? ""}
          onChange={(e) => setValue("tgdb_key", e.target.value)}
        />
        <div className="hint">
          Used when ScreenScraper has no match or its quota runs out. Matches
          on name rather than hash, so it is less precise.
        </div>
      </div>

      <div className="row" style={{ margin: "6px 0 14px" }}>
        <button className="btn" disabled={testing} onClick={runTest}>
          {testing ? "Checking…" : "Test credentials"}
        </button>
        <span className="hint" style={{ flex: 1 }}>
          Saves, then asks each provider whether your keys work.
        </span>
      </div>

      <div className="row" style={{ margin: "6px 0 14px" }}>
        <button
          className="btn"
          onClick={() => {
            onClose();
            onRefetchAll();
          }}
        >
          Fetch everything again
        </button>
        <span className="hint" style={{ flex: 1 }}>
          Looks up every game in the library, including ones that already have
          artwork, and replaces what it finds. Normally only games with no
          metadata are fetched. On a large library this takes a while and uses
          a good deal of your ScreenScraper allowance — the progress card can
          cancel it.
        </span>
      </div>

      {providers && (
        <div style={{ marginBottom: 16 }}>
          {providers.map((p) => (
            <div
              className="folder-row"
              key={p.provider}
              style={{
                borderLeft: `3px solid ${
                  !p.configured
                    ? "var(--border-strong)"
                    : p.ok
                      ? "var(--success)"
                      : "var(--danger)"
                }`,
              }}
            >
              <div className="folder-path">
                <div style={{ color: "var(--text)" }}>
                  {p.provider}
                  {" — "}
                  {!p.configured ? "not set up" : p.ok ? "working" : "not working"}
                </div>
                <div className="card-sub">{p.message}</div>
                {p.quota && <div className="card-sub">{p.quota}</div>}
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="section-title">Preferences</div>
      <div className="field">
        <label>Preferred region</label>
        <select
          value={settings.preferred_region ?? "us"}
          onChange={(e) => setValue("preferred_region", e.target.value)}
        >
          <option value="us">North America</option>
          <option value="eu">Europe</option>
          <option value="jp">Japan</option>
          <option value="wor">World</option>
        </select>
        <div className="hint">Chooses which region's box art and title are preferred.</div>
      </div>
      <div className="field">
        <label>Preferred language</label>
        <select
          value={settings.preferred_lang ?? "en"}
          onChange={(e) => setValue("preferred_lang", e.target.value)}
        >
          <option value="en">English</option>
          <option value="fr">French</option>
          <option value="de">German</option>
          <option value="es">Spanish</option>
          <option value="pt">Portuguese</option>
        </select>
      </div>
    </>
  );
}
