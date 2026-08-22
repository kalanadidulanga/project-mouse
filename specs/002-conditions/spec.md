# Feature Specification: M2 — Conditions (the rule engine)

**Feature Branch**: `002-conditions`

**Created**: 2026-08-22

**Status**: Draft

**Input**: ROADMAP.md M2. The reason to build this rather than use PowerToys Awake — bind the wake
lock to real conditions so it holds *only when needed*. Implements FEATURES B1, B2, B3, B5, B6, B7,
B10, B12, D2, D3. Authoritative design: `docs/ARCHITECTURE.md §5` (rule engine + tick),
`docs/FEATURES.md Part B`, `docs/WINDOWS-API.md`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Keep awake while a process runs (Priority: P1) 🎯 flagship

A developer sets a rule: *Keep running while `msbuild.exe` is running*. The machine stays awake for
the whole build and releases within a few seconds of it finishing — even if the build tool exits
and respawns under a new PID.

**Why this priority**: The single most-requested unserved primitive (PowerToys #27980, still open).
One rule covers builds, renders, training runs, transfers, backups, AI agents — most of PRODUCT §3.

**Independent Test**: Create a process-bound rule, start/stop the named process, watch
`powercfg /requests` acquire within one tick and release within ~5 s of exit. Kill+respawn the
process under a new PID → the lock is held continuously.

**Acceptance Scenarios**:

1. **Given** a rule "Keep running while `X.exe` runs" and `X.exe` running, **When** a tick
   evaluates, **Then** the power request is held.
2. **Given** that rule holding, **When** `X.exe` exits, **Then** the request is released within ~5 s.
3. **Given** the process exits and respawns under a new PID, **When** ticks continue, **Then** the
   request is held continuously (match by **name**, not PID).

### User Story 2 — Bind to time: schedule and expiry (Priority: P1)

A user keeps a wallboard awake weekdays 08:00–18:00, and separately says "keep awake for 2 hours"
before stepping away. Both release automatically.

**Why this priority**: The most-requested unshipped schedule feature (PowerToys #42720); expiry is
PowerToys #46646. Both are core "conditional, not always-on".

**Independent Test**: A `TimeWindow` rule holds only inside the window and survives a timezone
change/resume; an expiry rule releases at the stated time with remaining time shown in the tooltip.

**Acceptance Scenarios**:

1. **Given** a weekday 08:00–18:00 rule, **When** the clock is inside the window on a weekday,
   **Then** the mode is held; outside it, released.
2. **Given** an expiry "for 2 hours", **When** 2 h elapse, **Then** the lock releases; the tooltip
   shows remaining time while it is active.
3. **Given** a schedule, **When** the machine sleeps/resumes or the timezone changes
   (`WM_TIMECHANGE`), **Then** the window is recomputed against local time (no wrong-hour firing).

### User Story 3 — Guards: power, session, presentation (Priority: P2)

Rules respect reality: pause below 20% battery, stop when the workstation locks (unless explicitly
allowed), and — for presentations — keep the display on while never being surprising.

**Why this priority**: Prevents the tool from flattening a laptop, and `SHQueryUserNotificationState`
gives fullscreen/game/presentation/locked detection in one cheap call (four features).

**Independent Test**: Battery below threshold releases; back on AC re-acquires. Locking the session
releases a rule that requires unlocked. A fullscreen/presentation state is detected and acts as
configured.

**Acceptance Scenarios**:

1. **Given** a rule with "on AC / battery > 20%", **When** battery drops below 20% on DC, **Then**
   its contribution drops; back on AC it returns.
2. **Given** a rule requiring an unlocked session, **When** the workstation locks, **Then** the rule
   stops contributing; some rules (e.g. an overnight build) may opt to continue when locked.
3. **Given** presentation/fullscreen/locked, **When** a tick samples `SHQueryUserNotificationState`,
   **Then** the corresponding condition evaluates correctly (one call, not window-rect heuristics).

### User Story 4 — Composition, profiles, hotkey (Priority: P2)

A power user writes *weekdays 09:00–18:00, while `msbuild.exe` runs, on AC, unless presenting* as
one rule; groups rules into a **Long build** profile; and toggles everything with a global hotkey.

**Why this priority**: Composition (`AllOf`/`AnyOf`/`Not`) makes the model general instead of
special-cased; profiles + hotkey are the interaction primitives (D2/D3, both unshipped elsewhere).

**Independent Test**: A composed rule evaluates as the boolean combination of its parts against a
snapshot. Switching profiles changes the active rule set. A hotkey toggles without any window.

**Acceptance Scenarios**:

1. **Given** a rule with several `AND` conditions and a `Not(presenting)`, **When** evaluated,
   **Then** it holds iff all conditions are true and the negated one is false.
2. **Given** two profiles, **When** the active profile is switched (tray/hotkey/rule), **Then** only
   the active profile's enabled rules contribute.
3. **Given** a registered global hotkey, **When** pressed, **Then** the wake state toggles with no
   window created.

### Edge Cases

- **Combine by maximum** — if several rules contribute different modes, the strongest wins; no rule
  can weaken another (carried from M1).
- **Process enumeration cost** — sampled at a slower cadence (~5 s) than the 1 s tick; cheap state
  (idle/session/foreground/notification) sampled every tick.
- **49-day `GetTickCount` wrap / non-monotonic `dwTime`** — idle-based conditions must clamp
  (WINDOWS-API gotcha 1). (Idle-as-guard is M2-lite; full idle gating is M4.)
- **Resume from sleep** — re-arm and re-evaluate immediately; a schedule must not fire at the wrong
  hour after hours asleep.
- **Rule referencing an unavailable capability** — fail loudly / disable, never silently never-fire.
- **DST / timezone** — store local time + IANA zone; recompute on resume and `WM_TIMECHANGE`.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (B12)**: A `Rule` MUST be `{ enabled, conditions: Vec<Condition>, mode: WakeMode }`;
  conditions are ANDed; a rule contributes its `mode` iff enabled and all conditions hold.
- **FR-002 (§5 tick)**: A 1 s waitable-timer tick MUST sample state, evaluate all enabled rules of
  the active profile, compute `desired = max(contributions)`, and reconcile the power engine
  idempotently. Cheap state every tick; process list/CPU/battery every ~5 s.
- **FR-003 (B1)**: `ProcessRunning(name)` MUST match by executable name (case-insensitive),
  surviving PID changes; support multiple names (any alive).
- **FR-004 (B2)**: `TimeWindow { days, from, to, tz }` MUST evaluate against local time, handle
  windows crossing midnight, and recompute on resume / `WM_TIMECHANGE`.
- **FR-005 (B3)**: An expiry MUST release at a set instant/duration; remaining time MUST be visible
  in the tray tooltip.
- **FR-006 (B5)**: `OnACPower` / `BatteryAbove(pct)` MUST gate rules; default guidance: pause below
  20% on battery.
- **FR-007 (B6)**: `SessionUnlocked` (and remote/console) MUST be evaluable via session
  notifications + `SM_REMOTESESSION`; a rule may opt to continue when locked.
- **FR-008 (B7)**: `UserNotificationState` MUST come from a single `SHQueryUserNotificationState`
  call (presentation / fullscreen / game / locked-or-screensaver / quiet-time); NOT window-rect
  heuristics.
- **FR-009 (B10)**: `ForegroundAppIn(list)` / `ForegroundAppNotIn(list)` MUST use
  `QueryFullProcessImageNameW` with `PROCESS_QUERY_LIMITED_INFORMATION`.
- **FR-010 (B12)**: `Not` / `AnyOf` / `AllOf` MUST compose conditions with no special-case code.
- **FR-011 (D2)**: A profile is a named set of rules; exactly one active at a time; switchable from
  the tray, a hotkey, or a rule (`SwitchProfile`).
- **FR-012 (D3)**: A global hotkey MUST toggle the wake state without any window
  (`RegisterHotKey` / `tauri-plugin-global-shortcut`).
- **FR-013 (arch)**: All new OS access (process, session, foreground, notification, battery, CPU)
  MUST be behind `platform/` traits with `MockPlatform` fakes; `core/` stays `#[cfg(windows)]`-free.
- **FR-014**: Rules/profiles MUST persist via the M1 config (schema migration v1→v2), atomically,
  never silently reset.
- **FR-015 (idle CPU)**: The tick MUST use `CreateWaitableTimerExW` with a tolerable delay so idle
  CPU stays ≤ 0.05 %; process enumeration cadence-limited.

### Key Entities

- **Rule**: `{ id, name, enabled, conditions: Vec<Condition>, mode: WakeMode }` (M2 = power only;
  triggers/actions for the input engine arrive in M4).
- **Condition**: enum — `ProcessRunning`, `TimeWindow`, `Expiry`, `OnACPower`, `BatteryAbove`,
  `SessionUnlocked`, `UserNotificationState`, `ForegroundAppIn/NotIn`, `Not`, `AnyOf`, `AllOf`.
- **Snapshot**: the sampled state a tick evaluates against (idle, foreground, session, notification
  state, now; plus cadence-limited process list, CPU, battery). Enables Win32-free unit tests.
- **Profile**: `{ id, name, rules: Vec<Rule> }`; config holds profiles + active id.
- **Platform monitors**: `ProcessMonitor`, `SessionMonitor`, `ForegroundMonitor`
  (+ presentation state), `PowerSource` (battery/AC), `SystemLoad` (CPU) traits.

## Success Criteria *(mandatory)*

From ROADMAP M2 exit criteria.

- **SC-001**: `--while-process msbuild.exe` (or a UI rule) holds while it runs, releases within 5 s
  of exit.
- **SC-002**: Process matching survives a **PID change** (exit + respawn) — lock held continuously.
- **SC-003**: Expiry releases at the stated time; remaining time visible in the tooltip.
- **SC-004**: A schedule survives sleep/resume and a timezone change (`WM_TIMECHANGE`).
- **SC-005**: Battery below 20% releases; back on AC re-acquires.
- **SC-006**: Fullscreen game, presentation mode, and a locked screen are each detected via
  `SHQueryUserNotificationState` and act as configured.
- **SC-007**: Two rules wanting different modes → the machine holds the **stronger** one; a rule can
  never weaken another.
- **SC-008**: The rule engine is fully unit-tested against `MockPlatform`/`Snapshot` — **no Win32 in
  the test suite**.
- **SC-009**: A global hotkey toggles without any window existing.
- **SC-010**: Idle 10 min with rules active: CPU ≤ 0.05 %, working set within the M1 budget.

## Assumptions

- Windows only for M2; other OSes remain compiling stubs behind the traits.
- **Power only** — no input synthesis (M4). Conditions gate the *held* `WakeMode`; triggers/actions
  for events come in M4.
- Config migrates v1→v2 to add profiles/rules; a v1 file (bare `mode`) becomes a single default
  profile.
- The 1 s tick deferred from M1 is introduced here (M1 had no conditions to evaluate).
- Idle-as-a-guard is minimal in M2; full idle-based gating + stand-down is M4.
