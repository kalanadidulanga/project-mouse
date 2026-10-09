// Settings (spec 005 US4). Everything applies immediately, running or not.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { KEYS, type InputSettings, type RunSettings } from "./types";
import { NumberField, SettingRow, Switch } from "./controls";
import { UpdateSettings } from "./updates";

export default function Settings() {
  const [run, setRun] = useState<RunSettings | null>(null);
  const [input, setInput] = useState<InputSettings | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [autostartErr, setAutostartErr] = useState<string | null>(null);

  useEffect(() => {
    invoke<RunSettings>("get_run_settings").then(setRun).catch(() => {});
    invoke<InputSettings>("get_input_settings").then(setInput).catch(() => {});
    invoke<boolean>("get_autostart").then(setAutostart).catch(() => {});
  }, []);

  const saveRun = (next: RunSettings) => {
    setRun(next);
    invoke("set_run_settings", { settings: next }).catch(() => {});
  };
  const saveInput = (next: InputSettings) =>
    invoke<InputSettings>("set_input_settings", { settings: next }).then(setInput).catch(() => {});
  const toggleAutostart = (on: boolean) => {
    setAutostartErr(null);
    invoke<boolean>("set_autostart", { enabled: on })
      .then(setAutostart)
      .catch((e) => setAutostartErr(String(e)));
  };

  if (!run || !input) return <h1>Settings</h1>;
  const visible = input.key === 0 && input.motion !== "Virtual";

  return (
    <>
      <h1>Settings</h1>

      <section className="section">
        <h2>When running</h2>
        <SettingRow
          title="Move the mouse"
          note="Off: Start only keeps the PC awake. The screen can still lock, and Teams or Slack can still show you as away."
        >
          <Switch label="Move the mouse" on={run.move_mouse} onChange={(v) => saveRun({ ...run, move_mouse: v })} />
        </SettingRow>
        <SettingRow title="Keep the screen on" note="Off: the PC stays awake, but the screen may turn off.">
          <Switch label="Keep the screen on" on={run.keep_screen_on} onChange={(v) => saveRun({ ...run, keep_screen_on: v })} />
        </SettingRow>
        <SettingRow
          title="What to send"
          note="A key press instead of a mouse move. F15 is a key no keyboard has, but a few apps (PuTTY, PowerPoint, Google Docs) still react to it."
        >
          <select
            className="btn"
            aria-label="What to send"
            value={input.key}
            onChange={(e) => saveInput({ ...input, key: Number(e.target.value) })}
          >
            {KEYS.map(([code, label]) => (
              <option key={code} value={code}>
                {label}
              </option>
            ))}
          </select>
        </SettingRow>
        {visible && (
          <SettingRow
            title="Distance"
            note="How far each side of the movement goes. Pointer speed settings can stretch it a little. It always comes back to where it started."
          >
            <NumberField label="Distance in pixels" value={input.distance_px} max={500} onCommit={(n) => saveInput({ ...input, distance_px: n })} />
            px
          </SettingRow>
        )}
        <SettingRow
          title="Vary by"
          note="Changes the wait and the distance a little each time, so the move doesn't line up with other things on a timer and the cursor doesn't land on the same pixel. 0 keeps them fixed."
        >
          <NumberField label="Vary by percent" value={input.vary_pct} max={50} onCommit={(n) => saveInput({ ...input, vary_pct: n })} />%
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
        <SettingRow title="Start project-mouse with Windows" note="It opens in the tray, without this window.">
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
