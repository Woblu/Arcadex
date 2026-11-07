import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import * as api from "../../api";
import type { HackBundle, ImportProgress } from "../../types";
import type { TabProps } from "./tab";

export default function HacksTab({ onError }: TabProps) {
  const [catalogSize, setCatalogSize] = useState(0);
  const [importing, setImporting] = useState(false);
  const [importLine, setImportLine] = useState<string | null>(null);
  const [bundles, setBundles] = useState<HackBundle[] | null>(null);
  const [bundlesLoading, setBundlesLoading] = useState(false);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [bundleFilter, setBundleFilter] = useState("");

  useEffect(() => {
    void api.patchCatalogSize().then(setCatalogSize).catch(() => setCatalogSize(0));
  }, []);

  // The importer streams progress while it walks a bundle.
  useEffect(() => {
    const sub = listen<ImportProgress>("import-progress", (e) => {
      const p = e.payload;
      setImportLine(
        p.done
          ? `${p.imported} catalogued, ${p.skipped} skipped`
          : `${p.scanned} scanned — ${p.message}`,
      );
    });
    return () => {
      void sub.then((f) => f());
    };
  }, []);

  const loadBundles = async () => {
    setBundlesLoading(true);
    onError(null);
    try {
      setBundles(await api.listHackBundles());
    } catch (err) {
      onError(api.errorMessage(err));
    } finally {
      setBundlesLoading(false);
    }
  };

  const getBundle = async (bundle: HackBundle) => {
    setDownloading(bundle.name);
    onError(null);
    setImportLine(null);
    try {
      const summary = await api.downloadHackBundle(bundle);
      setImportLine(summary);
      setCatalogSize(await api.patchCatalogSize());
    } catch (err) {
      onError(api.errorMessage(err));
    } finally {
      setDownloading(null);
    }
  };

  const runImport = async (pick: () => Promise<string | null>) => {
    onError(null);
    setImportLine(null);
    try {
      const path = await pick();
      if (!path) return;
      setImporting(true);
      const summary = await api.importPatches(path);
      setImportLine(summary);
      setCatalogSize(await api.patchCatalogSize());
    } catch (err) {
      onError(api.errorMessage(err));
    } finally {
      setImporting(false);
    }
  };

  return (
    <>
      <div className="notice">
        Import ROM hack patches in bulk and Arcadex indexes them by the checksum
        of the ROM each one was built for. A game's detail panel then shows
        exactly which hacks your dump can run — no guessing about revisions or
        regions.
        <br />
        <br />
        Point it at a folder of patches, or straight at a <code>.7z</code>{" "}
        bundle from a community archive. Only the patch files are read out of a
        bundle; everything else inside is skipped rather than unpacked.
      </div>

      <div className="field">
        <label>Catalog</label>
        <div className="row">
          <div style={{ flex: 1 }}>{catalogSize.toLocaleString()} patches indexed</div>
          {catalogSize > 0 && (
            <button
              className="btn small danger"
              disabled={importing}
              onClick={async () => {
                await api.clearPatchCatalog();
                setCatalogSize(await api.patchCatalogSize());
                setImportLine(null);
              }}
            >
              Clear catalog
            </button>
          )}
        </div>
      </div>

      <div className="row">
        <button
          className="btn primary"
          disabled={importing}
          onClick={() => runImport(api.pickFolder)}
        >
          Import folder…
        </button>
        <button className="btn" disabled={importing} onClick={() => runImport(api.pickFile)}>
          Import .7z or patch…
        </button>
      </div>

      {importLine && (
        <div className="hint" style={{ marginTop: 10 }}>
          {importing ? "Importing — " : ""}
          {importLine}
        </div>
      )}

      <div className="hint" style={{ marginTop: 14 }}>
        Patches in a folder are catalogued where they are; nothing is copied.
        Patches pulled out of a bundle are stored in the app data directory.
        Applying one never modifies your ROM.
      </div>

      <div className="section-title">Download from the Internet Archive</div>

      {bundles === null ? (
        <>
          <button className="btn" onClick={loadBundles} disabled={bundlesLoading}>
            {bundlesLoading ? "Loading…" : "Browse patch bundles"}
          </button>
          <div className="hint" style={{ marginTop: 6 }}>
            The ROM Hack Patch Archive, the corpus ROMhacking.net released when
            it closed. Bundles are grouped by system and by game, and run to
            roughly a gigabyte each — pick the one that matches your library
            rather than taking the lot.
          </div>
        </>
      ) : (
        <>
          <input
            value={bundleFilter}
            placeholder="Filter bundles… e.g. SNES, Pokemon, Fire Emblem"
            onChange={(e) => setBundleFilter(e.target.value)}
            style={{ marginBottom: 10 }}
          />
          <div className="bundle-list">
            {bundles
              .filter((b) =>
                b.name.toLowerCase().includes(bundleFilter.trim().toLowerCase()),
              )
              .map((b) => (
                <div className="folder-row" key={b.name}>
                  <div className="folder-path">
                    <div style={{ color: "var(--text)" }}>{b.name.replace(/\.7z$/i, "")}</div>
                    <div className="card-sub">{(b.size / 1e9).toFixed(2)} GB</div>
                  </div>
                  <button
                    className="btn small"
                    disabled={downloading !== null}
                    onClick={() => void getBundle(b)}
                  >
                    {downloading === b.name ? "Downloading…" : "Download"}
                  </button>
                </div>
              ))}
          </div>
          <div className="hint" style={{ marginTop: 8 }}>
            Only the patch files are kept — the rest of each archive is skipped
            rather than unpacked.
          </div>
        </>
      )}
    </>
  );
}
