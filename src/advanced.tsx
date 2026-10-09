// Advanced (spec 005 FR-012): everything the engine can do beyond Start/Stop, unchanged. Rules
// and the timer hold power on their own conditions, even while Home says Stopped.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import RulesPage, { Mode, Rule, ProfileView, modeWord } from "./rules";

type Diagnostics = {
  effective_mode: string;
  system_sleep_blocked: boolean;
  display_blocked: boolean;
  lock_blocked: boolean;
  reason: string;
  memory_mb: number;
  system_idle_secs: number;
  human_idle_secs: number;
  input_enabled: boolean;
  input_blocked: boolean;
  remote_session: boolean;
};

type AwakeReport = {
  readable: boolean;
  system_held: boolean;
  display_held: boolean;
  away_mode_held: boolean;
  ours: "Off" | "KeepRunning" | "KeepPresenting";
};

type ProfileSummary = { id: string; name: string; active: boolean; rule_count: number };

function fmtIdle(secs: number): string {
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return m < 60 ? `${m}m ${s}s` : `${Math.floor(m / 60)}h ${m % 60}m`;
}

/** "Keep awake for 2 hours": a rule with an `ExpiryAt` condition, so it releases itself.
 *  Deliberately not the manual mode: manual never expires, a rule does. */
const TIMER_ID = "timer";
const DURATIONS: [string, number][] = [
  ["15m", 15],
  ["30m", 30],
  ["1h", 60],
  ["2h", 120],
  ["4h", 240],
];

function Timer({ onChange }: { onChange: () => void }) {
  const [rule, setRule] = useState<Rule | null>(null);
  const [mode, setMode] = useState<Mode>("KeepRunning");
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  const load = useCallback(() => {
    invoke<ProfileView>("get_rules")
      .then((p) => setRule(p.rules.find((r) => r.id === TIMER_ID) ?? null))
      .catch(() => {});
  }, []);
  useEffect(load, [load]);
  useEffect(() => {
    const t = window.setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => window.clearInterval(t);
  }, []);

  const deadline =
    rule?.conditions.reduce<number | null>(
      (acc, c) => (typeof c === "object" && "ExpiryAt" in c ? c.ExpiryAt : acc),
      null,
    ) ?? null;
  const left = deadline === null ? 0 : deadline - now;

  const cancel = useCallback(
    () =>
      invoke("delete_rule", { id: TIMER_ID }).then(() => {
        setRule(null);
        onChange();
      }),
    [onChange],
  );

  // ponytail: the expired rule is only swept while the window is open. It evaluates false either
  // way, so a stale one holds nothing, sweep it in the scheduler tick if that ever stops being true.
  useEffect(() => {
    if (deadline !== null && left <= 0) cancel();
  }, [deadline, left, cancel]);

  const start = (minutes: number) => {
    const at = Math.floor(Date.now() / 1000) + minutes * 60;
    const r: Rule = {
      id: TIMER_ID,
      name: `Keep ${modeWord(mode)} for ${minutes} minutes`,
      enabled: true,
      conditions: [{ ExpiryAt: at }],
      mode,
    };
    invoke("upsert_rule", { rule: r }).then(() => {
      setRule(r);
      onChange();
    });
  };

  if (deadline !== null && left > 0) {
    return (
      <div className="row">
        <span className="k" style={{ color: "var(--text)" }}>
          Keeping {modeWord(rule!.mode)} for another {fmtIdle(left)}
        </span>
        <button className="btn" onClick={cancel}>
          Cancel timer
        </button>
      </div>
    );
  }

  return (
    <div className="cond-row" style={{ marginTop: 16 }}>
      <span className="note">Keep</span>
      <select className="btn" value={mode} onChange={(e) => setMode(e.target.value as Mode)}>
        <option value="KeepRunning">running</option>
        <option value="KeepPresenting">presenting</option>
      </select>
      <span className="note">for</span>
      {DURATIONS.map(([label, mins]) => (
        <button key={label} className="btn" onClick={() => start(mins)}>
          {label}
        </button>
      ))}
    </div>
  );
}

const ELEVATED_CMD = "powercfg /requests";

/** E1. Two things the panel can state without admin: exactly what we hold, and exactly what
 *  Windows will report to an unelevated process. It does not merge them, and it does not claim
 *  the second is complete, see specs/003-settings-ui/research.md R1. */
function WhyAwake() {
  const [r, setR] = useState<AwakeReport | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const read = () => invoke<AwakeReport>("why_awake").then(setR).catch(() => {});
    read();
    const t = window.setInterval(read, 2000);
    return () => window.clearInterval(t);
  }, []);

  const copy = () => {
    navigator.clipboard
      ?.writeText(ELEVATED_CMD)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => {});
  };

  const ourLine =
    r?.ours === "KeepPresenting"
      ? "project-mouse is keeping this machine awake and the display on."
      : r?.ours === "KeepRunning"
        ? "project-mouse is keeping this machine awake."
        : "project-mouse is holding nothing.";

  const held = r
    ? [
        r.system_held && "keep the system awake",
        r.display_held && "keep the display on",
        r.away_mode_held && "hold away mode",
      ].filter(Boolean)
    : [];

  return (
    <>
      <h2>Why is my PC awake?</h2>

      <p className="why-line">{ourLine}</p>

      {r && !r.readable && (
        <p className="why-line">Windows would not tell us what else is holding a power request.</p>
      )}
      {r?.readable && (
        <p className="why-line">
          {held.length
            ? `Windows also reports a request on this machine to ${
                held.length > 1
                  ? `${held.slice(0, -1).join(", ")} and ${held[held.length - 1]}`
                  : held[0]
              }.`
            : "Windows reports no other request it will show us."}
        </p>
      )}

      <p className="note">
        That second line is what Windows will tell a program running without administrator rights.
        It does not name the program, and it does not cover every kind of request, so treat it as
        a hint, not an inventory. To get the full list with names, run this from an elevated
        prompt:
      </p>
      <div className="cmd">
        <code>{ELEVATED_CMD}</code>
        <button className="btn" onClick={copy}>{copied ? "Copied" : "Copy"}</button>
      </div>
    </>
  );
}

/** The profile the engine is holding, and the others it could hold instead. */
function ProfileSwitcher({ onChange }: { onChange: () => void }) {
  const [list, setList] = useState<ProfileSummary[]>([]);
  const load = useCallback(() => {
    invoke<ProfileSummary[]>("list_profiles").then(setList).catch(() => {});
  }, []);
  useEffect(load, [load]);

  const active = list.find((p) => p.active);
  if (!list.length) return null;

  return (
    <div className="row">
      <span className="k">Profile</span>
      <span className="v">
        <select
          className="btn"
          value={active?.id ?? ""}
          onChange={(e) =>
            invoke("set_profile", { id: e.target.value }).then(() => {
              load();
              onChange();
            })
          }
        >
          {list.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name} ({p.rule_count} {p.rule_count === 1 ? "rule" : "rules"})
            </option>
          ))}
        </select>
      </span>
    </div>
  );
}

/** Create and delete profiles. Deleting the last one is refused by the Rust side. */
function ProfileManager({ onChange }: { onChange: () => void }) {
  const [list, setList] = useState<ProfileSummary[]>([]);
  const [name, setName] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(() => {
    invoke<ProfileSummary[]>("list_profiles").then(setList).catch(() => {});
  }, []);
  useEffect(load, [load]);

  const after = () => {
    load();
    onChange();
  };
  const create = () => {
    if (!name.trim()) return;
    invoke("create_profile", { name: name.trim() }).then(() => {
      setName("");
      after();
    });
  };
  const remove = (id: string) => {
    setErr(null);
    invoke("delete_profile", { id })
      .then(after)
      .catch((e) => setErr(String(e)));
  };

  return (
    <>
      {list.map((p) => (
        <div className="row" key={p.id}>
          <span className="k" style={{ color: "var(--text)" }}>
            {p.name}
            {p.active ? " (active)" : ""}
          </span>
          <button className="btn" onClick={() => remove(p.id)}>Delete</button>
        </div>
      ))}
      {err && <p className="note" style={{ color: "var(--error)" }}>{err}</p>}
      <div className="cond-row" style={{ marginTop: 10 }}>
        <input
          className="btn"
          placeholder="new profile name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && create()}
        />
        <button className="btn primary" onClick={create}>Add profile</button>
      </div>
    </>
  );
}

/** What Windows is being asked for right now, and the idle clocks (FEATURES E2/E3). */
function Readout({ diag }: { diag: Diagnostics | null }) {
  const state = (blocked: boolean | undefined) => (
    <span className={`state ${blocked ? "blocked" : "allowed"}`}>{blocked ? "blocked" : "allowed"}</span>
  );
  return (
    <>
      <div className="effect">
        <span className="label">System sleep</span>
        {state(diag?.system_sleep_blocked)}
        <span className="label">Display off</span>
        {state(diag?.display_blocked)}
        <span className="label">Screen lock</span>
        {state(diag?.lock_blocked)}
        <span className="label">Mouse moves</span>
        <span className="state" style={diag?.input_blocked ? { color: "var(--error)" } : undefined}>
          {diag?.input_enabled ? (diag.input_blocked ? "blocked" : "on") : "off"}
        </span>
      </div>
      {diag?.input_blocked && (
        <p className="note error" style={{ marginTop: 12 }}>
          Input is being discarded: an app running as administrator has focus, so the move goes nowhere.
        </p>
      )}
      <div style={{ marginTop: 12 }}>
        <div className="row"><span className="k">Memory</span><span className="v">{diag ? `${diag.memory_mb.toFixed(1)} MB` : "-"}</span></div>
        <div className="row"><span className="k">System idle</span><span className="v">{diag ? fmtIdle(diag.system_idle_secs) : "-"}</span></div>
        <div className="row"><span className="k">Your idle</span><span className="v">{diag ? fmtIdle(diag.human_idle_secs) : "-"}</span></div>
        {diag?.remote_session && (
          <div className="row"><span className="k">Session</span><span className="v">remote (RDP or similar)</span></div>
        )}
      </div>
    </>
  );
}

function Activity() {
  const [logs, setLogs] = useState<string[]>([]);
  const load = useCallback(() => {
    invoke<string[]>("get_logs", { limit: 100 }).then(setLogs).catch(() => {});
  }, []);
  useEffect(load, [load]);
  return (
    <>
      <div className="log">{logs.length ? logs.join("\n") : "No activity yet."}</div>
      <button className="btn" style={{ marginTop: 8 }} onClick={load}>
        Refresh
      </button>
    </>
  );
}

function ImportMoveMouse() {
  const [path, setPath] = useState("");
  const [report, setReport] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const run = () => {
    setError(null);
    setReport(null);
    invoke<string[]>("import_move_mouse", { path }).then(setReport).catch((e) => setError(String(e)));
  };
  return (
    <>
      <div style={{ display: "flex", gap: 8 }}>
        <input
          className="btn"
          style={{ flex: 1 }}
          placeholder="Leave empty to find it, or paste the path to Settings.xml"
          value={path}
          onChange={(e) => setPath(e.target.value)}
        />
        <button className="btn primary" onClick={run}>
          Import
        </button>
      </div>
      {error && <p className="note error">{error}</p>}
      {report && (
        <ul className="note" style={{ marginTop: 8, paddingLeft: 18 }}>
          {report.map((l, i) => (
            <li key={i} style={{ marginBottom: 4 }}>
              {l}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

export default function Advanced() {
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const refresh = useCallback(() => {
    invoke<Diagnostics>("get_diagnostics").then(setDiag).catch(() => {});
  }, []);
  useEffect(() => {
    refresh();
    const t = window.setInterval(refresh, 2000);
    return () => window.clearInterval(t);
  }, [refresh]);

  return (
    <>
      <h1>Advanced</h1>
      <p className="note">
        These work alongside Start/Stop. Rules and the timer keep the PC awake on their own conditions,
        even while Home says Stopped. None of them move the mouse.
      </p>
      <section className="section">
        <h2>Profiles</h2>
        <ProfileSwitcher onChange={refresh} />
        <ProfileManager onChange={refresh} />
      </section>
      <section className="section">
        <h2>Keep awake for a while</h2>
        <Timer onChange={refresh} />
      </section>
      <section className="section">
        <RulesPage />
      </section>
      <section className="section">
        <h2>What Windows is being asked for</h2>
        <Readout diag={diag} />
      </section>
      <section className="section">
        <WhyAwake />
      </section>
      <section className="section">
        <h2>Activity</h2>
        <Activity />
      </section>
      <section className="section">
        <h2>Import from Move Mouse</h2>
        <ImportMoveMouse />
      </section>
    </>
  );
}
