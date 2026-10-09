// Mirrors of the Rust IPC types: ipc/mod.rs (Status), core/running.rs (RunSettings, StatusKind),
// core/input_engine.rs (InputSettings), core/motion.rs (Motion).
export type StatusKind =
  | "stopped"
  | "stopped_but_rule_holds"
  | "running"
  | "running_blocked"
  | "running_power_only";

export type Status = {
  kind: StatusKind;
  running: boolean;
  next_move_in_secs: number | null;
  keep_screen_on: boolean;
};

export type RunSettings = { move_mouse: boolean; keep_screen_on: boolean; start_on_launch: boolean };

export type Motion = "Virtual" | "Line" | "Square" | "Circle";

export type InputSettings = {
  interval_secs: number;
  key: number;
  motion: Motion;
  distance_px: number;
  vary_pct: number;
};

/** Virtual-key codes worth offering. 0 means "move the mouse". F15 is the category's convention
 *  (Caffeine), and it is also the one that breaks in PuTTY, PowerPoint and Google Docs, which is
 *  why the choice is the user's. */
export const KEYS: [number, string][] = [
  [0, "Mouse movement"],
  [0x7e, "F15 key press"],
  [0x91, "Scroll Lock key press"],
  [0x10, "Shift key press"],
];
