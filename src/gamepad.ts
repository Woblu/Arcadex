/**
 * Controller support.
 *
 * Two halves that stay deliberately separate:
 *
 *  - `useGamepad` turns the browser's polling Gamepad API into discrete
 *    events. Pads report *state*, not presses, so every frame is compared
 *    with the last to find the edges, and held directions repeat on a
 *    keyboard-like delay so a menu can be scrolled without machine-gunning.
 *
 *  - `focusInDirection` moves focus by geometry rather than by a hand-written
 *    map of what is next to what. Anything ordinarily interactive joins in, so
 *    a new skin or a new button is navigable the moment it renders and there
 *    is no second structure to keep in sync with the markup.
 */

import { useEffect, useRef, useSyncExternalStore } from "react";

export type NavDirection = "up" | "down" | "left" | "right";

export interface PadInfo {
  id: string;
  /** True when the browser recognised the pad and normalised its layout. */
  standard: boolean;
  buttons: number;
  axes: number;
}

export interface GamepadHandlers {
  onDirection?: (dir: NavDirection) => void;
  /** Activate whatever is focused. */
  onConfirm?: () => void;
  /** Close, or step back. */
  onBack?: () => void;
  onAlt?: () => void;
  onAux?: () => void;
  onStart?: () => void;
  onSelect?: () => void;
  onShoulder?: (side: "left" | "right") => void;
  /** Fires when a pad appears or goes away, so the UI can show button hints. */
  onConnected?: (connected: boolean, info: PadInfo | null) => void;
  /** Every button press by its raw index, for the setup screen. */
  onRawButton?: (index: number) => void;
}

/**
 * Which physical button sits at which index.
 *
 * The Gamepad API only promises these positions when it recognises the pad and
 * reports `mapping: "standard"`. Anything it has not seen before, and a pad
 * released last week is exactly that, arrives in raw HID order where the
 * indices mean whatever the manufacturer decided. Nintendo pads also swap the
 * face buttons relative to Xbox, so even a recognised one wants A and B the
 * other way round.
 *
 * So button positions are configuration, not a constant. `custom` exists
 * because no amount of guessing beats letting someone press the button they
 * mean.
 */
export interface PadBindings {
  confirm: number;
  back: number;
  alt: number;
  aux: number;
}

export type PadLayout = "auto" | "standard" | "nintendo" | "custom";

export const LAYOUTS: Record<"standard" | "nintendo", PadBindings> = {
  // Xbox-style: A is the bottom face button.
  standard: { confirm: 0, back: 1, alt: 2, aux: 3 },
  // Nintendo-style: A on the right, B at the bottom, X and Y swapped too.
  nintendo: { confirm: 1, back: 0, alt: 3, aux: 2 },
};

/** Shoulders, sticks and D-pad, which layouts agree on where they exist. */
const BTN = {
  lb: 4,
  rb: 5,
  select: 8,
  start: 9,
  up: 12,
  down: 13,
  left: 14,
  right: 15,
} as const;

/** Past this a stick counts as pushed. */
const STICK = 0.55;

/** Held-direction repeat, in milliseconds: first wait, then the rate. */
const REPEAT_DELAY = 420;
const REPEAT_RATE = 110;

/** Guess a layout from what the pad calls itself. */
export function detectLayout(info: PadInfo | null): "standard" | "nintendo" {
  if (!info) return "standard";
  const id = info.id.toLowerCase();
  const nintendo =
    id.includes("nintendo") ||
    id.includes("switch") ||
    id.includes("joy-con") ||
    id.includes("joycon") ||
    id.includes("pro controller");
  return nintendo ? "nintendo" : "standard";
}

export function resolveBindings(
  layout: PadLayout,
  custom: PadBindings | null,
  info: PadInfo | null,
): PadBindings {
  if (layout === "custom" && custom) return custom;
  if (layout === "standard" || layout === "nintendo") return LAYOUTS[layout];
  return LAYOUTS[detectLayout(info)];
}

/**
 * On an unrecognised pad the D-pad is usually a hat switch on an axis rather
 * than four buttons. The switch has eight positions, clockwise from up, and
 * the browser spreads them evenly from -1 to 1 - a seventh of the range
 * apart, so up is -1, right is -3/7 and up-left is 1. Resting is reported
 * outside the range, commonly 1.28.
 *
 * This used to read the value as a fraction of a full turn and then test
 * which way it pointed with the comparison the wrong way round, so on these
 * pads every direction came out reversed.
 */
export function hatDirections(value: number): Record<NavDirection, boolean> {
  const none = { up: false, down: false, left: false, right: false };
  if (value < -1.05 || value > 1.05) return none;

  // 0 up, 1 up-right, 2 right, 3 down-right, 4 down, 5 down-left, 6 left,
  // 7 up-left. A diagonal counts as both of its neighbours.
  const k = Math.round(((value + 1) * 7) / 2);
  return {
    up: k === 7 || k <= 1,
    right: k >= 1 && k <= 3,
    down: k >= 3 && k <= 5,
    left: k >= 5 && k <= 7,
  };
}

/** Something that happened on the pad this frame. */
export type PadEvent =
  | { kind: "direction"; dir: NavDirection }
  | { kind: "confirm" | "back" | "alt" | "aux" | "start" | "select" }
  | { kind: "shoulder"; side: "left" | "right" }
  | { kind: "raw"; index: number };

/** The part of a `Gamepad` that is read. */
export interface PadSnapshot {
  buttons: ReadonlyArray<{ pressed: boolean }>;
  axes: ReadonlyArray<number>;
}

/**
 * Turns one frame of pad state after another into presses.
 *
 * Kept apart from the hook, which owns the browser's side of things - the
 * animation frame, focus, pads coming and going - so this half can be fed
 * frames directly and checked.
 */
export class PadReader {
  /**
   * False until a frame has been spent recording what is already held down.
   *
   * Without this, coming back from an emulator while still holding a button
   * reads as a fresh press: the map of held buttons is empty, so the button
   * looks like it went from up to down and the game relaunches the moment
   * you quit it.
   */
  private primed = false;
  /** Last frame's state, keyed by button index. */
  private held = new Map<number, boolean>();
  /** Separate keys for raw reporting, so both can fire in one frame. */
  private heldRaw = new Map<number, boolean>();
  private nextRepeat = new Map<NavDirection, number>();

  /** Forget everything, so the next frame primes afresh. */
  reset() {
    this.primed = false;
    this.held.clear();
    this.heldRaw.clear();
    this.nextRepeat.clear();
  }

  get isPrimed() {
    return this.primed;
  }

  /** The presses in this frame, in the order they should be handled. */
  frame(pad: PadSnapshot, now: number, b: PadBindings, wantRaw = false): PadEvent[] {
    const out: PadEvent[] = [];

    // Spend one frame learning what is already held before anything counts
    // as a press. See `primed`.
    if (!this.primed) {
      this.primed = true;
      for (let i = 0; i < pad.buttons.length; i++) {
        this.held.set(i, pad.buttons[i].pressed);
        this.heldRaw.set(i, pad.buttons[i].pressed);
      }
      return out;
    }

    // Report presses by raw index, so the setup screen can show what the pad
    // actually sends rather than leaving someone to guess.
    if (wantRaw) {
      for (let i = 0; i < pad.buttons.length; i++) {
        const down = pad.buttons[i].pressed;
        if (down && !(this.heldRaw.get(i) ?? false)) out.push({ kind: "raw", index: i });
        this.heldRaw.set(i, down);
      }
    }

    const pressed = (index: number) =>
      index >= 0 && pad.buttons.length > index && pad.buttons[index].pressed;

    const edge = (index: number): boolean => {
      const down = pressed(index);
      const before = this.held.get(index) ?? false;
      this.held.set(index, down);
      return down && !before;
    };

    const direction = (dir: NavDirection, active: boolean) => {
      if (!active) {
        this.nextRepeat.delete(dir);
        return;
      }
      const due = this.nextRepeat.get(dir);
      if (due === undefined) {
        this.nextRepeat.set(dir, now + REPEAT_DELAY);
        out.push({ kind: "direction", dir });
      } else if (now >= due) {
        this.nextRepeat.set(dir, now + REPEAT_RATE);
        out.push({ kind: "direction", dir });
      }
    };

    const x = pad.axes[0] ?? 0;
    const y = pad.axes[1] ?? 0;

    // A recognised pad puts the D-pad on buttons; an unrecognised one
    // usually puts it on a hat axis instead.
    const hasDpadButtons = pad.buttons.length > BTN.right;
    const hat =
      !hasDpadButtons && pad.axes.length > 9
        ? hatDirections(pad.axes[9])
        : { up: false, down: false, left: false, right: false };

    direction("left", pressed(BTN.left) || hat.left || x < -STICK);
    direction("right", pressed(BTN.right) || hat.right || x > STICK);
    direction("up", pressed(BTN.up) || hat.up || y < -STICK);
    direction("down", pressed(BTN.down) || hat.down || y > STICK);

    if (edge(b.confirm)) out.push({ kind: "confirm" });
    if (edge(b.back)) out.push({ kind: "back" });
    if (edge(b.alt)) out.push({ kind: "alt" });
    if (edge(b.aux)) out.push({ kind: "aux" });
    if (edge(BTN.start)) out.push({ kind: "start" });
    if (edge(BTN.select)) out.push({ kind: "select" });
    if (edge(BTN.lb)) out.push({ kind: "shoulder", side: "left" });
    if (edge(BTN.rb)) out.push({ kind: "shoulder", side: "right" });
    return out;
  }
}

/** Hand one event to whichever handler wants it. */
function dispatch(h: GamepadHandlers, e: PadEvent) {
  switch (e.kind) {
    case "direction":
      return h.onDirection?.(e.dir);
    case "confirm":
      return h.onConfirm?.();
    case "back":
      return h.onBack?.();
    case "alt":
      return h.onAlt?.();
    case "aux":
      return h.onAux?.();
    case "start":
      return h.onStart?.();
    case "select":
      return h.onSelect?.();
    case "shoulder":
      return h.onShoulder?.(e.side);
    case "raw":
      return h.onRawButton?.(e.index);
  }
}

export function useGamepad(
  handlers: GamepadHandlers,
  opts: { enabled?: boolean; bindings?: PadBindings } = {},
) {
  // Kept in refs so changing either never restarts the polling loop.
  const ref = useRef(handlers);
  ref.current = handlers;
  const bindingsRef = useRef<PadBindings>(opts.bindings ?? LAYOUTS.standard);
  bindingsRef.current = opts.bindings ?? LAYOUTS.standard;

  const enabled = opts.enabled ?? true;

  useEffect(() => {
    if (!enabled) return;
    if (typeof navigator === "undefined" || !navigator.getGamepads) return;

    let raf = 0;
    let sawPad = false;
    const reader = new PadReader();

    let focused = true;
    const onBlur = () => {
      focused = false;
      reader.reset();
    };
    const onFocus = () => {
      focused = true;
    };
    window.addEventListener("blur", onBlur);
    window.addEventListener("focus", onFocus);

    const poll = () => {
      raf = requestAnimationFrame(poll);

      // While another window has the pad, its buttons are not ours to read.
      // An emulator is a window like any other, and a webview that is merely
      // behind one still gets handed every press: without this, playing a
      // game drives the library underneath it at the same time.
      //
      // Driven by real blur and focus events, and starting from "we have it",
      // so that a webview which reports focus oddly leaves the pad working
      // rather than silently dead.
      if (!focused) {
        if (reader.isPrimed) reader.reset();
        return;
      }

      const pads = navigator.getGamepads?.() ?? [];
      const pad = Array.from(pads).find((p): p is Gamepad => !!p && p.connected);

      if (!pad) {
        if (sawPad) {
          sawPad = false;
          reader.reset();
          ref.current.onConnected?.(false, null);
        }
        return;
      }

      if (!sawPad) {
        sawPad = true;
        ref.current.onConnected?.(true, {
          id: pad.id,
          standard: pad.mapping === "standard",
          buttons: pad.buttons.length,
          axes: pad.axes.length,
        });
      }

      const events = reader.frame(
        pad,
        performance.now(),
        bindingsRef.current,
        !!ref.current.onRawButton,
      );
      for (const e of events) dispatch(ref.current, e);
    };

    raf = requestAnimationFrame(poll);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("focus", onFocus);
    };
  }, [enabled]);
}

/**
 * True while the controller setup is waiting for a button.
 *
 * Those presses are answers, not navigation. Without this the app's own pad
 * handling acted on them too: pressing the button meant for Back closed
 * Settings in the middle of setting it up, and Confirm pressed whatever was
 * focused - on exactly the unrecognised pads the screen exists for.
 */
let capturing = false;
const captureListeners = new Set<() => void>();

export function setPadCapture(on: boolean) {
  if (capturing === on) return;
  capturing = on;
  captureListeners.forEach((listener) => listener());
}

export function usePadCapture(): boolean {
  return useSyncExternalStore(
    (listener) => {
      captureListeners.add(listener);
      return () => captureListeners.delete(listener);
    },
    () => capturing,
  );
}

// --------------------------------------------------------------- focus

/**
 * Everything ordinarily interactive counts, rather than only what has been
 * marked up for the pad. Tagging each control by hand would mean every new
 * button is one forgotten attribute away from being unreachable with a
 * controller and reachable with Tab, a difference nobody would notice until
 * they were holding the pad. `data-nav` is still honoured, for marking
 * something that is not natively focusable.
 */
const FOCUSABLE = [
  "button:not([disabled])",
  "input:not([disabled]):not([type=hidden])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "a[href]",
  "[data-nav]",
].join(",");

/**
 * The elements a controller may land on, narrowed to the topmost overlay when
 * one is open so a dialog cannot be navigated out from underneath.
 */
function candidates(): HTMLElement[] {
  const overlays = document.querySelectorAll<HTMLElement>(".overlay, .detail");
  const scope: ParentNode = overlays.length
    ? overlays[overlays.length - 1]
    : document;

  return Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => {
      if (el.hasAttribute("disabled")) return false;
      if (el.dataset.nav === "off") return false;
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return false;
      // Off-screen inside a scroll container still counts, since moving there
      // scrolls it in. Genuinely hidden does not.
      return el.offsetParent !== null || getComputedStyle(el).position === "fixed";
    },
  );
}

function centre(el: Element) {
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2, r };
}

/** Move focus one step in `dir`, to the nearest thing that way. */
export function focusInDirection(dir: NavDirection): boolean {
  const items = candidates();
  if (items.length === 0) return false;

  const active = document.activeElement as HTMLElement | null;
  if (!active || !items.includes(active)) {
    focusQuietly(items[0]);
    return true;
  }

  const others = items.filter((el) => el !== active);
  const best = nearestInDirection(centre(active), others.map(centre), dir);
  if (best === null) return false;
  focusQuietly(others[best]);
  return true;
}

/**
 * Which of `points` is the next one along from `from` in `dir`, by index, or
 * null when nothing lies that way.
 *
 * Distance along the axis of travel counts once and drift across it three
 * times, so "down" from a grid cell prefers the cell directly below over a
 * nearer one two columns away. That is the behaviour that makes a grid feel
 * like a grid.
 */
export function nearestInDirection(
  from: { x: number; y: number },
  points: ReadonlyArray<{ x: number; y: number }>,
  dir: NavDirection,
): number | null {
  const horizontal = dir === "left" || dir === "right";
  const sign = dir === "right" || dir === "down" ? 1 : -1;

  let best: number | null = null;
  let bestScore = Infinity;

  points.forEach((to, i) => {
    const along = horizontal ? (to.x - from.x) * sign : (to.y - from.y) * sign;
    const across = horizontal ? to.y - from.y : to.x - from.x;

    // Must actually lie in the direction asked for. The tolerance forgives
    // rows whose items are not pixel-aligned.
    if (along < 6) return;

    const score = along + Math.abs(across) * 3;
    if (score < bestScore) {
      bestScore = score;
      best = i;
    }
  });
  return best;
}

/**
 * The nearest ancestor that is genuinely meant to scroll.
 *
 * `overflow: hidden` is excluded deliberately. It is still scrollable through
 * script, so anything that overflows a clipped box can be dragged into view by
 * the browser, taking the whole layout with it - which is a bug every time,
 * never what was wanted.
 */
function scrollParent(el: HTMLElement): HTMLElement | null {
  let node = el.parentElement;
  while (node) {
    const style = getComputedStyle(node);
    if (
      (/(auto|scroll)/.test(style.overflowX) && node.scrollWidth > node.clientWidth) ||
      (/(auto|scroll)/.test(style.overflowY) && node.scrollHeight > node.clientHeight)
    ) {
      return node;
    }
    node = node.parentElement;
  }
  return null;
}

/** Someone who has asked for less motion gets none of this. */
function wantsMotion(): boolean {
  return !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Bring an element into view by scrolling only the box that is supposed to
 * scroll, rather than letting the browser walk up the tree scrolling whatever
 * it finds.
 *
 * It eases rather than jumps. Moving along a row of games is the main thing
 * anyone does in the console skin, and landing each one instantly reads as a
 * flicker rather than as movement: with nothing travelling between positions
 * there is no sense of having gone anywhere.
 *
 * `lead` is how far ahead of the edge to start sliding, as a share of the
 * element's own size. Waiting until something is genuinely off-screen means
 * the row sits still, still, still, then lurches; starting earlier keeps the
 * next game visible before you reach for it.
 */
export function bringIntoView(el: HTMLElement, lead = 0.6) {
  const parent = scrollParent(el);
  if (!parent) return;

  const box = parent.getBoundingClientRect();
  const rect = el.getBoundingClientRect();
  const behavior: ScrollBehavior = wantsMotion() ? "smooth" : "auto";

  const marginX = rect.width * lead;
  const marginY = rect.height * lead;

  let left = parent.scrollLeft;
  if (rect.left - marginX < box.left) {
    left += rect.left - marginX - box.left;
  } else if (rect.right + marginX > box.right) {
    left += rect.right + marginX - box.right;
  }

  let top = parent.scrollTop;
  if (rect.top - marginY < box.top) {
    top += rect.top - marginY - box.top;
  } else if (rect.bottom + marginY > box.bottom) {
    top += rect.bottom + marginY - box.bottom;
  }

  if (left === parent.scrollLeft && top === parent.scrollTop) return;
  parent.scrollTo({ left, top, behavior });
}

/** Focus without letting the browser do its own scrolling on the way. */
function focusQuietly(el: HTMLElement) {
  el.focus({ preventScroll: true });
  bringIntoView(el);
}

/**
 * Activate whatever is focused, as a click would, except a text field where
 * the button should not retype whatever happened to be there.
 */
export function activateFocused(): boolean {
  const active = document.activeElement as HTMLElement | null;
  if (!active) return false;
  if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) {
    return false;
  }
  active.click();
  return true;
}

/** Put focus somewhere sensible when a view first appears. */
export function focusFirst(): void {
  const items = candidates();
  const preferred = items.find((el) => el.dataset.navDefault !== undefined);
  const target = preferred ?? items[0];
  if (target) focusQuietly(target);
}
