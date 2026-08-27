// The rule builder. Every `Condition` variant the Rust evaluator understands is reachable from
// here — the UI is the control surface, not a subset of it (core/rule.rs is the source of truth).
// Conditions on one rule are ANDed; separate rules combine by taking the strongest mode.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Mirrors `core::rule::Condition` (serde externally tagged; unit variants are bare strings).
export type Condition =
  | { ProcessRunning: string[] }
  | { TimeWindow: { days: boolean[]; from: number; to: number } }
  | { ExpiryAt: number }
  | "OnACPower"
  | { BatteryAbove: number }
  | "SessionUnlocked"
  | { NotificationStateIn: string[] }
  | { ForegroundAppIn: string[] }
  | { ForegroundAppNotIn: string[] };

export type Mode = "KeepRunning" | "KeepPresenting";

export type Rule = {
  id: string;
  name: string;
  enabled: boolean;
  conditions: Condition[];
  mode: Mode;
};

export type ProfileView = { id: string; name: string; rules: Rule[] };

const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const NOTIF_STATES = [
  ["Presentation", "presentation mode"],
  ["Game", "a fullscreen game"],
  ["Busy", "a fullscreen app"],
  ["App", "a fullscreen Store app"],
  ["QuietTime", "quiet hours"],
  ["Normal", "normal"],
  ["NotPresent", "locked or screensaving"],
];

const hhmm = (m: number) =>
  `${String(Math.floor(m / 60) % 24).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
const toMinutes = (v: string) => {
  const [h, m] = v.split(":").map(Number);
  return ((h || 0) * 60 + (m || 0)) % 1440;
};
const names = (v: string) =>
  v.split(",").map((s) => s.trim()).filter(Boolean);

export function newId(): string {
  try {
    return crypto.randomUUID();
  } catch {
    return `rule-${Date.now()}`;
  }
}

export function modeWord(m: Mode): string {
  return m === "KeepPresenting" ? "presenting" : "running";
}

/** A condition as a clause of an English sentence. */
export function describe(c: Condition): string {
  if (c === "OnACPower") return "on AC power";
  if (c === "SessionUnlocked") return "the session is unlocked";
  if ("ProcessRunning" in c) return `${c.ProcessRunning.join(" or ")} is running`;
  if ("TimeWindow" in c) {
    const { days, from, to } = c.TimeWindow;
    const on = DAYS.filter((_, i) => days[i]);
    const when =
      on.length === 7 ? "every day" : on.length ? on.join(", ") : "no day";
    return `${when} between ${hhmm(from)} and ${hhmm(to)}`;
  }
  if ("ExpiryAt" in c)
    return `until ${new Date(c.ExpiryAt * 1000).toLocaleString()}`;
  if ("BatteryAbove" in c) return `battery is at or above ${c.BatteryAbove}%`;
  if ("NotificationStateIn" in c) {
    const labels = c.NotificationStateIn.map(
      (s) => NOTIF_STATES.find(([id]) => id === s)?.[1] ?? s,
    );
    return `the screen is in ${labels.join(" or ")}`;
  }
  if ("ForegroundAppIn" in c)
    return `${c.ForegroundAppIn.join(" or ")} is in the foreground`;
  if ("ForegroundAppNotIn" in c)
    return `${c.ForegroundAppNotIn.join(" or ")} is not in the foreground`;
  return "an unrecognised condition";
}

export function sentence(mode: Mode, conditions: Condition[]): string {
  const clauses = conditions.map(describe);
  return clauses.length
    ? `Keep ${modeWord(mode)} while ${clauses.join(" and ")}`
    : `Keep ${modeWord(mode)} always`;
}

type Kind =
  | "process"
  | "schedule"
  | "ac"
  | "battery"
  | "unlocked"
  | "screen"
  | "fg"
  | "notfg";

const KINDS: [Kind, string][] = [
  ["process", "a process is running"],
  ["schedule", "it is inside a time window"],
  ["ac", "the machine is on AC power"],
  ["battery", "battery is above a level"],
  ["unlocked", "the session is unlocked"],
  ["screen", "the screen is in a given state"],
  ["fg", "an app is in the foreground"],
  ["notfg", "an app is NOT in the foreground"],
];

/** One condition, built and handed back. Nesting (Not/AnyOf/AllOf) is deliberately not exposed —
 *  UI-UX §3 rules out a node graph; AND comes from stacking conditions, OR from separate rules. */
function ConditionForm({ onAdd }: { onAdd: (c: Condition) => void }) {
  const [kind, setKind] = useState<Kind>("process");
  const [text, setText] = useState("");
  const [days, setDays] = useState<boolean[]>([true, true, true, true, true, false, false]);
  const [from, setFrom] = useState("09:00");
  const [to, setTo] = useState("18:00");
  const [pct, setPct] = useState(20);
  const [states, setStates] = useState<string[]>(["Presentation"]);

  const build = (): Condition | null => {
    switch (kind) {
      case "process":
        return names(text).length ? { ProcessRunning: names(text) } : null;
      case "schedule":
        return { TimeWindow: { days, from: toMinutes(from), to: toMinutes(to) } };
      case "ac":
        return "OnACPower";
      case "battery":
        return { BatteryAbove: Math.max(0, Math.min(100, pct)) };
      case "unlocked":
        return "SessionUnlocked";
      case "screen":
        return states.length ? { NotificationStateIn: states } : null;
      case "fg":
        return names(text).length ? { ForegroundAppIn: names(text) } : null;
      case "notfg":
        return names(text).length ? { ForegroundAppNotIn: names(text) } : null;
    }
  };

  const add = () => {
    const c = build();
    if (c) {
      onAdd(c);
      setText("");
    }
  };

  return (
    <div className="cond">
      <div className="cond-row">
        <span className="note">while</span>
        <select
          className="btn"
          value={kind}
          onChange={(e) => setKind(e.target.value as Kind)}
        >
          {KINDS.map(([id, label]) => (
            <option key={id} value={id}>
              {label}
            </option>
          ))}
        </select>

        {(kind === "process" || kind === "fg" || kind === "notfg") && (
          <input
            className="btn"
            style={{ minWidth: 200 }}
            placeholder="chrome.exe, msbuild.exe"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
          />
        )}

        {kind === "battery" && (
          <input
            className="btn"
            type="number"
            min={0}
            max={100}
            style={{ width: 80 }}
            value={pct}
            onChange={(e) => setPct(Number(e.target.value))}
          />
        )}

        {kind === "schedule" && (
          <>
            <input
              className="btn"
              type="time"
              value={from}
              onChange={(e) => setFrom(e.target.value)}
            />
            <span className="note">to</span>
            <input
              className="btn"
              type="time"
              value={to}
              onChange={(e) => setTo(e.target.value)}
            />
          </>
        )}

        <button className="btn" onClick={add}>
          + condition
        </button>
      </div>

      {kind === "schedule" && (
        <div className="days">
          {DAYS.map((d, i) => (
            <label key={d}>
              <input
                type="checkbox"
                checked={days[i]}
                onChange={(e) =>
                  setDays(days.map((v, j) => (j === i ? e.target.checked : v)))
                }
              />{" "}
              {d}
            </label>
          ))}
        </div>
      )}

      {kind === "screen" && (
        <div className="days">
          {NOTIF_STATES.map(([id, label]) => (
            <label key={id}>
              <input
                type="checkbox"
                checked={states.includes(id)}
                onChange={(e) =>
                  setStates(
                    e.target.checked
                      ? [...states, id]
                      : states.filter((s) => s !== id),
                  )
                }
              />{" "}
              {label}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}

export default function RulesPage() {
  const [profile, setProfile] = useState<ProfileView | null>(null);
  const [mode, setMode] = useState<Mode>("KeepRunning");
  const [draft, setDraft] = useState<Condition[]>([]);

  const load = useCallback(() => {
    invoke<ProfileView>("get_rules").then(setProfile).catch(() => {});
  }, []);
  useEffect(load, [load]);

  const addRule = () => {
    const rule: Rule = {
      id: newId(),
      name: sentence(mode, draft),
      enabled: false, // disabled by default (UI-UX §3)
      conditions: draft,
      mode,
    };
    invoke("upsert_rule", { rule }).then(() => {
      setDraft([]);
      load();
    });
  };

  // The timer rule is owned by the Status page; it would only be confusing here.
  const rules = (profile?.rules ?? []).filter((r) => r.id !== "timer");

  return (
    <>
      <h1>Rules</h1>
      <p className="note">
        A rule keeps the machine awake while all of its conditions hold. New rules start disabled —
        turn one on when you want it. Two rules that disagree resolve to the stronger mode.
      </p>

      {rules.length ? (
        rules.map((r) => (
          <div className="row" key={r.id}>
            <span className="k" style={{ color: "var(--text)" }}>{r.name}</span>
            <span style={{ display: "flex", gap: 12, alignItems: "center" }}>
              <label style={{ fontSize: 12, color: "var(--text-2)" }}>
                <input
                  type="checkbox"
                  checked={r.enabled}
                  onChange={(e) =>
                    invoke("set_rule_enabled", { id: r.id, enabled: e.target.checked }).then(load)
                  }
                />{" "}
                on
              </label>
              <button className="btn" onClick={() => invoke("delete_rule", { id: r.id }).then(load)}>
                Delete
              </button>
            </span>
          </div>
        ))
      ) : (
        <p className="note">No rules yet.</p>
      )}

      <div className="builder">
        <strong style={{ fontSize: 13 }}>New rule</strong>
        <div className="cond-row" style={{ marginTop: 10 }}>
          <span className="note">Keep</span>
          <select
            className="btn"
            value={mode}
            onChange={(e) => setMode(e.target.value as Mode)}
          >
            <option value="KeepRunning">running (screen may sleep)</option>
            <option value="KeepPresenting">presenting (screen stays on)</option>
          </select>
        </div>

        {draft.map((c, i) => (
          <div className="row" key={i}>
            <span className="k">and {describe(c)}</span>
            <button className="btn" onClick={() => setDraft(draft.filter((_, j) => j !== i))}>
              Remove
            </button>
          </div>
        ))}

        <ConditionForm onAdd={(c) => setDraft([...draft, c])} />

        <div style={{ display: "flex", gap: 10, alignItems: "center", marginTop: 12 }}>
          <button className="btn primary" onClick={addRule}>
            Add rule
          </button>
          <span className="note">{sentence(mode, draft)}</span>
        </div>
      </div>
    </>
  );
}
