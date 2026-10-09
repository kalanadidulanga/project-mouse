// Appearance (spec 006 FR-015 to FR-017).
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Appearance as A } from "./types";
import { SettingRow, Switch } from "./controls";

export default function Appearance() {
  const [a, setA] = useState<A | null>(null);
  useEffect(() => {
    invoke<A>("get_appearance").then(setA).catch(() => {});
  }, []);
  const save = (next: A) => {
    setA(next);
    invoke("set_appearance", { appearance: next }).catch(() => {});
  };
  if (!a) return <h1>Appearance</h1>;
  return (
    <>
      <h1>Appearance</h1>
      <section className="section first">
        <SettingRow title="Always on top" hint="Keep this window above other windows.">
          <Switch label="Always on top" on={a.always_on_top} onChange={(v) => save({ ...a, always_on_top: v })} />
        </SettingRow>
        <SettingRow title="Taskbar dot" hint="Green while running, yellow while paused, on the taskbar button while this window is open.">
          <Switch label="Taskbar dot" on={a.taskbar_dot} onChange={(v) => save({ ...a, taskbar_dot: v })} />
        </SettingRow>
        <SettingRow
          title="Notifications"
          hint="Only for things you didn't do yourself."
          help="A schedule starting or stopping it, Run for ending, or Windows blocking a move. Your own clicks never notify."
        >
          <Switch label="Notifications" on={a.notifications} onChange={(v) => save({ ...a, notifications: v })} />
        </SettingRow>
      </section>
    </>
  );
}
