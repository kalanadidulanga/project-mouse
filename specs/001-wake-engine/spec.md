# Feature Specification: M1 — The Wake Engine

**Feature Branch**: `001-wake-engine`

**Created**: 2026-08-22

**Status**: Draft

**Input**: ROADMAP.md M1. The product's actual purpose — power inhibition, no rules, no UI, no
input synthesis. Implements FEATURES A1, A3, A5, D1, D4, D5, D6, D8. The `docs/` are the
authoritative spec; this file scopes M1 and restates only the acceptance criteria.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Keep a machine awake for a task, from the tray (Priority: P1)

A developer starts an overnight build. They left-or-right-click the tray icon and pick **Keep
running**. The machine no longer sleeps while the build runs; the screen is still free to dim,
blank, and lock. A presenter instead picks **Keep presenting**, and the display also stays lit
and unlocked. Either is turned **Off** the same way. No window is ever opened.

**Why this priority**: This is the entire purpose of the milestone and the product. Without it
there is nothing. It must also work on Modern Standby (S0) machines, where every competitor
fails.

**Independent Test**: Launch the app (tray only, no window). Switch between Off / Keep running /
Keep presenting from the native tray menu. Verify with `powercfg /requests` that the correct
requests are held with our reason string, and that display/sleep/lock behave as the mode table
says.

**Acceptance Scenarios**:

1. **Given** the app is running and Off, **When** the user selects **Keep running**, **Then**
   system sleep is blocked (`PowerRequestSystemRequired` + `PowerRequestExecutionRequired`) while
   the display may still sleep and the session may lock.
2. **Given** Keep running is active, **When** the user selects **Keep presenting**, **Then** the
   display and screensaver/lock are also inhibited (`+ PowerRequestDisplayRequired`).
3. **Given** any mode is active, **When** the user selects **Off**, **Then** all power requests
   are cleared and the machine returns to its normal power behavior with nothing left in
   `powercfg /requests` attributable to us.
4. **Given** Keep running is active on a Modern Standby (S0) laptop, **When** the lid is closed
   and the display turns off, **Then** the machine stays awake and a running job continues
   (manual release-checklist test; cannot be automated in CI).

### User Story 2 — Runs from a USB stick, no installer, no admin (Priority: P2)

An engineer on a locked-down corporate machine copies a single `.exe` to a USB stick, runs it,
and it starts silently to the tray. It writes no registry keys unless they explicitly enable
"start with Windows," needs no admin, and leaves nothing behind on exit.

**Why this priority**: The users with the worst version of this problem are on the most
locked-down machines; portable + no-admin is what makes the tool usable by them at all.

**Independent Test**: Run the single exe from a removable drive with Process Monitor watching.
Confirm: starts minimised to tray, zero registry writes (until autostart is toggled on), config
written beside the exe, no admin prompt.

**Acceptance Scenarios**:

1. **Given** a single exe on a USB stick, **When** it is launched, **Then** it starts to the
   tray with no window and requires no administrator rights.
2. **Given** portable mode, **When** the app writes config, **Then** it writes `config.json`
   beside the exe and makes **zero** registry writes.
3. **Given** the app is running, **When** the user enables "start with Windows," **Then** a
   single `HKCU\...\Run` entry is created (no elevation), and disabling it removes the entry.
4. **Given** a second instance is launched, **When** it detects the first, **Then** it signals
   the running instance and exits (single-instance).

### User Story 3 — Trustworthy config and logs (Priority: P3)

The user's profiles and settings survive a power cut mid-save, a corrupted config never silently
wipes their setup, and a rolling log exists for diagnostics without leaking anything sensitive.

**Why this priority**: Correctness and trust plumbing that everything later depends on; cheap now,
miserable to retrofit.

**Independent Test**: Kill the process during the save debounce window and confirm config is
intact on next start. Corrupt `config.json` by hand and confirm the app surfaces an error and
keeps the file rather than resetting.

**Acceptance Scenarios**:

1. **Given** a config change, **When** the process is force-killed during the debounce/write,
   **Then** on next launch the config is either the old or the new complete version, never a
   half-written file.
2. **Given** a corrupt `config.json`, **When** the app starts, **Then** it surfaces the error and
   keeps the broken file — it never silently falls back to defaults.
3. **Given** the app is running, **When** events occur (mode changes, errors), **Then** they are
   written to a rolling log file, and cursor coordinates are never logged at `info`.

### Edge Cases

- **Crash / forced kill while holding a request** — the request must not outlive the process. Clear
  it in the exit handler **and** a panic hook; verified clean in `powercfg /requests` after
  `taskkill /F` (M0 confirmed Windows also auto-releases handle-scoped requests, but we do not rely
  on that alone).
- **Quit from the tray** — must actually quit (not be trapped by the stay-alive `prevent_exit`
  handler, which only fires on last-window-closed).
- **Two rules/sources want different modes** — the stronger mode wins (Keep presenting > Keep
  running > Off); no source can weaken another. (Groundwork; full rule composition is M2.)
- **`config.json` directory not writable** (read-only USB) — surface an error, keep running with
  in-memory state, do not crash.
- **Autostart entry already present / stale** — toggling is idempotent.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (A1)**: System MUST expose three mutually exclusive modes — Off, Keep running, Keep
  presenting — as two independently-persisted states, switchable from the native tray menu with no
  window.
- **FR-002 (A1)**: Keep running MUST hold `PowerRequestSystemRequired` + `PowerRequestExecutionRequired`;
  Keep presenting MUST additionally hold `PowerRequestDisplayRequired`.
- **FR-003 (A2)**: System MUST use the `PowerCreateRequest`/`PowerSetRequest` path (never
  `SetThreadExecutionState`), so Modern Standby is actually prevented.
- **FR-004 (A3)**: Each power request MUST carry a human-readable `REASON_CONTEXT` string that
  names the mode, so it appears in `powercfg /requests` attributed to us.
- **FR-005 (A3)**: System MUST release all power requests on every exit path AND in a panic hook;
  a leaked request is a defect.
- **FR-006 (A3)**: The power request MUST be owned by a single long-lived owner (the scheduler
  thread), reconciled idempotently against desired state — acquiring the same mode repeatedly is a
  no-op.
- **FR-007 (A5)**: Screensaver suppression in Keep presenting MUST come from
  `PowerRequestDisplayRequired` only; the app MUST NOT mutate `SPI_SETSCREENSAVEACTIVE` or any
  persistent system setting.
- **FR-008 (D1)**: The tray icon MUST show visually distinct states (at minimum Active vs Off)
  distinguishable at 16×16 in greyscale, with a tooltip stating what is currently true of the
  machine and why.
- **FR-009 (D4)**: System MUST support optional auto-start via a single `HKCU\...\Run` entry (no
  elevation), always starting minimised to the tray; toggling it is idempotent.
- **FR-010 (D5)**: System MUST run as a single portable exe with no installer and no admin, writing
  config beside the exe in portable mode and making zero registry writes until autostart is enabled.
- **FR-011 (D5)**: System MUST enforce a single instance; a second launch signals the first and exits.
- **FR-012 (D6)**: System MUST log to a rolling file via `tracing`, and MUST NOT log cursor
  coordinates at `info`.
- **FR-013 (D8)**: Config MUST carry a `schema_version` from v1 with a migration chain, be written
  atomically (temp + fsync + replace) and debounced, and MUST surface an error + keep the file on a
  parse failure — never silently reset.
- **FR-014 (arch)**: All OS calls MUST live behind the `platform/` traits; `core/` MUST contain no
  `#[cfg(windows)]`. A `MockPlatform` MUST exist so the engine is testable without Win32.
- **FR-015**: The app MUST stay alive with zero windows (tray-only), and the tray Quit item MUST
  actually exit.

### Key Entities

- **WakeMode**: `Off` | `KeepRunning` | `KeepPresenting`. Combined by maximum, not last-writer.
- **PowerRequest (held state)**: a handle + the set of request types currently set + the reason
  string; acquired, reconciled, released. Not an event.
- **Config**: `schema_version` + persisted mode state + settings (autostart on/off, log level);
  serialized as atomic JSON.
- **Platform**: the trait bundle (`PowerGuard`, `AutoStart`, plus stubs for later monitors) the
  engine is allowed to know about the OS through, with a `Capabilities` descriptor.

## Success Criteria *(mandatory)*

Measurable, from ROADMAP M1 exit criteria.

- **SC-001**: Tray icon with three modes, switchable from the native menu, with no window in
  existence.
- **SC-002**: Keep running blocks system sleep while allowing display sleep and lock; Keep
  presenting blocks sleep, display-off, and screensaver — each verified by observed behavior.
- **SC-003**: `powercfg /requests` shows our reason string with the active mode named.
- **SC-004**: Quitting via the tray leaves `powercfg /requests` clean; `taskkill /F` also leaves it
  clean.
- **SC-005**: Config survives a forced power-off mid-write (kill during the debounce window);
  a corrupt config surfaces an error and keeps the file — it never silently resets.
- **SC-006**: Portable: single exe on a USB stick, config beside it, zero registry writes (verified
  with Process Monitor), runs without admin.
- **SC-007**: Idle for 10 minutes: CPU ≤ 0.05 %, private working set ≤ 8 MB (measured as private
  working set + `EmptyWorkingSet` trim, per the M0 caveat).
- **SC-008**: The rule/power engine passes a unit-test suite run entirely against `MockPlatform`,
  with no Win32 calls in the tests.
- **SC-009 (manual, release checklist)**: On an S0 laptop, lid closed, Keep running active, the
  machine is still reachable after 8 h and a running job completed.

## Assumptions

- Windows 10 1809+ / 11 is the only target for M1; macOS/Linux are stubs behind the platform traits.
- No rules, conditions, UI window, or input synthesis in M1 — those are M2/M3/M4. Mode is set
  directly from the tray and persisted.
- "Portable mode" is detected by the presence of a writable config beside the exe (else
  `%APPDATA%\project-mouse`); exact detection rule finalised in planning.
- `project-mouse` is a working name; the binary/identifier rename is a separate pre-M5 decision.
- The M0 spike validated the Tauri stack and the power-request path; M1 builds the real,
  non-throwaway implementation and must additionally measure true private working set + apply
  `EmptyWorkingSet`.
