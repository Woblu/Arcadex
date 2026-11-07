/**
 * In-app updates.
 *
 * The app checks quietly on launch and then gets out of the way: finding an
 * update is not a reason to interrupt what someone sat down to do. Nothing is
 * downloaded until the notice is accepted, and the notice can be dismissed for
 * the session.
 *
 * Every update is verified against the public key compiled into the app before
 * a byte of it is run, so a tampered or mis-hosted file is refused rather than
 * installed. That check is the plugin's, not ours — this module only decides
 * *when* to ask and what to say.
 */

import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string | null;
  date: string | null;
  /** Held so the download can start without asking the server twice. */
  handle: Update;
}

export type DownloadState =
  | { phase: "idle" }
  | { phase: "downloading"; received: number; total: number | null }
  | { phase: "installing" }
  | { phase: "failed"; message: string };

export async function currentVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "unknown";
  }
}

/**
 * Ask whether there is something newer.
 *
 * Returns `null` both when the app is current and when the check could not be
 * made — being offline, or having no endpoint configured yet, is an ordinary
 * state for a desktop app and not worth an error banner. `manual` flips that:
 * someone who pressed the button deserves to be told why nothing happened.
 */
export async function checkForUpdate(manual = false): Promise<UpdateInfo | null> {
  try {
    const found = await check();
    if (!found) return null;
    return {
      version: found.version,
      currentVersion: found.currentVersion,
      notes: found.body?.trim() || null,
      date: found.date ?? null,
      handle: found,
    };
  } catch (e) {
    if (manual) throw e;
    return null;
  }
}

/**
 * Download the update, then install and restart into it.
 *
 * `onProgress` is fed the running byte count so the UI can show something
 * moving — an installer that silently does nothing for a minute reads as a
 * hang, and this one is tens of megabytes.
 */
/**
 * Set by the app while a game is running or being got ready.
 *
 * Installing restarts Arcadex, and the restart takes with it the wait on the
 * emulator that records the session when it closes - so updating mid-game
 * lost that session's play time. Both places that can install an update come
 * through here.
 */
let gameRunning = false;

export function setGameRunning(running: boolean) {
  gameRunning = running;
}

export async function installUpdate(
  info: UpdateInfo,
  onProgress: (state: DownloadState) => void,
): Promise<void> {
  if (gameRunning) {
    throw new Error(
      "A game is running. Update once it has closed, so its play time is recorded.",
    );
  }
  let received = 0;
  let total: number | null = null;

  await info.handle.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? null;
        onProgress({ phase: "downloading", received: 0, total });
        break;
      case "Progress":
        received += event.data.chunkLength;
        onProgress({ phase: "downloading", received, total });
        break;
      case "Finished":
        onProgress({ phase: "installing" });
        break;
    }
  });

  // On Windows the installer replaces the running executable, so the app has
  // to go away for the new one to take its place.
  await relaunch();
}

export function formatBytes(n: number): string {
  const KB = 1024;
  const MB = KB * 1024;
  const GB = MB * 1024;
  if (n < KB) return `${n} B`;
  if (n < MB) return `${Math.round(n / KB)} KB`;
  if (n < GB) return `${(n / MB).toFixed(1)} MB`;
  return `${(n / GB).toFixed(1)} GB`;
}
