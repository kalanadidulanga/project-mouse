// Settings (spec 005 US4). Everything applies immediately, running or not.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { RunSettings } from "./types";
import { SettingRow, Switch } from "./controls";
import { UpdateSettings } from "./updates";

export default function Settings() {
  const [run, setRun] = useState<RunSettings | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [autostartErr, setAutostartErr] = useState<string | null>(null);

  useEffect(() => {
    invoke<RunSettings>("get_run_settings").then(setRun).catch(() => {});
    invoke<boolean>("get_autostart").then(setAutostart).catch(() => {});
  }, []);

  const saveRun = (next: RunSettings) => {
    setRun(next);
    invoke("set_run_settings", { settings: next }).catch(() => {});
  };
  const toggleAutostart = (on: boolean) => {
    setAutostartErr(null);
    invoke<boolean>("set_autostart", { enabled: on })
      .then(setAutostart)
      .catch((e) => setAutostartErr(String(e)));
  };

  if (!run) return <h1>Settings</h1>;

  return (
    <>
      <h1>Settings</h1>

      <section className="section">
        <h2>When running</h2>
        <SettingRow
          title="Move the mouse"
          hint="Off: Start only keeps the PC awake. The screen can still lock, and Teams or Slack can still show you as away."
        >
          <Switch label="Move the mouse" on={run.move_mouse} onChange={(v) => saveRun({ ...run, move_mouse: v })} />
        </SettingRow>
        <SettingRow title="Keep the screen on" hint="Off: the PC stays awake, but the screen may turn off.">
          <Switch label="Keep the screen on" on={run.keep_screen_on} onChange={(v) => saveRun({ ...run, keep_screen_on: v })} />
        </SettingRow>
      </section>

      <section className="section">
        <h2>Starting</h2>
        <SettingRow title="Start automatically when project-mouse opens">
          <Switch
            label="Start automatically when project-mouse opens"
            on={run.start_on_launch}
            onChange={(v) => saveRun({ ...run, start_on_launch: v })}
          />
        </SettingRow>
        <SettingRow title="Start project-mouse with Windows" hint="It opens in the tray, without this window.">
          <Switch label="Start project-mouse with Windows" on={autostart} onChange={toggleAutostart} />
        </SettingRow>
        {autostartErr && <p className="note error">{autostartErr}</p>}
        <p className="note">
          Ctrl+Alt+K starts and stops it from anywhere.
        </p>
      </section>

      <UpdateSettings />

      <section className="section">
        <h2>About</h2>
        <p className="note">Source and issues: github.com/kalanadidulanga/project-mouse</p>
        <p className="note">
          It does not change your power plan, and it lets go of everything when you quit. With Move the
          mouse off it sends no input at all, so it cannot keep the screen from locking or keep a chat
          status active.
        </p>
      </section>
    </>
  );
}
