import { useId, useState, type ReactNode } from "react";
import { DAY_NAMES, hhmm, toMinutes } from "./types";

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

/** One setting: a short label, at most one hint line, and the long explanation behind a "?"
 *  that opens and closes with the keyboard (spec 006 FR-025). */
export function SettingRow({
  title,
  hint,
  help,
  children,
}: {
  title: string;
  hint?: string;
  help?: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <div className="setting-wrap">
      <div className="setting">
        <div>
          <div className="setting-title">
            {title}
            {help && (
              <button
                type="button"
                className="help"
                aria-expanded={open}
                aria-controls={id}
                aria-label={`More about ${title}`}
                onClick={() => setOpen(!open)}
              >
                ?
              </button>
            )}
          </div>
          {hint && <div className="note">{hint}</div>}
        </div>
        <div className="setting-control">{children}</div>
      </div>
      {help && open && (
        <div id={id} className="help-text">
          {help}
        </div>
      )}
    </div>
  );
}

/** Seven toggle chips, Monday first. */
export function DaysPicker({ days, onChange }: { days: boolean[]; onChange: (d: boolean[]) => void }) {
  return (
    <div className="days" role="group" aria-label="Days">
      {DAY_NAMES.map((n, i) => (
        <button
          key={n}
          type="button"
          className={`chip ${days[i] ? "on" : ""}`}
          aria-pressed={days[i]}
          onClick={() => onChange(days.map((d, j) => (j === i ? !d : d)))}
        >
          {n}
        </button>
      ))}
    </div>
  );
}

/** A time of day, as minutes since midnight. An empty or partial value is ignored. */
export function TimeField({
  minutes,
  onChange,
  label,
}: {
  minutes: number;
  onChange: (m: number) => void;
  label: string;
}) {
  return (
    <input
      type="time"
      className="btn time"
      aria-label={label}
      value={hhmm(minutes)}
      onChange={(e) => e.target.value && onChange(toMinutes(e.target.value))}
    />
  );
}
