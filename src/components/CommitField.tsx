import { useEffect, useRef, useState, type InputHTMLAttributes } from "react";

type Props = Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> & {
  value: string;
  /** Called once you are done with the field, not on every keystroke. */
  onCommit: (value: string) => void;
};

/**
 * A text field that saves when you leave it, press Enter, or close what it
 * sits in - not once per keystroke.
 *
 * An emulator command saved on every key press was a database write per
 * letter, and in a game's panel a fresh preview of the launch command as
 * well, built from a path that was half typed every time but the last.
 * What you type is kept here until it is finished.
 */
export default function CommitField({ value, onCommit, onBlur, onKeyDown, ...rest }: Props) {
  const [draft, setDraft] = useState(value);
  // Read from the unmount cleanup, which only ever sees the first render.
  const latest = useRef({ draft: value, committed: value, onCommit });
  latest.current.onCommit = onCommit;

  // Follow the stored value when something else changes it, Browse filling
  // in a command, say.
  useEffect(() => {
    setDraft(value);
    latest.current.draft = value;
    latest.current.committed = value;
  }, [value]);

  const commit = () => {
    const l = latest.current;
    if (l.draft !== l.committed) {
      l.committed = l.draft;
      l.onCommit(l.draft);
    }
  };

  // Closing the panel with a change still in the field keeps the change.
  useEffect(() => () => commit(), []);

  return (
    <input
      {...rest}
      value={draft}
      onChange={(e) => {
        setDraft(e.target.value);
        latest.current.draft = e.target.value;
      }}
      onBlur={(e) => {
        commit();
        onBlur?.(e);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        onKeyDown?.(e);
      }}
    />
  );
}
