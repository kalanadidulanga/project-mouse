import { useEffect, useState, type ReactNode } from "react";

/** An on/off switch with the keyboard behaviour of a real one. */
export function Switch({ on, onChange, label }: { on: boolean; onChange: (next: boolean) => void; label: string }) {
  return (
    <div
      className={`switch ${on ? "on" : ""}`}
      role="switch"
      aria-checked={on}
      aria-label={label}
      tabIndex={0}
      onClick={() => onChange(!on)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onChange(!on);
        }
      }}
    >
      <span className="track">
        <span className="thumb" />
      </span>
      <span>{on ? "On" : "Off"}</span>
    </div>
  );
}

/** A number box that commits on blur or Enter. Anything that is not a number puts back the value
 *  in effect, so an empty box never reaches Rust as `null`, and the value is held to 0..max so it
 *  cannot overflow the Rust type. Rust applies the real range (Review Focus 2). */
export function NumberField({
  value,
  max,
  onCommit,
  label,
}: {
  value: number;
  max: number;
  onCommit: (n: number) => void;
  label: string;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const n = Math.round(Number(text));
    if (text.trim() === "" || !Number.isFinite(n)) {
      setText(String(value));
      return;
    }
    const clamped = Math.min(Math.max(n, 0), max);
    if (clamped === value) setText(String(value));
    else onCommit(clamped);
  };
  return (
    <input
      className="btn num"
      inputMode="numeric"
      aria-label={label}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

/** One settings line: what it is and what it does on the left, the control on the right. */
export function SettingRow({ title, note, children }: { title: string; note?: string; children: ReactNode }) {
  return (
    <div className="setting">
      <div>
        <div className="setting-title">{title}</div>
        {note && <div className="note">{note}</div>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}
