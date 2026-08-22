# Tasks: M2 — Conditions

**Feature**: `002-conditions` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

`[P]` = parallelizable. TDD: test task before impl. `core/` logic tested against a hand-built
`Snapshot` / `MockPlatform` — **no Win32 in tests** (constitution V).

---

## Phase 1 — Rule engine core (pure, no OS)  🎯 the product

- [ ] **T001** `core/snapshot.rs`: `Snapshot { now, idle, foreground_exe, session_locked,
  remote_session, notification_state, running_processes, on_ac, battery_pct, cpu_pct }` — plain data
  a tick fills and conditions read. `Default` for tests.
- [ ] **T002** `core/rule.rs`: `Condition` enum (ProcessRunning, TimeWindow, Expiry, OnACPower,
  BatteryAbove, SessionUnlocked, UserNotificationState, ForegroundAppIn/NotIn, Not, AnyOf, AllOf),
  `Rule { id, name, enabled, conditions, mode }`, `Profile { id, name, rules }`. serde.
- [ ] **T003 [P]** Test `core/evaluator.rs`: each condition against a constructed `Snapshot`
  (process name case-insensitive; time window incl. crossing midnight; expiry vs now; battery/AC;
  session; notification state; foreground in/not-in; Not/AnyOf/AllOf). MUST fail first.
- [ ] **T004** `core/evaluator.rs`: `Condition::eval(&Snapshot) -> bool` + `desired_mode(profile,
  &Snapshot) -> WakeMode` (combine-by-max over matching enabled rules). Make T003 pass.
- [ ] **T005 [P]** Test: combine-by-max across rules (SC-007) — weaker never lowers stronger.

## Phase 2 — The tick

- [ ] **T006** `timing/ticker.rs`: `CreateWaitableTimerExW` + `SetWaitableTimerEx` (1000 ms, 200 ms
  tolerable delay); a loop calling a closure each tick. On non-Windows, a `std::thread::sleep`
  fallback so it compiles.
- [ ] **T007** `core/engine.rs` (extend): hold profiles + active id; `tick(&Snapshot)` computes
  `desired_mode` and reconciles; scheduler thread samples cheap state each tick and process list
  every ~5 s. Reason string names the contributing rule(s).
- [ ] **T008** `lib.rs`: spawn the scheduler thread (owns the reconciler); stop it cleanly on exit
  (still release power on every path).

## Phase 3 — Platform monitors (behind traits)

- [ ] **T009** `platform/mod.rs` + `mock.rs`: add `ProcessMonitor`, `SessionMonitor`,
  `ForegroundMonitor` (+ presentation/notification state), `PowerSource`, `SystemLoad` traits + fakes.
- [ ] **T010 [P]** `platform/windows/process.rs`: `CreateToolhelp32Snapshot` process names.
- [ ] **T011 [P]** `platform/windows/foreground.rs`: `GetForegroundWindow` +
  `QueryFullProcessImageNameW` (`PROCESS_QUERY_LIMITED_INFORMATION`) + `SHQueryUserNotificationState`.
- [ ] **T012 [P]** `platform/windows/session.rs`: `SM_REMOTESESSION` + `WM_WTSSESSION_CHANGE`
  (lock/unlock) via `WTSRegisterSessionNotification`.
- [ ] **T013 [P]** `platform/windows/load.rs`: `GetSystemPowerStatus` (AC/battery) +
  `GetSystemTimes` (CPU delta).
- [ ] **T014** Wire the monitors into the scheduler's per-tick / cadence-limited sampling.

## Phase 4 — Persistence, profiles, hotkey

- [ ] **T015** Test: config **v1→v2** migration — a bare-`mode` v1 file becomes one default profile
  holding that mode. MUST fail first.
- [ ] **T016** `config/model.rs` + `migrate.rs`: schema_version 2, profiles + active id; migration.
  Persist rules atomically.
- [ ] **T017** Tray: profile submenu (switch active), and rules surfaced read-only for now (full rule
  builder UI is M3). `SwitchProfile` action.
- [ ] **T018** Global hotkey (D3) via `tauri-plugin-global-shortcut` — toggle wake with no window.

## Phase 5 — Time correctness & exit criteria

- [ ] **T019** Handle resume (`PBT_APMRESUMEAUTOMATIC`) + `WM_TIMECHANGE`: re-evaluate immediately;
  recompute schedules against local time. Idle-time clamp/wrap guard (WINDOWS-API gotcha 1).
- [ ] **T020** Expiry remaining-time in the tray tooltip (SC-003).
- [ ] **T021** Exit-criteria pass SC-001..SC-010; `cargo test` green (all engine tests on
  `Snapshot`/`MockPlatform`); idle CPU ≤ 0.05 % with rules active.

---

## Dependencies

- Phase 1 (pure engine) → Phase 2 (tick) → Phase 3 (real monitors) → Phase 4 (persist/UI/hotkey) →
  Phase 5 (time + verify). Phase 1 is the flagship and is fully testable with zero OS code.
- Within a story, test → impl (TDD). `[P]` = different files.

## Parallel opportunities

- T003 and T005 (evaluator tests) alongside T002.
- T010–T013 (the four Windows monitors) are independent files.
