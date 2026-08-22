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
        {page === "rules" && <RulesPage profile={state?.profile} />}
        {page === "activity" && <ActivityPage logs={logs} />}
        {page === "settings" && <SettingsPage state={state} onMode={setMode} />}
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
        <span className="state allowed">off</span>
      </div>

      <div style={{ marginTop: 20 }}>
        <div className="row"><span className="k">Profile</span><span className="v">{state?.profile ?? "—"}</span></div>
        <div className="row"><span className="k">Memory</span><span className="v">{diag ? `${diag.memory_mb.toFixed(1)} MB` : "—"}</span></div>
        <div className="row"><span className="k">System idle</span><span className="v">{diag ? fmtIdle(diag.system_idle_secs) : "—"}</span></div>
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

function RulesPage({ profile }: { profile?: string }) {
  return (
    <>
      <h1>Rules</h1>
      <p className="note">
        Active profile: <strong>{profile ?? "—"}</strong>.
      </p>
      <p className="note">
        The plain-language rule builder arrives next. For now, bind the wake lock to a process from
        the command line, for example:
      </p>
      <p className="note">
        <span className="mono-hint">project-mouse --while-process msbuild.exe</span>
      </p>
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

function SettingsPage({ state, onMode }: { state: StateView | null; onMode: (m: string) => void }) {
  return (
    <>
      <h1>Settings</h1>
      <p className="note">Set the mode directly:</p>
      <ModeButtons manual={state?.manual_mode} onMode={onMode} />
      <p className="note" style={{ marginTop: 20 }}>
        Start with Windows and Quit are on the tray menu. Toggle the wake lock any time with
        <span className="mono-hint"> Ctrl+Alt+K</span>.
      </p>
      <p className="note">
        By default this tool synthesizes no input and cannot defeat a screen lock or a chat
        presence indicator. It does not modify your power plan, and releases everything on exit.
      </p>
    </>
  );
}
