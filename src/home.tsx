// Home (spec 005 FR-010, UI-UX §0.5): what is true right now, one button, and the two settings
// people actually touch.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { KEYS, type InputSettings, type Motion, type Status } from "./types";
import { NumberField } from "./controls";
import { UpdateBanner } from "./updates";

const MOVEMENTS: [Motion, string][] = [
  ["Square", "Small square"],
  ["Line", "Back and forth"],
  ["Circle", "Small circle"],
  ["Virtual", "Invisible (the cursor doesn't move)"],
];

const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

function describe(s: Status): { title: string; detail: string; tone: "on" | "off" | "warn" } {
  switch (s.kind) {
    case "running":
      return {
        title: "Running",
        detail: s.next_move_in_secs == null ? "Starting…" : `Next move in ${clock(s.next_move_in_secs)}`,
        tone: "on",
      };
    case "running_blocked":
      return {
        title: "Running, but Windows blocked the last move",
        detail: "An app running as administrator is in front. Click another window and the next move will land.",
        tone: "warn",
      };
    case "running_power_only":
      return { title: "Running", detail: "Keeping the PC awake. Mouse moves are off in Settings.", tone: "on" };
    case "stopped_but_rule_holds":
      return { title: "Stopped", detail: "A rule in Advanced is still keeping the PC awake.", tone: "off" };
    default:
      return { title: "Stopped", detail: "Your PC can sleep and lock as normal.", tone: "off" };
  }
}

export default function Home() {
  const [s, setS] = useState<Status | null>(null);
  const [input, setInput] = useState<InputSettings | null>(null);
  const [testing, setTesting] = useState(false);

  const read = useCallback(() => {
    invoke<Status>("get_status").then(setS).catch(() => {});
  }, []);

  useEffect(() => {
    read();
    invoke<InputSettings>("get_input_settings").then(setInput).catch(() => {});
    // Once a second, as text (UI-UX §4). The event makes Start/Stop from the tray show at once.
    const t = window.setInterval(read, 1000);
    const un = listen("state:changed", read);
    return () => {
      window.clearInterval(t);
      un.then((f) => f());
    };
  }, [read]);

  const save = (next: InputSettings) =>
    invoke<InputSettings>("set_input_settings", { settings: next })
      .then((applied) => {
        setInput(applied); // Rust clamps; show what took effect
        read();
      })
      .catch(() => {});

  const test = () => {
    setTesting(true);
    invoke("test_move").catch(() => {});
    window.setTimeout(() => setTesting(false), 800); // one path at a time, not a queue of them
  };

  const running = s?.running ?? false;
  const d = s ? describe(s) : null;
  const key = input && input.key !== 0 ? KEYS.find(([k]) => k === input.key)?.[1] : null;

  return (
    <>
      <UpdateBanner />
      <div className={`status ${d?.tone ?? "off"}`} role="status" aria-live="polite">
        <div className="status-title">
          <span className="dot" aria-hidden="true" />
          {d?.title ?? " "}
        </div>
        <div className="status-detail">{d?.detail ?? " "}</div>
        {running && s && (
          <div className="status-detail">
            {s.keep_screen_on ? "PC won't sleep · screen stays on" : "PC won't sleep · the screen may turn off"}
          </div>
        )}
      </div>

      <button
        className={`btn big ${running ? "" : "primary"}`}
        onClick={() => invoke(running ? "stop" : "start").then(read).catch(() => {})}
      >
        {running ? "■  Stop" : "▶  Start"}
      </button>

      {input && (
        <div className="fields">
          <div className="field">
            <span>{key ? "Press the key after" : "Move the mouse after"}</span>
            <span className="inline">
              <NumberField
                label="Seconds with no input before a move"
                value={input.interval_secs}
                max={86_400}
                onCommit={(n) => save({ ...input, interval_secs: n })}
              />
              seconds with no input
            </span>
          </div>
          <div className="field">
            <span>Movement</span>
            <span className="inline">
              {key ? (
                <span className="note">{key} (change it in Settings)</span>
              ) : (
                <select
                  className="btn"
                  aria-label="Movement"
                  value={input.motion}
                  onChange={(e) => save({ ...input, motion: e.target.value as Motion })}
                >
                  {MOVEMENTS.map(([id, label]) => (
                    <option key={id} value={id}>
                      {label}
                    </option>
                  ))}
                </select>
              )}
              <button className="btn" onClick={test} disabled={testing}>
                Test
              </button>
            </span>
          </div>
        </div>
      )}

      <p className="note">
        When running, this moves your mouse a few pixels once your PC has had no input for that many
        seconds. Windows, the screen lock and apps that watch for idle time, such as Teams and Slack,
        see that as activity. It also keeps the PC from sleeping. Monitoring software can detect
        simulated input.
      </p>
      <p className="note">Closing this window keeps project-mouse running in the system tray, next to the clock.</p>
    </>
  );
}
