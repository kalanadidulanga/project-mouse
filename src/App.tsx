import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
          />
        )}
        {page === "rules" && <RulesPage />}
        {page === "activity" && <ActivityPage logs={logs} />}
        {page === "settings" && (
          <SettingsPage
            state={state}
            diag={diag}
            onMode={setMode}
            onToggleInput={() =>
              invoke("set_input_enabled", { enabled: !diag?.input_enabled }).then(refresh)
            }
          />
        )}
      </main>
    </div>
  );
}

function StatusPage({
  state,
  diag,
  onMode,
  onTogglePause,
}: {
  state: StateView | null;
  diag: Diagnostics | null;
  onMode: (m: string) => void;
  onTogglePause: () => void;
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
        <div className="status-sub">{diag?.reason ?? " "}</div>
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

      <div style={{ marginTop: 20 }}>
        <div className="row"><span className="k">Profile</span><span className="v">{state?.profile ?? "—"}</span></div>
        <div className="row"><span className="k">Memory</span><span className="v">{diag ? `${diag.memory_mb.toFixed(1)} MB` : "—"}</span></div>
        <div className="row"><span className="k">System idle</span><span className="v">{diag ? fmtIdle(diag.system_idle_secs) : "—"}</span></div>
        <div className="row"><span className="k">Human idle</span><span className="v">{diag ? fmtIdle(diag.human_idle_secs) : "—"}</span></div>
      </div>

      <ModeButtons manual={state?.manual_mode} onMode={onMode} />
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

type Rule = {
  id: string;
  name: string;
  enabled: boolean;
  conditions: unknown[];
  mode: string; // "KeepRunning" | "KeepPresenting"
};
type ProfileView = { id: string; name: string; rules: Rule[] };

function newId(): string {
  try {
    return crypto.randomUUID();
  } catch {
    return `rule-${Date.now()}`;
  }
}

function RulesPage() {
  const [profile, setProfile] = useState<ProfileView | null>(null);
  const [proc, setProc] = useState("");
  const [mode, setMode] = useState("KeepRunning");

  const load = useCallback(() => {
    invoke<ProfileView>("get_rules").then(setProfile).catch(() => {});
  }, []);
  useEffect(load, [load]);

  const add = () => {
    const name = proc.trim();
    if (!name) return;
    const rule: Rule = {
      id: newId(),
      name: `Keep ${mode === "KeepPresenting" ? "presenting" : "running"} while ${name} runs`,
      enabled: false, // disabled by default (UI-UX §3)
      conditions: [{ ProcessRunning: [name] }],
      mode,
    };
    invoke("upsert_rule", { rule }).then(() => {
      setProc("");
      load();
    });
  };
  const toggle = (id: string, enabled: boolean) =>
    invoke("set_rule_enabled", { id, enabled }).then(load);
  const remove = (id: string) => invoke("delete_rule", { id }).then(load);

  return (
    <>
      <h1>Rules</h1>
      <p className="note">
        A rule keeps the machine awake while its condition holds. New rules start disabled — turn
        one on when you want it.
      </p>

      {profile && profile.rules.length > 0 ? (
        profile.rules.map((r) => (
          <div className="row" key={r.id}>
            <span className="k" style={{ color: "var(--text)" }}>{r.name}</span>
            <span style={{ display: "flex", gap: 12, alignItems: "center" }}>
              <label style={{ fontSize: 12, color: "var(--text-2)" }}>
                <input
                  type="checkbox"
                  checked={r.enabled}
                  onChange={(e) => toggle(r.id, e.target.checked)}
                />{" "}
                on
              </label>
              <button className="btn" onClick={() => remove(r.id)}>Delete</button>
            </span>
          </div>
        ))
      ) : (
        <p className="note">No rules yet.</p>
      )}

      <div style={{ marginTop: 20, borderTop: "1px solid var(--border)", paddingTop: 16 }}>
        <p className="note">Add a rule: keep awake while a process runs.</p>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <select className="btn" value={mode} onChange={(e) => setMode(e.target.value)}>
            <option value="KeepRunning">Keep running</option>
            <option value="KeepPresenting">Keep presenting</option>
          </select>
          <span className="note">while</span>
          <input
            className="btn"
            style={{ minWidth: 160 }}
            placeholder="process.exe"
            value={proc}
            onChange={(e) => setProc(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
          />
          <span className="note">is running</span>
          <button className="btn primary" onClick={add}>Add rule</button>
        </div>
      </div>
    </>
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

function SettingsPage({
  state,
  diag,
  onMode,
  onToggleInput,
}: {
  state: StateView | null;
  diag: Diagnostics | null;
  onMode: (m: string) => void;
  onToggleInput: () => void;
}) {
  const inputOn = diag?.input_enabled ?? false;
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
            onClick={onToggleInput}
            onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onToggleInput()}
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
