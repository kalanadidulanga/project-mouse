// Home (spec 006 FR-024): what is true right now, one button, Run for, and a summary of the
// movement with a way to change it.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  clock, hhmm, keyLabel, motionLabel,
  type InputSettings, type PauseReason, type RunFor, type RunSettings, type Status,
} from "./types";
import { TimeField } from "./controls";
import { UpdateBanner } from "./updates";

const RUN_FOR: [string, string][] = [
  ["forever", "Until I stop"],
  ["30", "30 minutes"],
  ["60", "1 hour"],
  ["120", "2 hours"],
  ["240", "4 hours"],
  ["until", "Until a time"],
];

function toRunFor(choice: string, until: number): RunFor {
  if (choice === "forever") return { kind: "forever" };
  if (choice === "until") return { kind: "until", at: until };
  return { kind: "minutes", minutes: Number(choice) };
}

function pauseText(p: PauseReason): string {
  switch (p.reason) {
    case "battery":
      return "On battery. It carries on when you plug in.";
    case "locked":
      return "The screen is locked. It carries on when you unlock.";
    case "presenting":
      return "You are presenting, or a full-screen app is open.";
    case "blackout":
      return `Blackout until ${hhmm(p.until)}.`;
  }
}

type Tone = "on" | "off" | "warn" | "pause";

function describe(s: Status): { title: string; detail: string; tone: Tone } {
  switch (s.kind) {
    case "running":
      return { title: "Running", detail: s.next_move_in_secs == null ? "Starting." : `Next move in ${clock(s.next_move_in_secs)}`, tone: "on" };
    case "running_blocked":
      return {
        title: "Running, but Windows blocked the last move",
        detail: "An app running as administrator is in front. Click another window and the next move will land.",
        tone: "warn",
      };
    case "running_power_only":
      return { title: "Running", detail: "Keeping the PC awake. Mouse moves are off (Behaviour).", tone: "on" };
    case "paused":
      return { title: "Paused", detail: s.pause ? pauseText(s.pause) : "", tone: "pause" };
    case "stopped_but_rule_holds":
      return {
        title: "Stopped",
        detail: s.holding_app
          ? `But ${s.holding_app} is running, so the PC stays awake (Behaviour).`
          : "But a rule is keeping the PC awake (About, Troubleshooting).",
        tone: "off",
      };
    default:
      return { title: "Stopped", detail: "Your PC can sleep and lock as normal.", tone: "off" };
  }
}

/** The one honest explanation, matched to what Start will actually do (constitution I/II). */
function explanation(input: InputSettings, run: RunSettings): string {
  if (!run.move_mouse) {
    return "When running, this keeps the PC awake. It sends no input, so the screen can still lock and Teams or Slack can still show you as away.";
  }
  const what =
    input.key !== 0
      ? `presses ${keyLabel(input.key).replace(" key press", "")} once`
      : input.motion === "Virtual"
        ? "nudges the mouse invisibly"
        : "moves your mouse a few pixels and back";
  return `When running, this ${what} once your PC has had no input for a while. Windows, the screen lock and apps that watch for idle time, such as Teams and Slack, see that as activity. It also keeps the PC from sleeping. Monitoring software can detect simulated input.`;
}

function summary(input: InputSettings, run: RunSettings): string {
  if (!run.move_mouse) return "Mouse moves are off";
  const what = input.key !== 0 ? keyLabel(input.key) : motionLabel(input.motion);
  const wait = input.interval_random ? `${input.interval_secs} to ${input.interval_max_secs} s` : `${input.interval_secs} s`;
  return `${what} after ${wait} with no input`;
}

export default function Home({ go }: { go: (page: "movement" | "behaviour") => void }) {
  const [s, setS] = useState<Status | null>(null);
  const [input, setInput] = useState<InputSettings | null>(null);
  const [run, setRun] = useState<RunSettings | null>(null);
  const [choice, setChoice] = useState("forever");
  const [until, setUntil] = useState(18 * 60);
  const [testing, setTesting] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const read = useCallback(() => {
    invoke<Status>("get_status").then(setS).catch(() => {});
  }, []);
  const readSettings = useCallback(() => {
    invoke<InputSettings>("get_input_settings").then(setInput).catch(() => {});
    invoke<RunSettings>("get_run_settings").then(setRun).catch(() => {});
  }, []);

  useEffect(() => {
    read();
    readSettings();
    // Once a second, as text (UI-UX §4). The event makes changes from the tray show at once.
    const t = window.setInterval(read, 1000);
    const un = listen("state:changed", () => {
      read();
      readSettings();
    });
    return () => {
      window.clearInterval(t);
      un.then((f) => f());
    };
  }, [read, readSettings]);

  const running = s?.running ?? false;
  const d = s ? describe(s) : null;

  const startStop = () =>
    invoke(running ? "stop" : "start", running ? {} : { runFor: toRunFor(choice, until) })
      .then(read)
      .catch(() => {});

  const changeRunFor = (c: string, u: number) => {
    setChoice(c);
    setUntil(u);
    if (running) invoke("set_run_for", { runFor: toRunFor(c, u) }).then(read).catch(() => {});
  };

  const test = () => {
    setTesting(true);
    invoke("test_move").catch(() => {});
    if (input && input.key === 0 && input.motion === "Virtual") {
      setNote("Sent an invisible move: the cursor stays put, but Windows saw input.");
      window.setTimeout(() => setNote(null), 2500);
    }
    window.setTimeout(() => setTesting(false), 800); // one path at a time, not a queue of them
  };

  const stopsAt = s?.stops_at ? new Date(s.stops_at * 1000) : null;

  return (
    <>
      <UpdateBanner />
      <div className={`status ${d?.tone ?? "off"}`}>
        <div className="status-title" role="status" aria-live="polite">
          <span className="dot" aria-hidden="true" />
          {d?.title ?? " "}
        </div>
        <div className="status-detail" role="timer">
          {d?.detail ?? " "}
        </div>
        {running && s && (
          <div className="status-detail">
            {s.keep_screen_on ? "PC won't sleep · screen stays on" : "PC won't sleep · the screen may turn off"}
            {stopsAt && ` · Stops at ${hhmm(stopsAt.getHours() * 60 + stopsAt.getMinutes())}`}
          </div>
        )}
      </div>

      <button className={`btn big ${running ? "" : "primary"}`} onClick={startStop}>
        {running ? (
          <><span aria-hidden="true">■</span>  Stop</>
        ) : (
          <><span aria-hidden="true">▶</span>  Start</>
        )}
      </button>

      <div className="fields">
        <div className="field">
          <span>Run for</span>
          <span className="inline">
            <select className="btn" aria-label="Run for" value={choice} onChange={(e) => changeRunFor(e.target.value, until)}>
              {RUN_FOR.map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))}
            </select>
            {choice === "until" && <TimeField label="Stop at" minutes={until} onChange={(m) => changeRunFor("until", m)} />}
          </span>
        </div>
        {input && run && (
          <div className="field">
            <span>Movement</span>
            <span className="inline">
              <span className="summary">{summary(input, run)}</span>
              <button className="btn" onClick={() => go(run.move_mouse ? "movement" : "behaviour")}>
                Edit
              </button>
              <button className="btn" onClick={test} disabled={testing || !run.move_mouse}>
                Test
              </button>
            </span>
          </div>
        )}
        {note && (
          <p className="note" role="status">
            {note}
          </p>
        )}
      </div>

      {input && run && <p className="note">{explanation(input, run)}</p>}
      <p className="note">
        {s && <>Idle for {clock(s.idle_secs)} · </>}Closing this window keeps project-mouse running in the tray, next to the clock.
      </p>
    </>
  );
}
