// Schedules and Blackouts (spec 006 FR-010, FR-011): Move Mouse's two time tabs. Every change is
// saved at once. A blackout whose end equals its start is saved but flagged as empty on its row.
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EVERY_DAY, WEEKDAYS, type Blackout, type Schedule, type Timetable } from "./types";
import { DaysPicker, Switch, TimeField } from "./controls";

function useTimetable(): [Timetable | null, (t: Timetable) => void] {
  const [t, setT] = useState<Timetable | null>(null);
  const latest = useRef(0);
  useEffect(() => {
    invoke<Timetable>("get_timetable").then(setT).catch(() => {});
  }, []);
  // Only the newest request may touch the screen; a failed save re-reads the stored timetable.
  const save = (next: Timetable) => {
    const n = ++latest.current;
    setT(next);
    invoke<Timetable>("set_timetable", { timetable: next })
      .then((r) => n === latest.current && setT(r))
      .catch(() =>
        invoke<Timetable>("get_timetable")
          .then((r) => n === latest.current && setT(r))
          .catch(() => {}),
      );
  };
  return [t, save];
}

export function Schedules() {
  const [t, save] = useTimetable();
  if (!t) return <h1>Schedules</h1>;
  const set = (i: number, s: Schedule) => save({ ...t, schedules: t.schedules.map((x, j) => (j === i ? s : x)) });
  const add = (...s: Schedule[]) => save({ ...t, schedules: [...t.schedules, ...s] });

  return (
    <>
      <h1>Schedules</h1>
      <p className="note">
        Start or stop at set times. If you start or stop it yourself in between, that stands until the next scheduled
        time. If project-mouse opens after today's start time, it starts then. It has to be running in the tray, so turn on Start with Windows (Behaviour).
      </p>
      {t.schedules.length === 0 && <p className="empty">No schedules yet.</p>}
      {t.schedules.map((s, i) => (
        <div className="entry" key={i}>
          <select className="btn" aria-label={`Action for schedule ${i + 1}`} value={s.action} onChange={(e) => set(i, { ...s, action: e.target.value as Schedule["action"] })}>
            <option value="Start">Start</option>
            <option value="Stop">Stop</option>
          </select>
          <span>at</span>
          <TimeField label={`Time for schedule ${i + 1}`} minutes={s.at} onChange={(m) => set(i, { ...s, at: m })} />
          <DaysPicker days={s.days} onChange={(d) => set(i, { ...s, days: d })} />
          <span className="grow" />
          <Switch label={`Enabled, schedule ${i + 1}`} on={s.enabled} onChange={(v) => set(i, { ...s, enabled: v })} />
          <button className="btn icon-btn" aria-label={`Delete schedule ${i + 1}`} onClick={() => save({ ...t, schedules: t.schedules.filter((_, j) => j !== i) })}>
            ✕
          </button>
        </div>
      ))}
      <div className="cond-row" style={{ marginTop: 12 }}>
        <button
          className="btn primary"
          onClick={() =>
            add(
              { days: WEEKDAYS, at: 9 * 60, action: "Start", enabled: true },
              { days: WEEKDAYS, at: 18 * 60, action: "Stop", enabled: true },
            )
          }
        >
          + Work hours (Mon to Fri, 09:00 to 18:00)
        </button>
        <button className="btn" onClick={() => add({ days: WEEKDAYS, at: 9 * 60, action: "Start", enabled: true })}>
          + Start time
        </button>
        <button className="btn" onClick={() => add({ days: WEEKDAYS, at: 18 * 60, action: "Stop", enabled: true })}>
          + Stop time
        </button>
      </div>
    </>
  );
}

export function Blackouts() {
  const [t, save] = useTimetable();
  if (!t) return <h1>Blackouts</h1>;
  const set = (i: number, b: Blackout) => save({ ...t, blackouts: t.blackouts.map((x, j) => (j === i ? b : x)) });
  const add = (b: Blackout) => save({ ...t, blackouts: [...t.blackouts, b] });

  return (
    <>
      <h1>Blackouts</h1>
      <p className="note">
        Quiet times with no mouse moves, such as lunch. The PC stays awake, so it carries on afterwards. A window can
        cross midnight; its days are the days it starts on.
      </p>
      {t.blackouts.length === 0 && <p className="empty">No blackouts yet.</p>}
      {t.blackouts.map((b, i) => (
        <div className="entry" key={i}>
          <TimeField label={`From, blackout ${i + 1}`} minutes={b.from} onChange={(m) => set(i, { ...b, from: m })} />
          <span>to</span>
          <TimeField label={`To, blackout ${i + 1}`} minutes={b.to} onChange={(m) => set(i, { ...b, to: m })} />
          <DaysPicker days={b.days} onChange={(d) => set(i, { ...b, days: d })} />
          {b.from === b.to && <span className="note error">This blackout is empty: its end is the same as its start.</span>}
          <span className="grow" />
          <Switch label={`Enabled, blackout ${i + 1}`} on={b.enabled} onChange={(v) => set(i, { ...b, enabled: v })} />
          <button className="btn icon-btn" aria-label={`Delete blackout ${i + 1}`} onClick={() => save({ ...t, blackouts: t.blackouts.filter((_, j) => j !== i) })}>
            ✕
          </button>
        </div>
      ))}
      <div className="cond-row" style={{ marginTop: 12 }}>
        <button className="btn primary" onClick={() => add({ days: WEEKDAYS, from: 12 * 60 + 30, to: 13 * 60 + 30, enabled: true })}>
          + Lunch (Mon to Fri, 12:30 to 13:30)
        </button>
        <button className="btn" onClick={() => add({ days: EVERY_DAY, from: 12 * 60, to: 13 * 60, enabled: true })}>
          + Blackout
        </button>
      </div>
    </>
  );
}
