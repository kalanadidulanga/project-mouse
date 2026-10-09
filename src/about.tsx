// About (spec 006 FR-024, FR-029, FR-030): what this is, updates, import, and Troubleshooting in
// plain words.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { clock, type Status } from "./types";
import { UpdateSettings } from "./updates";

type LegacyRule = { id: string; name: string; enabled: boolean };
type ProfileView = { id: string; name: string; rules: LegacyRule[] };

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

function fmtIdle(secs: number): string {
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return m < 60 ? `${m}m ${s}s` : `${Math.floor(m / 60)}h ${m % 60}m`;
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
          aria-label="Path to Move Mouse Settings.xml"
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

/** Rules from an earlier version (FR-029): anything that is not the apps list, shown so nothing
 *  keeps the PC awake invisibly. Hidden when there are none. */
function LegacyRules() {
  const [rules, setRules] = useState<LegacyRule[]>([]);
  const load = useCallback(() => {
    invoke<ProfileView>("get_rules")
      .then((p) => setRules(p.rules.filter((r) => r.id !== "apps")))
      .catch(() => {});
  }, []);
  useEffect(load, [load]);
  if (!rules.length) return null;
  return (
    <>
      <h2>Rules from an earlier version</h2>
      <p className="note">These keep the PC awake on their own conditions. Turn them off or delete them if you don't need them.</p>
      {rules.map((r) => (
        <div className="entry" key={r.id}>
          <span className="grow">{r.name}</span>
          <label className="note">
            <input type="checkbox" aria-label={`Enabled, ${r.name}`} checked={r.enabled} onChange={(e) => invoke("set_rule_enabled", { id: r.id, enabled: e.target.checked }).then(load)} /> on
          </label>
          <button className="btn icon-btn" aria-label={`Delete ${r.name}`} onClick={() => invoke("delete_rule", { id: r.id }).then(load)}>
            ✕
          </button>
        </div>
      ))}
    </>
  );
}

export default function About() {
  const [s, setS] = useState<Status | null>(null);
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  useEffect(() => {
    const read = () => {
      invoke<Status>("get_status").then(setS).catch(() => {});
      invoke<Diagnostics>("get_diagnostics").then(setDiag).catch(() => {});
    };
    read();
    const t = window.setInterval(read, 1000);
    return () => window.clearInterval(t);
  }, []);

  return (
    <>
      <h1>About</h1>
      <section className="section first">
        <p className="note">Source and issues: github.com/kalanadidulanga/project-mouse</p>
        <p className="note">{s ? `System idle for ${clock(s.idle_secs)}` : " "}</p>
        <p className="note">
          It does not change your power plan, and it lets go of everything when you quit. Monitoring software can detect
          simulated input. With Move the mouse off it sends no input at all, so it cannot keep the screen from locking or
          keep a chat status active.
        </p>
      </section>

      <UpdateSettings />

      <section className="section">
        <h2>Import from Move Mouse</h2>
        <ImportMoveMouse />
      </section>

      <details className="section troubleshoot">
        <summary>Troubleshooting</summary>
        <h2>What Windows is being asked for</h2>
        <Readout diag={diag} />
        <WhyAwake />
        <h2>Recent activity</h2>
        <Activity />
        <LegacyRules />
      </details>
    </>
  );
}
