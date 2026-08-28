import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import RulesPage, { Mode, Rule, ProfileView, modeWord } from "./rules";
import FirstRun from "./firstrun";
import "./styles.css";

type StateView = {
  effective_mode: string;
  manual_mode: string;
  paused: boolean;
  profile: string;
};

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

type InputSettings = {
  interval_secs: number;
  idle_threshold_secs: number;
  key: number;
};

type Page = "status" | "rules" | "activity" | "settings";

const MODE_LABEL: Record<string, string> = {
  off: "Not holding anything",
  keep_running: "Keeping the system awake",
  keep_presenting: "Keeping the display on",
};

function fmtIdle(secs: number): string {
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return m < 60 ? `${m}m ${s}s` : `${Math.floor(m / 60)}h ${m % 60}m`;
}

export default function App() {
  const [page, setPage] = useState<Page>("status");
  const [state, setState] = useState<StateView | null>(null);
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [firstRun, setFirstRun] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<boolean>("is_first_run").then(setFirstRun).catch(() => setFirstRun(false));
  }, []);

  const refresh = useCallback(async () => {
    try {
      setState(await invoke<StateView>("get_state"));
      setDiag(await invoke<Diagnostics>("get_diagnostics"));
    } catch {
      /* window may be closing */
    }
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen("state:changed", refresh);
    // Text-only refresh for the live idle/memory readouts (UI-UX §4 allows a per-second text tick).
    const timer = window.setInterval(refresh, 2000);
    return () => {
      unlisten.then((f) => f());
      window.clearInterval(timer);
    };
  }, [refresh]);

  useEffect(() => {
    if (page === "activity") {
      invoke<string[]>("get_logs", { limit: 100 }).then(setLogs).catch(() => {});
    }
  }, [page]);

  const setMode = (mode: string) => invoke("set_mode", { mode }).then(refresh);
  const togglePause = () =>
    invoke(state?.paused ? "resume_all" : "pause_all").then(refresh);

  // Hold the shell back until we know — flashing the full UI and then replacing it with the
  // first-run question would be worse than one frame of nothing.
  if (firstRun === null) return <div className="app" />;
  if (firstRun) return <FirstRun onDone={() => { setFirstRun(false); refresh(); }} />;

  return (
    <div className="app">
      <nav className="rail">
        <div className="brand">project-mouse</div>
        {(["status", "rules", "activity", "settings"] as Page[]).map((p) => (
          <button
            key={p}
            className={page === p ? "active" : ""}
            onClick={() => setPage(p)}
          >
            {p[0].toUpperCase() + p.slice(1)}
          </button>
        ))}
        <div className="spacer" />
      </nav>

      <main className="content">
        {page === "status" && (
          <StatusPage
            state={state}
            diag={diag}
            onMode={setMode}
            onTogglePause={togglePause}
            onRefresh={refresh}
          />
        )}
        {page === "rules" && <RulesPage />}
        {page === "activity" && <ActivityPage logs={logs} />}
        {page === "settings" && (
          <SettingsPage
            state={state}
            diag={diag}
            onMode={setMode}
            onRefresh={refresh}
          />
        )}
      </main>
    </div>
  );
}

/** "Keep awake for 2 hours" — a rule with an `ExpiryAt` condition, so it releases itself.
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
  // way, so a stale one holds nothing — sweep it in the scheduler tick if that ever stops being true.
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
 *  the second is complete — see specs/003-settings-ui/research.md R1. */
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
    <div className="why">
      <strong style={{ fontSize: 13 }}>Why is my PC awake?</strong>

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
        It does not name the program, and it does not cover every kind of request — so treat it as
        a hint, not an inventory. To get the full list with names, run this from an elevated
        prompt:
      </p>
      <div className="cmd">
        <code>{ELEVATED_CMD}</code>
        <button className="btn" onClick={copy}>{copied ? "Copied" : "Copy"}</button>
      </div>
    </div>
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
            {p.active ? " — active" : ""}
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

function StatusPage({
  state,
  diag,
  onMode,
  onTogglePause,
  onRefresh,
}: {
  state: StateView | null;
  diag: Diagnostics | null;
  onMode: (m: string) => void;
  onTogglePause: () => void;
  onRefresh: () => void;
}) {
  const eff = state?.effective_mode ?? "off";
  const active = eff !== "off";
  return (
    <>
      <h1>Status</h1>
      <div className="status-card">
        <div className={`status-line ${active ? "active" : ""}`}>
          {state?.paused ? "Paused" : MODE_LABEL[eff]}
        </div>
        <div className="status-sub">{diag?.reason ?? " "}</div>
        <div
          className={`switch ${state?.paused ? "on" : ""}`}
          role="switch"
          aria-checked={state?.paused ?? false}
          tabIndex={0}
          onClick={onTogglePause}
          onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onTogglePause()}
        >
          <span className="track"><span className="thumb" /></span>
          <span>{state?.paused ? "Resume" : "Pause"}</span>
        </div>
      </div>

      <ModeButtons manual={state?.manual_mode} onMode={onMode} />
      <Timer onChange={onRefresh} />

      <div className="effect" style={{ marginTop: 20 }}>
        <span className="label">System sleep</span>
        <span className={`state ${diag?.system_sleep_blocked ? "blocked" : "allowed"}`}>
          {diag?.system_sleep_blocked ? "blocked" : "allowed"}
        </span>
        <span className="label">Display off</span>
        <span className={`state ${diag?.display_blocked ? "blocked" : "allowed"}`}>
          {diag?.display_blocked ? "blocked" : "allowed"}
        </span>
        <span className="label">Screen lock</span>
        <span className={`state ${diag?.lock_blocked ? "blocked" : "allowed"}`}>
          {diag?.lock_blocked ? "blocked" : "allowed"}
        </span>
        <span className="label">Input synthesis</span>
        <span className={`state ${diag?.input_blocked ? "" : "allowed"}`} style={diag?.input_blocked ? { color: "var(--error)" } : undefined}>
          {diag?.input_enabled ? (diag.input_blocked ? "blocked" : "on") : "off"}
        </span>
      </div>

      {diag?.input_blocked && (
        <p className="note" style={{ color: "var(--error)", marginTop: 12 }}>
          Input is being discarded — an elevated window has focus, so synthesized input goes nowhere.
        </p>
      )}

      <WhyAwake />

      <div style={{ marginTop: 20 }}>
        <ProfileSwitcher onChange={onRefresh} />
        <div className="row"><span className="k">Memory</span><span className="v">{diag ? `${diag.memory_mb.toFixed(1)} MB` : "—"}</span></div>
        <div className="row"><span className="k">System idle</span><span className="v">{diag ? fmtIdle(diag.system_idle_secs) : "—"}</span></div>
        <div className="row"><span className="k">Human idle</span><span className="v">{diag ? fmtIdle(diag.human_idle_secs) : "—"}</span></div>
        {diag?.remote_session && (
          <div className="row"><span className="k">Session</span><span className="v">remote (RDP or similar)</span></div>
        )}
      </div>
    </>
  );
}

function ModeButtons({ manual, onMode }: { manual?: string; onMode: (m: string) => void }) {
  const modes: [string, string][] = [
    ["off", "Off"],
    ["keep_running", "Keep running"],
    ["keep_presenting", "Keep presenting"],
  ];
  return (
    <div className="btn-group">
      {modes.map(([id, label]) => (
        <button
          key={id}
          className={`btn ${manual === id ? "selected" : ""}`}
          onClick={() => onMode(id)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function ActivityPage({ logs }: { logs: string[] }) {
  return (
    <>
      <h1>Activity</h1>
      <div className="log">{logs.length ? logs.join("\n") : "No activity yet."}</div>
    </>
  );
}

/** Virtual-key codes worth offering. F15 is the category's convention (Caffeine); it is also the
 *  one that breaks in PuTTY and Google Docs, which is exactly why the choice is the user's. */
const KEYS: [number, string][] = [
  [0, "virtual jiggle (nothing moves on screen)"],
  [0x7e, "F15"],
  [0x91, "Scroll Lock"],
  [0x10, "Shift"],
];

function InputSettingsForm({ onChanged }: { onChanged: () => void }) {
  const [s, setS] = useState<InputSettings | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    invoke<InputSettings>("get_input_settings").then(setS).catch(() => {});
  }, []);

  if (!s) return null;

  const save = (next: InputSettings) => {
    setS(next);
    // Rust clamps; show what actually took effect rather than what was typed.
    invoke<InputSettings>("set_input_settings", { settings: next }).then((applied) => {
      setS(applied);
      setSaved(true);
      window.setTimeout(() => setSaved(false), 1500);
      onChanged();
    });
  };

  return (
    <div style={{ marginTop: 12 }}>
      <div className="row">
        <span className="k">Jiggle every</span>
        <span className="v">
          <input
            className="btn"
            type="number"
            min={5}
            max={3600}
            style={{ width: 90 }}
            value={s.interval_secs}
            onChange={(e) => setS({ ...s, interval_secs: Number(e.target.value) })}
            onBlur={() => save(s)}
          />{" "}
          seconds
        </span>
      </div>
      <div className="row">
        <span className="k">Only after you have been idle for</span>
        <span className="v">
          <input
            className="btn"
            type="number"
            min={0}
            max={86400}
            style={{ width: 90 }}
            value={s.idle_threshold_secs}
            onChange={(e) => setS({ ...s, idle_threshold_secs: Number(e.target.value) })}
            onBlur={() => save(s)}
          />{" "}
          seconds
        </span>
      </div>
      <div className="row">
        <span className="k">What to send</span>
        <span className="v">
          <select
            className="btn"
            value={s.key}
            onChange={(e) => save({ ...s, key: Number(e.target.value) })}
          >
            {KEYS.map(([code, label]) => (
              <option key={code} value={code}>
                {label}
              </option>
            ))}
          </select>
        </span>
      </div>
      <p className="note">
        {saved ? "Saved." : "Interval is clamped to 5 s–1 h."}
      </p>
    </div>
  );
}

function SettingsPage({
  state,
  diag,
  onMode,
  onRefresh,
}: {
  state: StateView | null;
  diag: Diagnostics | null;
  onMode: (m: string) => void;
  onRefresh: () => void;
}) {
  const inputOn = diag?.input_enabled ?? false;
  const toggleInput = () =>
    invoke("set_input_enabled", { enabled: !inputOn }).then(onRefresh);
  const [mmPath, setMmPath] = useState("");
  const [mmReport, setMmReport] = useState<string[] | null>(null);
  const [mmError, setMmError] = useState<string | null>(null);
  const importMM = () => {
    setMmError(null);
    setMmReport(null);
    invoke<string[]>("import_move_mouse", { path: mmPath })
      .then(setMmReport)
      .catch((e) => setMmError(String(e)));
  };
  return (
    <>
      <h1>Settings</h1>
      <p className="note">Set the mode directly:</p>
      <ModeButtons manual={state?.manual_mode} onMode={onMode} />

      <div style={{ marginTop: 24, borderTop: "1px solid var(--border)", paddingTop: 16 }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <strong style={{ fontSize: 13 }}>Synthesize input</strong>
          <div
            className={`switch ${inputOn ? "on" : ""}`}
            role="switch"
            aria-checked={inputOn}
            tabIndex={0}
            style={{ marginTop: 0 }}
            onClick={toggleInput}
            onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && toggleInput()}
          >
            <span className="track"><span className="thumb" /></span>
            <span>{inputOn ? "On" : "Off"}</span>
          </div>
        </div>
        <p className="note" style={{ marginTop: 8 }}>
          With this on, the app synthesizes input — a virtual jiggle while you are idle — to reset
          session and presence timers that keeping the machine awake cannot. Synthesized input is
          detectable and may be against an acceptable-use policy. It stands down the moment you
          return, and stops the instant you turn this off.
        </p>
        {inputOn && <InputSettingsForm onChanged={onRefresh} />}
      </div>

      <div style={{ marginTop: 24, borderTop: "1px solid var(--border)", paddingTop: 16 }}>
        <strong style={{ fontSize: 13 }}>Profiles</strong>
        <p className="note" style={{ marginTop: 4 }}>
          Each profile is its own set of rules. The tray menu switches between them too.
        </p>
        <ProfileManager onChange={onRefresh} />
      </div>

      <div style={{ marginTop: 24, borderTop: "1px solid var(--border)", paddingTop: 16 }}>
        <strong style={{ fontSize: 13 }}>Import from Move Mouse</strong>
        <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
          <input
            className="btn"
            style={{ flex: 1 }}
            placeholder="path to Settings.xml"
            value={mmPath}
            onChange={(e) => setMmPath(e.target.value)}
          />
          <button className="btn primary" onClick={importMM}>Import</button>
        </div>
        {mmError && <p className="note" style={{ color: "var(--error)" }}>{mmError}</p>}
        {mmReport && (
          <ul className="note" style={{ marginTop: 8, paddingLeft: 18 }}>
            {mmReport.map((l, i) => <li key={i} style={{ marginBottom: 4 }}>{l}</li>)}
          </ul>
        )}
      </div>

      <p className="note" style={{ marginTop: 20 }}>
        Start with Windows and Quit are on the tray menu. Toggle the wake lock any time with
        <span className="mono-hint"> Ctrl+Alt+K</span>.
      </p>
      <p className="note">
        With input synthesis off (the default), this tool cannot defeat a screen lock or a chat
        presence indicator. It does not modify your power plan, and releases everything on exit.
      </p>
    </>
  );
}
