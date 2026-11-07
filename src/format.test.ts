import { describe, expect, it } from "vitest";

import { formatPlaytime, formatSize, platformName } from "./api";
import { DEFAULT_SKIN, GAME_MODE_SKIN, isSkin, pickSkin, SKINS } from "./skins/shell";
import { formatBytes } from "./update";
import { ago } from "./components/settings/SyncTab";

describe("formatPlaytime", () => {
  it("reads naturally from seconds to hours", () => {
    expect(formatPlaytime(0)).toBe("Never played");
    expect(formatPlaytime(42)).toBe("42s");
    expect(formatPlaytime(60 * 25)).toBe("25m");
    expect(formatPlaytime(3600 * 3 + 60 * 7 + 30)).toBe("3h 7m");
  });
});

describe("formatSize", () => {
  it("picks the unit and keeps one decimal only where it helps", () => {
    expect(formatSize(0)).toBe("—");
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(40 * 1024 * 1024)).toBe("40 MB");
    // A PlayStation 3 game, measured as its whole folder.
    expect(formatSize(23.4 * 1024 ** 3)).toBe("23 GB");
  });
});

describe("formatBytes", () => {
  it("counts a download", () => {
    expect(formatBytes(900)).toBe("900 B");
    expect(formatBytes(2048)).toBe("2 KB");
    expect(formatBytes(5.25 * 1024 * 1024)).toBe("5.3 MB");
  });
});

describe("platformName", () => {
  it("falls back to the slug for a system it does not know", () => {
    const known = [
      { slug: "ps3", name: "PlayStation 3", extensions: [], cores: [], gameCount: 1 },
    ];
    expect(platformName(known, "ps3")).toBe("PlayStation 3");
    expect(platformName(known, "mystery")).toBe("mystery");
  });
});

describe("skins", () => {
  it("knows every skin it lists, and nothing else", () => {
    for (const skin of SKINS) expect(isSkin(skin.value)).toBe(true);
    expect(isSkin("winamp")).toBe(false);
    expect(isSkin(undefined)).toBe(false);
  });
});

describe("listOf", () => {
  it("names a few systems and counts the rest", async () => {
    const { listOf } = await import("./components/SetupChecklist");
    expect(listOf(["Xbox 360"])).toBe("Xbox 360");
    expect(listOf(["PlayStation 3", "Xbox 360"])).toBe("PlayStation 3 and Xbox 360");
    expect(listOf(["A", "B", "C"])).toBe("A, B and C");
    expect(listOf(["A", "B", "C", "D", "E"])).toBe("A, B and 3 more");
  });
});

describe("ago", () => {
  const now = 1_000_000;
  it("says when another machine last synced, roughly", () => {
    expect(ago(now - 30, now)).toBe("just now");
    expect(ago(now - 60 * 12, now)).toBe("12 minutes ago");
    expect(ago(now - 3600 * 5, now)).toBe("5 hours ago");
    expect(ago(now - 3600 * 30, now)).toBe("30 hours ago");
    expect(ago(now - 86400 * 2, now)).toBe("2 days ago");
  });
  it("does not go negative when clocks disagree", () => {
    expect(ago(now + 500, now)).toBe("just now");
  });
});

describe("pickSkin", () => {
  it("keeps Game Mode's layout apart from the desktop's", () => {
    expect(pickSkin({}, false)).toBe(DEFAULT_SKIN);
    expect(pickSkin({}, true)).toBe(GAME_MODE_SKIN);
    expect(pickSkin({ ui_skin: "wii" }, false)).toBe("wii");
    // A desktop choice does not follow into Game Mode, or the other way.
    expect(pickSkin({ ui_skin: "launchbox" }, true)).toBe(GAME_MODE_SKIN);
    expect(pickSkin({ ui_skin_game_mode: "switch" }, false)).toBe(DEFAULT_SKIN);
    expect(pickSkin({ ui_skin_game_mode: "switch" }, true)).toBe("switch");
    expect(pickSkin({ ui_skin_game_mode: "nonsense" }, true)).toBe(GAME_MODE_SKIN);
  });
});
