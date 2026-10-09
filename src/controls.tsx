import { useState, type ReactNode } from "react";

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

/** A number box that commits on blur or Enter. The box shows `value` whenever it is not being
 *  edited, so after a commit it shows what the parent holds: the clamped value Rust applied, or
 *  the old one if the call failed. Anything that is not a number commits nothing, so an empty box
 *  never reaches Rust as `null`, and the value is held to 0..max so it cannot overflow the Rust
 *  type. Rust applies the real range (Review Focus 2). */
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
  const [text, setText] = useState<string | null>(null);
  const commit = () => {
    const t = text;
    setText(null);
    if (t === null) return;
    const n = Math.round(Number(t));
    if (t.trim() === "" || !Number.isFinite(n)) return;
    const clamped = Math.min(Math.max(n, 0), max);
    if (clamped !== value) onCommit(clamped);
  };
  return (
    <input
      className="btn num"
      inputMode="numeric"
      aria-label={label}
      value={text ?? String(value)}
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
