# Implementation Plan: M2 — Conditions

**Branch**: `002-conditions` | **Date**: 2026-08-22 | **Spec**: [spec.md](./spec.md)

**Input**: [spec.md](./spec.md); design in `docs/ARCHITECTURE.md §5`, `docs/FEATURES.md Part B`,
`docs/WINDOWS-API.md`; principles in `.specify/memory/constitution.md`.

## Summary

Add the rule engine and the 1 s evaluation tick on top of M1's power core. A `Snapshot` of sampled
state is evaluated against each enabled rule's `conditions`; matching rules contribute a `WakeMode`;
the maximum is reconciled by the existing idempotent power reconciler. Conditions cover process,
schedule, expiry, power source, session, notification-state, and foreground app, with `Not`/`AnyOf`/
`AllOf` composition. Profiles group rules; a global hotkey toggles. All new OS access sits behind
`platform/` traits so the whole engine is unit-tested against a fake `Snapshot`.

## Technical Context

**Language/Version**: Rust 2021 (as M1).

**Primary Dependencies**: M1 set + new `windows` features — `Win32_System_Diagnostics_ToolHelp`
(process enum), `Win32_UI_Shell` (`SHQueryUserNotificationState`), `Win32_UI_WindowsAndMessaging`
(`GetForegroundWindow`), `Win32_System_RemoteDesktop` (`SM_REMOTESESSION` + session notify),
`Win32_System_SystemInformation`/existing Power (battery, `GetSystemTimes`). `tauri-plugin-global-shortcut` for D3.

**Storage**: config v1→v2 migration — add `profiles` + `active_profile`; a v1 bare-`mode` file
becomes one default profile. Atomic write (M1).

**Testing**: `cargo test` — condition evaluation, combine-by-max, schedule/expiry/timezone math,
process-name matching, and the tick's desired-mode computation are all pure logic tested against a
constructed `Snapshot` and `MockPlatform`. No Win32 in tests.

**Target Platform / Project Type / Perf / Constraints**: as M1. New: process enumeration is
cadence-limited (~5 s); the tick uses `CreateWaitableTimerExW` + tolerable delay to hold idle CPU
≤ 0.05 %.

## Constitution Check

*GATE: passes.*

- **I. Mechanisms separate** — still power-only; conditions gate the *held* mode; no events/input. ✅
- **III. Non-destructive/auditable** — reason string updated to name the contributing rule(s). ✅
- **IV. Platform boundary** — every new OS call (process/session/foreground/notification/battery/CPU)
  is a new `platform/` trait with a `Mock`; `core/` stays cfg-free. ✅
- **V. Test-first** — condition evaluator, schedule/expiry math, PID-change matching, combine-by-max,
  and config v2 migration are written test-first against `Snapshot`/`MockPlatform`. ✅
- **VI. Config safety** — v1→v2 migration in the existing chain; atomic; no silent reset. ✅

No violations → Complexity Tracking empty.

## Project Structure

New/changed under `src-tauri/src/` (M1 files unchanged unless noted):

```text
core/
├── rule.rs         # Rule, Condition (enum + composition), Profile
├── snapshot.rs     # Snapshot of sampled state (the thing conditions read)
├── evaluator.rs    # Condition::eval(&Snapshot) -> bool; desired_mode(profile,&snapshot)
├── engine.rs       # (extended) hold rules/profiles; tick() computes desired -> reconcile
└── modes.rs        # (unchanged) combine-by-max already here

timing/
└── ticker.rs       # CreateWaitableTimerExW + SetWaitableTimerEx (tolerable delay), sample cadence

platform/
├── mod.rs          # + ProcessMonitor, SessionMonitor, ForegroundMonitor, PowerSource, SystemLoad
├── mock.rs         # + fakes for all of the above (feed a Snapshot in tests)
└── windows/
    ├── process.rs      # CreateToolhelp32Snapshot
    ├── session.rs      # WTSRegisterSessionNotification + SM_REMOTESESSION
    ├── foreground.rs   # GetForegroundWindow + QueryFullProcessImageNameW + SHQueryUserNotificationState
    └── load.rs         # GetSystemPowerStatus (battery/AC) + GetSystemTimes (CPU)

config/
├── model.rs        # + Profile/Rule/Condition serde; schema_version = 2
└── migrate.rs      # + v1 -> v2 (bare mode -> one default profile)

main.rs / lib.rs    # start the ticker thread; register global hotkey; profile switching in tray
```

**Structure Decision**: the tick loop lives on a dedicated `std::thread` (the scheduler) that owns
the `PowerReconciler` (as ARCHITECTURE §2 intends); it samples cheap state each second and the
process list every ~5 s, evaluates the active profile, and reconciles. `core/` remains Win32-free and
fully testable by feeding a hand-built `Snapshot`.

## Complexity Tracking

No constitution violations. (Section intentionally empty.)
