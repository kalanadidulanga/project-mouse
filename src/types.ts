// Mirrors of the Rust IPC types: ipc/mod.rs (Status), core/running.rs (RunSettings, StatusKind),
// core/input_engine.rs (InputSettings), core/motion.rs (Motion, Speed), core/autopilot.rs
// (PauseReason, RunFor, Schedule, Blackout, Timetable), config/model.rs (Appearance).
export type StatusKind =
  | "stopped"
  | "stopped_but_rule_holds"
  | "running"
  | "running_blocked"
  | "running_power_only"
  | "paused";

export type PauseReason =
  | { reason: "battery" }
  | { reason: "locked" }
  | { reason: "presenting" }
  | { reason: "blackout"; until: number };

export type Status = {
  kind: StatusKind;
  running: boolean;
  next_move_in_secs: number | null;
  keep_screen_on: boolean;
  pause: PauseReason | null;
  stops_at: number | null;
  idle_secs: number;
  holding_app: string | null;
};

export type RunSettings = {
  move_mouse: boolean;
  keep_screen_on: boolean;
  start_on_launch: boolean;
  pause_on_battery: boolean;
  pause_when_locked: boolean;
  pause_when_presenting: boolean;
};

export type Motion =
  | "Virtual" | "Square" | "Circle"
  | "RightAndLeft" | "LeftAndRight" | "UpAndDown" | "DownAndUp"
  | "North" | "NorthEast" | "East" | "SouthEast" | "South" | "SouthWest" | "West" | "NorthWest"
  | "Random";

export type Speed = "Slow" | "Normal" | "Fast" | "Custom";

export type InputSettings = {
  interval_secs: number;
  interval_random: boolean;
  interval_max_secs: number;
  key: number;
  motion: Motion;
  distance_px: number;
  distance_random: boolean;
  distance_max_px: number;
  speed: Speed;
  custom_step_ms: number;
  abortable: boolean;
};

export type RunFor = { kind: "forever" } | { kind: "minutes"; minutes: number } | { kind: "until"; at: number };

export type Schedule = { days: boolean[]; at: number; action: "Start" | "Stop"; enabled: boolean };
export type Blackout = { days: boolean[]; from: number; to: number; enabled: boolean };
export type Timetable = { schedules: Schedule[]; blackouts: Blackout[] };
export type Appearance = { always_on_top: boolean; taskbar_dot: boolean; notifications: boolean };

/** Virtual-key codes worth offering. 0 means "move the mouse". F15 is the category's convention
 *  (Caffeine), and it is also the one that breaks in PuTTY, PowerPoint and Google Docs, which is
 *  why the choice is the user's. */
export const KEYS: [number, string][] = [
  [0, "Mouse movement"],
  [0x7e, "F15 key press"],
  [0x91, "Scroll Lock key press"],
  [0x10, "Shift key press"],
];

/** Move Mouse's full Direction list, grouped (spec 006 FR-001). */
export const DIRECTIONS: { group: string; items: [Motion, string][] }[] = [
  { group: "Shapes", items: [["Square", "Small square"], ["Circle", "Small circle"]] },
  {
    group: "Back and forth",
    items: [
      ["RightAndLeft", "Right and left"],
      ["LeftAndRight", "Left and right"],
      ["UpAndDown", "Up and down"],
      ["DownAndUp", "Down and up"],
    ],
  },
  {
    group: "One direction, out and back",
    items: [
      ["North", "North (up)"],
      ["NorthEast", "North-east"],
      ["East", "East (right)"],
      ["SouthEast", "South-east"],
      ["South", "South (down)"],
      ["SouthWest", "South-west"],
      ["West", "West (left)"],
      ["NorthWest", "North-west"],
    ],
  },
  {
    group: "Other",
    items: [
      ["Random", "Random direction"],
      ["Virtual", "Invisible (resets idle time, the cursor stays put)"],
    ],
  },
];

export const motionLabel = (m: Motion) =>
  DIRECTIONS.flatMap((g) => g.items).find(([id]) => id === m)?.[1] ?? m;
export const keyLabel = (k: number) => KEYS.find(([c]) => c === k)?.[1] ?? "a key press";

export const DAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
export const WEEKDAYS = [true, true, true, true, true, false, false];
export const EVERY_DAY = [true, true, true, true, true, true, true];

export const hhmm = (m: number) =>
  `${String(Math.floor(m / 60) % 24).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
export const toMinutes = (v: string) => {
  const [h, m] = v.split(":").map(Number);
  return ((h || 0) * 60 + (m || 0)) % 1440;
};
export const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
