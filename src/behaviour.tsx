// Behaviour (spec 006 FR-008, FR-009, FR-024, FR-026, FR-027): what Start does, when it pauses,
// what keeps the PC awake on its own, and how it starts.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RunSettings } from "./types";
import { SettingRow, Switch } from "./controls";

export default function Behaviour() {
  const [run, setRun] = useState<RunSettings | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [autostartErr, setAutostartErr] = useState<string | null>(null);
  const [apps, setApps] = useState<string[]>([]);
  const [running, setRunning] = useState<string[]>([]);
  const [draft, setDraft] = useState("");

  const readAutostart = useCallback(() => {
    invoke<boolean>("get_autostart").then(setAutostart).catch(() => {});
  }, []);

  useEffect(() => {
    invoke<RunSettings>("get_run_settings").then(setRun).catch(() => {});
    invoke<string[]>("get_apps").then(setApps).catch(() => {});
    invoke<string[]>("list_running_apps").then(setRunning).catch(() => {});
    readAutostart();
    // The tray can flip Start with Windows too; stay in step with it.
    const un = listen("state:changed", readAutostart);
    return () => {
      un.then((f) => f());
    };
  }, [readAutostart]);

  const saveRun = (next: RunSettings) => {
    setRun(next);
    invoke("set_run_settings", { settings: next }).catch(() => {});
  };
  const saveApps = (names: string[]) =>
    invoke<string[]>("set_apps", { names }).then(setApps).catch(() => {});
  const addApp = () => {
    if (draft.trim()) saveApps([...apps, draft.trim()]).then(() => setDraft(""));
  };
  const toggleAutostart = (on: boolean) => {
    setAutostartErr(null);
    invoke<boolean>("set_autostart", { enabled: on })
      .then(setAutostart)
      .catch((e) => setAutostartErr(String(e)));
  };

  if (!run) return <h1>Behaviour</h1>;

  return (
    <>
      <h1>Behaviour</h1>

      <section className="section first">
        <h2>When running</h2>
        <SettingRow
          title="Move the mouse"
          hint="Off: Start only keeps the PC awake."
          help="With this off, nothing is sent: the screen can still lock, and Teams or Slack can still show you as away."
        >
          <Switch label="Move the mouse" on={run.move_mouse} onChange={(v) => saveRun({ ...run, move_mouse: v })} />
        </SettingRow>
        <SettingRow title="Keep the screen on" hint="Off: the PC stays awake, but the screen may turn off.">
          <Switch label="Keep the screen on" on={run.keep_screen_on} onChange={(v) => saveRun({ ...run, keep_screen_on: v })} />
        </SettingRow>
      </section>

      <section className="section">
        <h2>Pause automatically</h2>
        <SettingRow title="On battery" hint="Lets the PC sleep to save the battery. Carries on when you plug in.">
          <Switch label="Pause on battery" on={run.pause_on_battery} onChange={(v) => saveRun({ ...run, pause_on_battery: v })} />
        </SettingRow>
        <SettingRow title="When the screen is locked" hint="Moves stop and the PC stays awake. Carries on when you unlock.">
          <Switch label="Pause when the screen is locked" on={run.pause_when_locked} onChange={(v) => saveRun({ ...run, pause_when_locked: v })} />
        </SettingRow>
        <SettingRow
          title="While presenting"
          hint="Presentations, full-screen videos and games. Moves stop and the PC stays awake."
          help="Windows reports when presentation mode is on or an app is full screen. Nothing on screen twitches while people are watching."
        >
          <Switch label="Pause while presenting" on={run.pause_when_presenting} onChange={(v) => saveRun({ ...run, pause_when_presenting: v })} />
        </SettingRow>
      </section>

      <section className="section">
        <h2>Keep awake while these apps run</h2>
        <p className="note">
          Whenever one of these is running, the PC stays awake, even while Home says Stopped. No mouse moves. Good for
          builds, renders and downloads.
        </p>
        {apps.length === 0 && <p className="empty">No apps yet.</p>}
        {apps.map((a) => (
          <div className="entry" key={a}>
            <span className="grow">{a}</span>
            <button className="btn icon-btn" aria-label={`Remove ${a}`} onClick={() => saveApps(apps.filter((x) => x !== a))}>
              ✕
            </button>
          </div>
        ))}
        <div className="cond-row" style={{ marginTop: 10 }}>
          <input
            className="btn"
            style={{ flex: 1 }}
            list="running-apps"
            placeholder="App name, e.g. msbuild.exe"
            aria-label="App to add"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addApp()}
          />
          <datalist id="running-apps">
            {running.map((n) => (
              <option key={n} value={n} />
            ))}
          </datalist>
          <button className="btn primary" onClick={addApp}>
            Add
          </button>
        </div>
      </section>

      <section className="section">
        <h2>Starting</h2>
        <SettingRow title="Start automatically when project-mouse opens">
          <Switch label="Start automatically when project-mouse opens" on={run.start_on_launch} onChange={(v) => saveRun({ ...run, start_on_launch: v })} />
        </SettingRow>
        <SettingRow title="Start project-mouse with Windows" hint="It opens in the tray, without this window.">
          <Switch label="Start project-mouse with Windows" on={autostart} onChange={toggleAutostart} />
        </SettingRow>
        {autostartErr && <p className="note error">{autostartErr}</p>}
        <p className="note">Ctrl+Alt+K starts and stops it from anywhere.</p>
      </section>

    </>
  );
}
