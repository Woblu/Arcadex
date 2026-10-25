import { describe, expect, it } from "vitest";

import {
  LAYOUTS,
  PadReader,
  detectLayout,
  hatDirections,
  nearestInDirection,
  resolveBindings,
  type PadEvent,
  type PadSnapshot,
} from "./gamepad";

/** A standard-mapping pad with these buttons held and the sticks at rest. */
function pad(held: number[] = [], axes: number[] = [0, 0, 0, 0]): PadSnapshot {
  return {
    buttons: Array.from({ length: 17 }, (_, i) => ({ pressed: held.includes(i) })),
    axes,
  };
}

const kinds = (events: PadEvent[]) =>
  events.map((e) => (e.kind === "direction" ? `direction:${e.dir}` : e.kind));

describe("PadReader", () => {
  it("counts a press once, however long the button is held", () => {
    const r = new PadReader();
    r.frame(pad(), 0, LAYOUTS.standard);
    expect(kinds(r.frame(pad([0]), 16, LAYOUTS.standard))).toEqual(["confirm"]);
    expect(r.frame(pad([0]), 32, LAYOUTS.standard)).toEqual([]);
    expect(r.frame(pad([0]), 48, LAYOUTS.standard)).toEqual([]);
    r.frame(pad(), 64, LAYOUTS.standard);
    expect(kinds(r.frame(pad([0]), 80, LAYOUTS.standard))).toEqual(["confirm"]);
  });

  // Quitting an emulator with A still down must not launch the game again
  // the moment the library has the pad back.
  it("does not read a button already held when it starts as a press", () => {
    const r = new PadReader();
    expect(r.frame(pad([0]), 0, LAYOUTS.standard)).toEqual([]);
    expect(r.frame(pad([0]), 16, LAYOUTS.standard)).toEqual([]);
    r.frame(pad(), 32, LAYOUTS.standard);
    expect(kinds(r.frame(pad([0]), 48, LAYOUTS.standard))).toEqual(["confirm"]);
  });

  it("starts afresh after a reset, as when the window loses focus", () => {
    const r = new PadReader();
    r.frame(pad(), 0, LAYOUTS.standard);
    r.reset();
    expect(r.isPrimed).toBe(false);
    // Held across the reset: learnt, not pressed.
    expect(r.frame(pad([0]), 16, LAYOUTS.standard)).toEqual([]);
    expect(r.frame(pad([0]), 32, LAYOUTS.standard)).toEqual([]);
  });

  it("repeats a held direction after a pause, then steadily", () => {
    const r = new PadReader();
    r.frame(pad(), 0, LAYOUTS.standard);
    const down = (t: number) => kinds(r.frame(pad([13]), t, LAYOUTS.standard));

    expect(down(100)).toEqual(["direction:down"]);
    // Nothing more until the first repeat is due.
    expect(down(300)).toEqual([]);
    expect(down(519)).toEqual([]);
    expect(down(520)).toEqual(["direction:down"]);
    // Then quicker.
    expect(down(600)).toEqual([]);
    expect(down(630)).toEqual(["direction:down"]);

    // Letting go and pushing again moves at once.
    r.frame(pad(), 640, LAYOUTS.standard);
    expect(down(650)).toEqual(["direction:down"]);
  });

  it("reads the stick past its dead zone only", () => {
    const r = new PadReader();
    r.frame(pad(), 0, LAYOUTS.standard);
    expect(r.frame(pad([], [0.4, 0]), 16, LAYOUTS.standard)).toEqual([]);
    expect(kinds(r.frame(pad([], [0.9, -0.9]), 32, LAYOUTS.standard))).toEqual([
      "direction:right",
      "direction:up",
    ]);
  });

  it("follows the layout for which face button is which", () => {
    const r = new PadReader();
    r.frame(pad(), 0, LAYOUTS.nintendo);
    // On a Nintendo pad, A is on the right: index 1.
    expect(kinds(r.frame(pad([1]), 16, LAYOUTS.nintendo))).toEqual(["confirm"]);
    r.frame(pad(), 32, LAYOUTS.nintendo);
    expect(kinds(r.frame(pad([0]), 48, LAYOUTS.nintendo))).toEqual(["back"]);
  });

  it("reports raw button numbers only to whoever asked for them", () => {
    const r = new PadReader();
    r.frame(pad(), 0, LAYOUTS.standard, true);
    expect(r.frame(pad([7]), 16, LAYOUTS.standard, true)).toEqual([{ kind: "raw", index: 7 }]);

    const quiet = new PadReader();
    quiet.frame(pad(), 0, LAYOUTS.standard);
    expect(quiet.frame(pad([7]), 16, LAYOUTS.standard)).toEqual([]);
  });

  it("reads a D-pad on a hat axis when the pad has no D-pad buttons", () => {
    const r = new PadReader();
    const hatPad = (value: number): PadSnapshot => ({
      buttons: Array.from({ length: 12 }, () => ({ pressed: false })),
      axes: [0, 0, 0, 0, 0, 0, 0, 0, 0, value],
    });
    r.frame(hatPad(1.28), 0, LAYOUTS.standard);
    expect(kinds(r.frame(hatPad(-1), 16, LAYOUTS.standard))).toEqual(["direction:up"]);
  });
});

describe("hatDirections", () => {
  // The values a browser reports: eight positions a seventh of the range
  // apart, clockwise from up.
  const position = (k: number) => -1 + (2 * k) / 7;
  const only = (dirs: string[]) => ({
    up: dirs.includes("up"),
    right: dirs.includes("right"),
    down: dirs.includes("down"),
    left: dirs.includes("left"),
  });

  it("reads each of the eight positions the way the pad is pressed", () => {
    expect(hatDirections(position(0))).toEqual(only(["up"]));
    expect(hatDirections(position(1))).toEqual(only(["up", "right"]));
    expect(hatDirections(position(2))).toEqual(only(["right"]));
    expect(hatDirections(position(3))).toEqual(only(["down", "right"]));
    expect(hatDirections(position(4))).toEqual(only(["down"]));
    expect(hatDirections(position(5))).toEqual(only(["down", "left"]));
    expect(hatDirections(position(6))).toEqual(only(["left"]));
    expect(hatDirections(position(7))).toEqual(only(["up", "left"]));
  });

  it("reads nothing at rest", () => {
    expect(hatDirections(1.2857)).toEqual(only([]));
    expect(hatDirections(3.28)).toEqual(only([]));
  });
});

describe("nearestInDirection", () => {
  // A 3x3 grid of cells 100 apart.
  const grid = [0, 1, 2].flatMap((row) =>
    [0, 1, 2].map((col) => ({ x: col * 100, y: row * 100 })),
  );
  const at = (col: number, row: number) => grid[row * 3 + col];
  const index = (col: number, row: number) => row * 3 + col;

  it("moves to the cell straight below, not a nearer one off to the side", () => {
    expect(nearestInDirection(at(1, 0), grid, "down")).toBe(index(1, 1));
    expect(nearestInDirection(at(0, 1), grid, "right")).toBe(index(1, 1));
    expect(nearestInDirection(at(2, 2), grid, "up")).toBe(index(2, 1));
  });

  it("finds nothing past the edge", () => {
    expect(nearestInDirection(at(0, 0), grid, "left")).toBeNull();
    expect(nearestInDirection(at(0, 0), grid, "up")).toBeNull();
  });

  it("forgives a row that is a few pixels out of line", () => {
    const row = [
      { x: 0, y: 0 },
      { x: 120, y: 3 },
      { x: 240, y: -2 },
    ];
    expect(nearestInDirection(row[0], row, "right")).toBe(1);
    // Something barely to the side is not "right".
    expect(nearestInDirection({ x: 116, y: 50 }, row, "right")).toBe(2);
  });
});

describe("layouts", () => {
  it("guesses a Nintendo layout from the pad's name", () => {
    const info = (id: string) => ({ id, standard: true, buttons: 16, axes: 4 });
    expect(detectLayout(info("Pro Controller (Vendor: 057e)"))).toBe("nintendo");
    expect(detectLayout(info("Xbox Wireless Controller"))).toBe("standard");
    expect(detectLayout(null)).toBe("standard");
  });

  it("prefers a custom layout once one has been set", () => {
    const custom = { confirm: 3, back: 2, alt: 1, aux: 0 };
    expect(resolveBindings("custom", custom, null)).toBe(custom);
    // Custom with nothing recorded falls back to guessing.
    expect(resolveBindings("custom", null, null)).toBe(LAYOUTS.standard);
    expect(resolveBindings("nintendo", custom, null)).toBe(LAYOUTS.nintendo);
  });
});
