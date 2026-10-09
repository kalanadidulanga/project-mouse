# Feature Specification: M8, Move Mouse parity

**Branch**: `005-start-stop` (continues M7) · **Created**: 2026-10-09 · **Status**: Draft

**Input**: Kalana, 2026-10-09, while testing M7:
- *"puluwan tharam features add karanna"* ("add as many features as you can")
- screenshots of every Move Mouse tab: Actions, Behaviour, Appearance, Schedules, Blackouts and About
- the Direction, Speed, Trigger and Repeat lists
- *"hodata kalpana karala add karanna one ewa add karanna"* ("think it through and add what should be added")

Kalana selected all 14 candidate features, then approved both design sections in brainstorming.

M7 (`specs/005-start-stop/spec.md`) stays the base. Start/Stop is still one button, and everything here either refines what Start does or decides when it runs.

## User Scenarios & Testing

### US1: Move the mouse the way Move Mouse does (P1)
Pick from Move Mouse's full direction list:
- Small square, Small circle
- Left and right, Right and left, Up and down, Down and up
- the eight compass directions (out and back)
- Random
- Invisible

The distance can be fixed or random between a min and a max. The speed can be Slow, Normal, Fast or Custom. With *Abort if I move* on, a move stops the instant the user touches the mouse.
**Independent test:** with a 5 s interval and each direction in turn, `GetCursorPos` reads the same before and after the move. With abort on, moving the mouse during a path leaves the cursor where the user put it.

### US2: Timing (P1)
Move every N seconds, or randomly between a min and a max. **Run for** stops Start by itself, after 30 min / 1 h / 2 h / 4 h or at a chosen time. Home shows *Stops at 18:00*.
**Independent test:** Run for 1 minute → the app returns to Stopped within 2 s of the deadline, and a notification says so.

### US3: Automatic pauses (P1)
- **Pause on battery:** off by default. It pauses moves *and* keep-awake, so the PC can sleep and save the battery, and resumes on AC.
- **Pause when locked:** on by default, matching Move Mouse. It pauses moves only, and keep-awake continues.
- **Blackouts:** for example Mon to Fri, 12:30 to 13:30. They pause moves only, so the PC stays awake and can resume afterwards.

While paused, Home reads *Paused: on battery* / *screen locked* / *blackout until 13:30*.
**Independent test:** Win+L → the moves stop and the power request stays. Unlock → the moves resume.

### US4: Schedules (P2)
A list of "these days, at this time, Start (or Stop)". Each entry fires at its moment. If the user presses Start or Stop by hand in between, that choice stands until the next scheduled time.
**Independent test:** a Start entry one minute ahead starts the app and shows a notification.

### US5: Window and tray extras (P2)
- **Always on top:** off by default.
- **Taskbar dot:** a green dot while running, a yellow dot while paused, and none while stopped. It shows only while the window is open.
- **Notifications:** on by default.
- **Live idle time:** *Idle for m:ss* on Home and in About.

### Edge cases
- **A schedule time passes while the PC sleeps.** On wake, the latest edge crossed since the previous tick fires, within the same local day. Edges from a previous day never fire.
- **Two edges in one crossed interval.** The later time wins. For the same time, Stop wins.
- **Manual override.** It stands until the next edge. Starting when already running, or stopping when already stopped, does nothing.
- **A blackout that crosses midnight** (22:00 to 06:00): its days refer to the start day. `from == to` means an empty window, and the UI does not allow saving one.
- **Run for "until 09:00" set at 10:00** means 09:00 tomorrow.
- **Clearing the deadline.** Stop (manual, scheduled or hotkey) clears a Run-for deadline. A scheduled Start never sets one.
- **Pause priority:** battery, then locked, then blackout. Status names only the first.
- **Battery pause while running.** The power request is released and the manual mode is kept. On AC, the request is re-held and moves resume. No notification is sent for pauses.
- **Swapped ranges.** min > max is corrected by swapping the two. The clamps still apply: interval 5 to 3600 s, distance 1 to 500 px, custom speed 1 to 50 ms per step.
- **Abort.**
  - The cursor counts as moved by the user when it is more than 2 px from where the path put it.
  - An aborted path still counts as a move: the countdown restarts, and the user's input resets the idle time anyway.
  - Invisible has no path, so abort does not apply to it.
- **Windows suppresses notifications** (Focus Assist, or notifications turned off). Nothing else is affected.
- **A clock change or DST jumping backwards** may re-cross an edge. That is harmless, because a repeated Start or Stop does nothing.

## Requirements

- **FR-001 Directions**, grouped in the dropdown:
  - Shapes: Small square, Small circle
  - Back and forth: Left and right, Right and left, Up and down, Down and up
  - One direction: N, NE, E, SE, S, SW, W, NW
  - Other: Random, Invisible
- **FR-002 Every path is closed.** A one-direction move goes out and comes back. Random picks one of the eight compass directions per trigger, seeded from the tick. Invisible stays the +1/-1 jiggle.
- **FR-003 Distance** is fixed, or random in [min, max] and drawn per trigger. The clamp is 1 to 500 px.
- **FR-004 Speed** sets the milliseconds between path steps: Slow 20, Normal 10, Fast 4, Custom 1 to 50. Paths keep the 40-step cap.
- **FR-005 Abort if I move** is on by default. After each step the platform compares the cursor with where the path put it. More than 2 px off → stop at once and report the move as aborted.
- **FR-006 Interval** is fixed, or random in [min, max]. It is drawn once per cycle, so the countdown is steady. The clamp is 5 to 3600 s. This replaces *Vary by %*.
- **FR-007 Run for:**
  - Choices: until I stop / 30 min / 1 h / 2 h / 4 h / until HH:MM.
  - At the deadline it stops automatically.
  - The deadline is kept in memory only, and Stop clears it.
  - Home shows *Stops at HH:MM*.
- **FR-008 Pause on battery** is off by default. It pauses moves and keep-awake.
- **FR-009 Pause when locked** is on by default. It pauses moves only.
- **FR-010 Blackouts** are a list of `{days[7], from, to, enabled}`. They pause moves only and may cross midnight.
- **FR-011 Schedules** are a list of `{days[7], time, action: Start|Stop, enabled}`, with the edge semantics above.
- **FR-012 A pause is an overlay, not a state change.** The running state (manual mode ≠ Off) does not change. `core::running` enforces both invariants:
  - input on ⇔ running ∧ move_mouse ∧ ¬paused
  - power held ⇔ running ∧ ¬battery-paused
- **FR-013 Decisions live in a pure `core::autopilot`.** Given the settings, the deadline, the previous tick's local time and the snapshot, it returns an optional Start/Stop command (with its cause) and an optional pause reason. No OS code.
- **FR-014 Status** gains:
  - kind `paused`, with `reason`: `battery`, `locked` or `blackout` (and `until` for blackouts)
  - `stops_at` (epoch seconds or null)
  - `idle_secs`
- **FR-015 Always on top** is off by default. It applies when the window opens and when the setting changes.
- **FR-016 Taskbar dot.** `set_overlay_icon` shows green while running, yellow while paused, and nothing while stopped. Both dots are generated at runtime, so no assets are needed.
- **FR-017 Notifications** are on by default, via `tauri-plugin-notification` from Rust only. Each of these sends one:
  - a scheduled Start
  - a scheduled Stop
  - a Run-for stop
  - the first blocked move of a run
- **FR-018 Live idle time** appears on Home as *Idle for m:ss* and in About.
- **FR-019 Config v4**, with a v3→v4 migration:
  - `vary_pct` is dropped, and the ranges default to fixed.
  - New run settings get their defaults.
  - `schedules` and `blackouts` start empty.
  - The Advanced timer rule (`id: "timer"`) is removed from every profile.
- **FR-020 Remove the Advanced timer.** *Keep awake for a while* is removed, because Run for replaces it.
- **FR-021 Make Invisible explain itself.**
  - The label reads *Invisible (resets idle time, the cursor stays put)*.
  - **Test** with Invisible shows *Sent an invisible move* for about 2 s.
- **FR-022 Honest naming.** Random features are described as keeping moves from lining up with other timers, never as human-like (constitution II).
- **FR-023 Not building:**
  - Move Mouse's Trigger (Start/Interval/Stop) and Repeat (Forever/Throttle): they exist for its multi-action list. Ours is one movement, and Run for covers "stop after a while".
  - The action list itself, with click, scroll, keystroke, command and script.
  - Cron "advanced schedules".
  - Volume, hiding, disguise and screen-burn options (PRODUCT §5, constitution III).

### Layout: tabs, not a long Settings page (Kalana, 2026-10-09)

Kalana tested the M7 UI. Their verdict: Settings is one long scroll; Advanced is incomprehensible; Move Mouse is clear because every concern has its own tab. They asked for more tabs and a UX anyone can use, and chose to split up Advanced and simplify it.

- **FR-024 Seven tabs.** The rail has seven tabs, each with a small inline-SVG icon and a label:

  | Tab | Contents |
  |---|---|
  | **Home** | Status line, Start/Stop, **Run for**, a one-line summary (*Small square after 60 s · Edit* → Movement), **Test**, *Idle for m:ss* |
  | **Movement** | What to send (mouse or key), direction, distance, speed, abort if I move, interval |
  | **Behaviour** | Move the mouse, keep the screen on, the pauses (battery, locked, presenting), keep awake while these apps run, start when the app opens, start with Windows, the hotkey |
  | **Schedules** | The list editor |
  | **Blackouts** | The list editor |
  | **Appearance** | Always on top, taskbar dot, notifications |
  | **About** | Version and updates; idle time; Import from Move Mouse; **Troubleshooting**, collapsed by default |

  The window stays 760×540, and the active tab is marked by more than colour.
- **FR-025 Short rows, detail on request.** Each setting has a short label and at most one short hint line. The longer explanation sits behind a **?** button, as in Move Mouse. The button toggles an inline note, carries `aria-expanded`, and works from the keyboard. The paragraph-length notes in M7 move behind these buttons.
- **FR-026 Keep awake while these apps run.** This replaces the rule builder with a plain list of executable names. Add one by typing it or by picking from the running apps (a new `list_running_apps` IPC from the sampler). The list is stored as one rule in the active profile: id `apps`, `ProcessRunning(list)`, KeepRunning, enabled while non-empty. It holds power even while Stopped. Home then reads *Stopped, but msbuild.exe is keeping the PC awake*, naming the first running match.
- **FR-027 Pause while presenting** is off by default. It covers presentation mode, a full-screen app and a full-screen game, using the `NotifState` values `Presentation`, `Busy` and `Game`. It pauses moves only. Pause priority becomes battery, then locked, then presenting, then blackout.
- **FR-028 The profiles UI is removed.** The engine keeps the active profile. The tray's Profile submenu still appears only when more than one profile exists.
- **FR-029 About ▸ Troubleshooting**, rewritten in plain words:
  - *Why is my PC awake?*
  - *What Windows is being asked for* (sleep, display, lock and moves, each allowed or blocked)
  - the two idle clocks and memory
  - recent activity
  - *Rules from an earlier version*: any rule other than `apps`, each with on/off and delete, so that nothing keeps the PC awake invisibly. The section is hidden when there are none.
- **FR-030 About ▸ Import from Move Mouse.** One button that finds `Settings.xml` by itself, with an optional path field. The report shows below it.
- **FR-031 Config v4 folds old rules into the list.** Any enabled rule whose only condition is `ProcessRunning` merges into the `apps` rule. Every other rule is kept as it is (constitution VI), and FR-029 lists it.

## Success Criteria

- **SC-001:** All 16 directions close their path, as a unit test. In the real app, `GetCursorPos` is unchanged before and after a move for Square, NE and Random.
- **SC-002:** Random interval and distance draws stay within [min, max] and vary (unit test).
- **SC-003:** Abort stops the path at the first step where the mock cursor deviates (unit test), and does so in the real app.
- **SC-004:** The autopilot unit tests cover crossing an edge, a sleep gap, midnight, manual override, and two edges in one interval.
- **SC-005:** The pause unit tests cover:
  - battery releases power
  - locked and blackout keep power
  - the priority order
  - a blackout across midnight
- **SC-006:** The v3→v4 migration unit tests pass, including timer-rule removal.
- **SC-007:** These are all clean:
  - cargo test, clippy `-D warnings`, fmt
  - tsc, vite build
  - the honesty grep, the boundary grep
  - no em dash in any tracked file outside vendored spec-kit
- **SC-008 (Kalana, real app):**
  - a schedule one minute ahead starts the app with a notification
  - Win+L pauses moves and unlock resumes them
  - unplugging pauses on battery
  - the taskbar dot changes colour
- **SC-009:** With the notification plugin, the tray-only working set stays within the 8 MB budget.
- **SC-010:** Every setting is reachable within two clicks of Home. No settings page needs more than a little scrolling at 760×540. Every **?** button opens and closes with the keyboard.
- **SC-011:** The v3→v4 migration turns a process-only rule into the `apps` list and keeps every other rule (unit test).

## Assumptions

- `tauri-plugin-notification` delivers toasts for this unpackaged NSIS app on Windows 10 and 11, keyed on the app identifier.
- `set_overlay_icon` is Windows-only, and this app is Windows-only.
- Schedules and blackouts use local time (`platform::local_time`), and the evaluator already samples it into `Snapshot`.

## Out of scope

Everything in FR-023, multi-monitor targeting, and the rename (PRODUCT §9).
