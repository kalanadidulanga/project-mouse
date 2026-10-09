# Feature Specification: M7 — Start/Stop (the app you can see)

**Feature Branch**: `005-start-stop` · **Created**: 2026-10-09 · **Status**: Draft

**Input**: Kalana, 2026-10-09: *"I can't understand anything in this app. The user can't
understand anything. Move Mouse is what I still use and everything in it is clear. Build mine
the same way, with no errors or bugs."*

Kalana uses Move Mouse to keep Teams/Slack from showing Away. Their sleep problem is solved
separately by a power-plan setting. Their Move Mouse `Settings.xml` holds exactly one action: a
visible **Square, 10 px, every 200 s**. No auto-pause, schedules, blackouts or auto-start.

**Decisions (brainstorming, 2026-10-09):** Start = move the mouse **and** keep awake, as one
button · the window opens at launch, X → tray · plain Windows style, no ring · approach A: Home +
Settings + Advanced, nothing deleted.

This reverses the 2026-08-27 "power-first, not a Move Mouse clone" direction **for the primary
surface**. The same change amends `docs/PRODUCT.md §0`, `docs/UI-UX.md §0.5`,
`docs/FEATURES.md` Part C/D1 and constitution principle I (v1.1.0).

## Root causes (bugs, not design)

1. **A normal launch shows no window.** The window opens only with `--show`
   (`src-tauri/src/lib.rs:659`), so the user gets a tray icon and nothing else. Windows often
   hides that icon in the overflow.
2. Because of 1, the first-run screen never shows.
3. Launching the exe again while it runs does nothing you can see. `apply_forwarded` only
   handles `--show`/`--keep`/`--release`.
4. The visible Square moves **one 10 px leg per interval** and needs four intervals to close.
   Each move is a single unanimated jump, so it looks like nothing happens.
5. Stand-down plus the 60 s idle threshold is invisible. Nothing tells the user why the cursor
   isn't moving.
6. Autostart passes `--minimized`, and nothing reads it.

## User Scenarios & Testing

### US1 — Open it and see it (P1)
Launch the app. A window appears showing **Stopped** and a **Start** button.
**Independent test:** run the exe with no arguments → a top-level window titled `project-mouse`
exists. Run it with `--minimized` → no window. Run the exe a second time while it is running →
the existing window opens or takes focus.

### US2 — Start it and watch it work (P1)
Press **Start**. The status changes to **Running**, with `Next move in m:ss` underneath. Once
the PC has had no input for the interval, the cursor traces a small square and returns to where
it started. Any real mouse or keyboard input resets the countdown. **Test** moves once,
immediately. **Stop** releases the power request and ends the moves.
**Independent test:** set the interval to 5 s, press Start, and don't touch anything. After
5 s, `GetCursorPos` reads the same before and after the path, and the visible trace shows in a
screen capture. Press Stop → diagnostics report nothing held by us.

### US3 — Close it and it keeps running (P1)
X destroys the window. The app keeps running in the tray. The tray icon is in colour while
running and grey while stopped. Left-click opens the window. The right-click menu offers
Start/Stop. Ctrl+Alt+K toggles Start/Stop.
**Independent test:** Start → X → the window is destroyed, the process is alive, the tray
tooltip reads `Running`, and moves continue. Tray click → the window comes back.

### US4 — Settings (P2)
Move the mouse · Keep the screen on · Distance · What to send · Vary · Start automatically when
the app opens · Start with Windows · Updates · About. Changes apply immediately, even while
running.

### US5 — Advanced keeps everything (P3)
Profiles, rules, the keep-awake-for timer, "Why is my PC awake?", the effect readout, idle
clocks, memory, the activity log and the Move Mouse importer all keep their current behaviour
on one scrolling page.

### Edge cases
- **Blocked (UIPI):** an elevated window has focus. Status shows the warning, and the engine
  makes **at most one attempt per interval** (no per-second retries).
- **User grabs the mouse mid-path:** the path still finishes its relative steps, so it adds
  zero net offset to wherever the user put the cursor.
- **Stopped, but a rule holds power:** status says *A rule in Advanced is still keeping the PC
  awake*.
- **Move the mouse off:** Running is power-only, and status says so. There is no countdown.
- **Clamps:** interval 5 s–1 h and distance 1–500 px, as today.
- **Corrupt config:** unchanged. Saving stays disabled and the app launches Stopped.
- **Quit while running:** the power request is released, as today.
- **Window destroyed mid-path:** the path runs in Rust, so destroying the window does not
  affect it.
- **Pointer acceleration** can scale each leg. The path closes by construction (its steps sum
  to zero), not by exact pixel arithmetic.

## Requirements

- **FR-001 Launch:** open the window unless `--minimized` is in argv. A second instance with no
  control flag (`--keep`/`--release`) opens or focuses the window.
- **FR-002 Close:** X destroys the window (unchanged). The app stays in the tray, and Home
  always says so in one line.
- **FR-003 One running state.** *Running* ⇔ manual mode ≠ Off. The input engine is enabled
  ⇔ running ∧ `move_mouse`. A single `core` function enforces this invariant after every
  manual-mode change. Start sets manual to `KeepPresenting` when `keep_screen_on` is on,
  `KeepRunning` otherwise. Stop sets it to `Off`. IPC, tray, hotkey and CLI all go through that
  function. Changing `move_mouse` or `keep_screen_on` while running re-applies it.
- **FR-004 Timing:** a move fires when system idle ≥ interval **and** the time since the last
  move ≥ interval. The second condition is the blocked-case guard. *Vary* draws the interval
  once per cycle, so the countdown is stable. `idle_threshold_secs` is removed.
- **FR-005 Smooth closed path:** each trigger traces the **whole** shape in small steps, at most
  ~10 ms apart and ≤ ~600 ms in total, and the steps sum to (0, 0). Invisible stays the +1/−1
  jiggle, and a key stays one keypress.
- **FR-006 The filter covers the whole path.** The self-injection filter and the C7 blocked
  check treat everything from the path's start to its end as ours. Otherwise later steps would
  register as human input.
- **FR-007 Test move:** `test_move` makes one dispatch right away, whether or not the app is
  running or idle, and goes through the same filter.
- **FR-008 Exposed state:** `next_move_in_secs` (none when stopped or when Move the mouse is
  off) and a status kind for Home. The tray tooltip reads
  `project-mouse — Running · next move in 0:42` or `project-mouse — Stopped`, pushed only when
  the text changes.
- **FR-009 Tray:** two icons, colour while running and grey while stopped. Menu: **Start|Stop**
  · Open · ─ · Start with Windows · Check for updates… · ─ · Quit. The Profile submenu shows
  only when there is more than one profile.
- **FR-010 Home:** layout per `docs/UI-UX.md §0.5`, with five status states. The honest
  paragraph sits next to Start and says, in plain words, what moves, why, and that
  *monitoring software can detect simulated input*.
- **FR-011 Settings:** the US4 list, plus `get_autostart`/`set_autostart` IPC so the UI checkbox
  and the tray check item agree.
- **FR-012 Advanced:** the existing panels move here unchanged. The exception is the Move Mouse
  importer: it now carries the cursor action's interval, direction and distance into Home and
  turns Move the mouse on. Left empty, it finds `Settings.xml` in either of Move Mouse's usual
  places. The Off/Keep running/Keep presenting buttons and the Pause switch are removed, since
  they duplicate Start/Stop. `pause_all`/`resume_all` and the engine's pause flag go too. Only
  that switch used them, and each rule keeps its own enable toggle.
- **FR-013 Remove first run:** delete the `FirstRun` component, `is_first_run`/
  `complete_first_run` and the `FIRST_RUN` flag.
- **FR-014 Config v3:** add `move_mouse` (default true), `keep_screen_on` (true) and
  `start_on_launch` (false). `mode` and `input_enabled` are no longer persisted; old files
  still load. Migration v2→v3: if `input_enabled` was false, reset `input` to the v3 defaults
  (interval 60 s, Square, 10 px, mouse, vary 0). The running state is not persisted. Only
  `start_on_launch` or `--keep` starts the app running at launch.
- **FR-015 Honest naming:** unchanged (constitution II). The CI honesty grep passes.
- **FR-016 Hotkey:** Ctrl+Alt+K toggles Start/Stop through FR-003.

## Success Criteria

- **SC-001:** A plain launch shows the window, `--minimized` shows none, and a second launch
  shows it. Each is asserted by enumerating top-level windows by title.
- **SC-002:** Running with a 5 s interval, the cursor traces the path and `GetCursorPos` is
  equal before and after.
- **SC-003:** Real input resets the countdown (unit test on the engine, plus a manual check).
- **SC-004:** Stop → `why_awake.ours == Off` within one tick.
- **SC-005:** X → the window is destroyed, the process is alive, and the tray icon and tooltip
  match the state. A tray click reopens the window.
- **SC-006:** Blocked: the engine makes at most one injection attempt per interval (unit test).
- **SC-007:** A v2 config with `input_enabled=false` migrates to the defaults with
  `move_mouse=true`. A v2 config with `input_enabled=true` keeps its settings (unit tests).
- **SC-008:** cargo test, clippy `-D warnings`, fmt, tsc, vite build and the honesty grep are
  all clean.
- **SC-009:** The tray-only working set stays within the 8 MB budget after the window closes.
- **SC-010 (Kalana):** with a 200 s interval, Teams stays Available through at least 10 minutes
  idle.

## Assumptions

- Teams and Slack derive presence from Windows' last-input time, so a relative `SendInput`
  move resets it. True today, not guaranteed, and the UI does not promise it.
- The grey tray icon is generated from `assets/icon.svg`.

## Out of scope

The radial launcher and countdown ring · Move Mouse's action list (click, scroll, keystroke
sequences, run command) · schedule/blackout UI changes · toast notifications · the rename
(PRODUCT §9).
