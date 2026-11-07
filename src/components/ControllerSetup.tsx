/**
 * Controller setup.
 *
 * The Gamepad API normalises button positions only for pads the browser
 * already knows. A pad newer than the browser arrives in raw HID order, where
 * index 2 might be anything, which is how pressing one button ends up opening
 * something unrelated.
 *
 * Rather than shipping a table of every controller ever made, this shows what
 * the pad actually reports and lets you press the four buttons you mean. It is
 * the one part of the app where guessing is clearly worse than asking.
 */

import { useEffect, useRef, useState } from "react";

import {
  detectLayout,
  setPadCapture,
  useGamepad,
  type PadBindings,
  type PadInfo,
  type PadLayout,
} from "../gamepad";

const STEPS: Array<{ key: keyof PadBindings; label: string; hint: string }> = [
  { key: "confirm", label: "Confirm", hint: "Starts a game, presses a button" },
  { key: "back", label: "Back", hint: "Closes a panel" },
  { key: "alt", label: "Options", hint: "Opens a game's panel" },
  { key: "aux", label: "Favourite", hint: "Stars the selected game" },
];

interface Props {
  layout: PadLayout;
  custom: PadBindings | null;
  onChange: (layout: PadLayout, custom: PadBindings | null) => void;
}

export default function ControllerSetup({ layout, custom, onChange }: Props) {
  const [info, setInfo] = useState<PadInfo | null>(null);
  const [lastPressed, setLastPressed] = useState<number | null>(null);
  const [step, setStep] = useState<number | null>(null);
  const draft = useRef<Partial<PadBindings>>({});

  // Clear the "last pressed" flash so it reads as a pulse per press rather
  // than a value that just sits there.
  useEffect(() => {
    if (lastPressed === null) return;
    const id = window.setTimeout(() => setLastPressed(null), 700);
    return () => window.clearTimeout(id);
  }, [lastPressed]);

  // While a button is being asked for, the rest of the app leaves the pad
  // alone.
  useEffect(() => {
    setPadCapture(step !== null);
    return () => setPadCapture(false);
  }, [step]);

  useGamepad({
    onConnected: (connected, padInfo) => setInfo(connected ? padInfo : null),
    onRawButton: (index) => {
      setLastPressed(index);
      if (step === null) return;

      draft.current[STEPS[step].key] = index;
      if (step + 1 < STEPS.length) {
        setStep(step + 1);
      } else {
        onChange("custom", draft.current as PadBindings);
        setStep(null);
      }
    },
  });

  const active = layout === "custom" ? custom : null;
  const effective =
    layout === "custom" && custom
      ? "your own"
      : layout === "auto"
        ? `auto (${detectLayout(info)})`
        : layout;

  return (
    <>
      <div className="pad-status">
        {info ? (
          <>
            <div>
              <strong>{info.id}</strong>
            </div>
            <div className="hint">
              {info.buttons} buttons, {info.axes} axes ·{" "}
              {info.standard ? (
                "recognised layout"
              ) : (
                <span className="pad-warn">
                  layout not recognised, so button positions are guesses until
                  you set them below
                </span>
              )}
            </div>
          </>
        ) : (
          <div className="hint">
            No controller detected. Press a button on it to wake it up.
          </div>
        )}
      </div>

      <div className="field">
        <label>Button layout</label>
        <select
          value={layout}
          onChange={(e) => onChange(e.target.value as PadLayout, custom)}
        >
          <option value="auto">Detect from the controller name</option>
          <option value="standard">Xbox style (A at the bottom)</option>
          <option value="nintendo">Nintendo style (A on the right)</option>
          <option value="custom" disabled={!custom}>
            Buttons you set yourself
          </option>
        </select>
        <div className="hint">Currently using: {effective}</div>
      </div>

      {step === null ? (
        <div className="row" style={{ marginTop: 10 }}>
          <button
            className="btn small primary"
            onClick={() => {
              draft.current = {};
              setStep(0);
            }}
            disabled={!info}
          >
            {custom ? "Set buttons again" : "Set the buttons myself"}
          </button>
          {custom && (
            <button
              className="btn small"
              onClick={() => onChange("auto", null)}
            >
              Forget them
            </button>
          )}
        </div>
      ) : (
        <div className="notice pad-capture">
          <strong>Press the button you use for {STEPS[step].label}</strong>
          <div className="hint" style={{ marginTop: 4 }}>
            {STEPS[step].hint} · {step + 1} of {STEPS.length}
          </div>
          <button
            className="btn small"
            style={{ marginTop: 10 }}
            onClick={() => setStep(null)}
          >
            Cancel
          </button>
        </div>
      )}

      {active && (
        <div className="hint" style={{ marginTop: 8 }}>
          Confirm = button {active.confirm}, Back = {active.back}, Options ={" "}
          {active.alt}, Favourite = {active.aux}
        </div>
      )}

      <div className="pad-tester">
        <span className="hint">Press anything to check it is seen:</span>
        <span className={`pad-blip ${lastPressed !== null ? "lit" : ""}`}>
          {lastPressed !== null ? `button ${lastPressed}` : "waiting"}
        </span>
      </div>
    </>
  );
}
