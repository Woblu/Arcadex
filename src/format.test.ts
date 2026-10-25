import { describe, expect, it } from "vitest";

import { formatPlaytime, formatSize, platformName } from "./api";
import { isSkin, SKINS } from "./skins/shell";
import { formatBytes } from "./update";

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
