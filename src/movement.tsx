// Movement (spec 006 FR-001 to FR-006, FR-024): what happens, and when.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DIRECTIONS, KEYS, type InputSettings, type Motion, type Speed } from "./types";
import { NumberField, SettingRow, Switch } from "./controls";

const SPEEDS: [Speed, string][] = [["Slow", "Slow"], ["Normal", "Normal"], ["Fast", "Fast"], ["Custom", "Custom"]];

export default function Movement() {
  const [s, setS] = useState<InputSettings | null>(null);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    invoke<InputSettings>("get_input_settings").then(setS).catch(() => {});
  }, []);

  const save = (next: InputSettings) =>
    invoke<InputSettings>("set_input_settings", { settings: next }).then(setS).catch(() => {});

  const test = () => {
    invoke("test_move").catch(() => {});
    if (s && s.key === 0 && s.motion === "Virtual") {
      setNote("Sent an invisible move: the cursor stays put, but Windows saw input.");
      window.setTimeout(() => setNote(null), 2500);
    }
  };

  if (!s) return <h1>Movement</h1>;
  const mouse = s.key === 0;
  const visible = mouse && s.motion !== "Virtual";

  return (
    <>
      <h1>Movement</h1>

      <section className="section first">
        <h2>What happens</h2>
        <SettingRow
          title="Send"
          hint="Move the mouse, or press a key."
          help="A key press works where mouse movement is ignored, such as some remote desktops. F15 is a key no keyboard has, but a few apps (PuTTY, PowerPoint, Google Docs) still react to it."
        >
          <select className="btn" aria-label="Send" value={s.key} onChange={(e) => save({ ...s, key: Number(e.target.value) })}>
            {KEYS.map(([code, label]) => (
              <option key={code} value={code}>
                {label}
              </option>
            ))}
          </select>
        </SettingRow>

        {mouse && (
          <SettingRow
            title="Direction"
            hint="Every movement comes back to where it started."
            help="Invisible nudges the mouse one pixel and back in the same instant: Windows sees input and nothing moves on screen. Use it while you share your screen."
          >
            <select className="btn" aria-label="Direction" value={s.motion} onChange={(e) => save({ ...s, motion: e.target.value as Motion })}>
              {DIRECTIONS.map((g) => (
                <optgroup key={g.group} label={g.group}>
                  {g.items.map(([id, label]) => (
                    <option key={id} value={id}>
                      {label}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
          </SettingRow>
        )}

        {visible && (
          <>
            <SettingRow
              title="Distance"
              hint={s.distance_random ? "A new distance each time, between these two." : "How far each side goes."}
              help="Measured in screen pixels. The cursor always ends exactly where it started."
            >
              <NumberField label="Distance in pixels" value={s.distance_px} max={500} onCommit={(n) => save({ ...s, distance_px: n })} />
              {s.distance_random && (
                <>
                  to
                  <NumberField label="Largest distance in pixels" value={s.distance_max_px} max={500} onCommit={(n) => save({ ...s, distance_max_px: n })} />
                </>
              )}
              px
            </SettingRow>
            <SettingRow title="Random distance">
              <Switch label="Random distance" on={s.distance_random} onChange={(v) => save({ ...s, distance_random: v })} />
            </SettingRow>
            <SettingRow title="Speed" hint="How fast the cursor travels the path.">
              <select className="btn" aria-label="Speed" value={s.speed} onChange={(e) => save({ ...s, speed: e.target.value as Speed })}>
                {SPEEDS.map(([id, label]) => (
                  <option key={id} value={id}>
                    {label}
                  </option>
                ))}
              </select>
              {s.speed === "Custom" && (
                <>
                  <NumberField label="Milliseconds between steps" value={s.custom_step_ms} max={50} onCommit={(n) => save({ ...s, custom_step_ms: n })} />
                  ms
                </>
              )}
            </SettingRow>
            <SettingRow title="Stop if I move the mouse" hint="Touch the mouse while it moves and it lets go at once.">
              <Switch label="Stop if I move the mouse" on={s.abortable} onChange={(v) => save({ ...s, abortable: v })} />
            </SettingRow>
          </>
        )}
      </section>

      <section className="section">
        <h2>When</h2>
        <SettingRow
          title="Move after"
          hint={s.interval_random ? "A new wait each time, between these two." : "Seconds with no mouse or keyboard input."}
          help="The countdown restarts whenever you use the mouse or keyboard, so it never moves while you work. Teams usually marks you away after about five minutes without input."
        >
          <NumberField label="Seconds with no input" value={s.interval_secs} max={3_600} onCommit={(n) => save({ ...s, interval_secs: n })} />
          {s.interval_random && (
            <>
              to
              <NumberField label="Longest wait in seconds" value={s.interval_max_secs} max={3_600} onCommit={(n) => save({ ...s, interval_max_secs: n })} />
            </>
          )}
          s
        </SettingRow>
        <SettingRow title="Random wait" hint="So the moves don't line up with other things on a timer.">
          <Switch label="Random wait" on={s.interval_random} onChange={(v) => save({ ...s, interval_random: v })} />
        </SettingRow>
      </section>

      <section className="section">
        <SettingRow title="Try it" hint="Runs one movement now, even while stopped.">
          <button className="btn" onClick={test}>
            Test
          </button>
        </SettingRow>
        {note && (
          <p className="note" role="status">
            {note}
          </p>
        )}
      </section>
    </>
  );
}
