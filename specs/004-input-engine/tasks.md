# Tasks: M4 — The Input Engine

**Feature**: `004-input-engine` · Spec: [spec.md](./spec.md) · Design: `docs/FEATURES.md Part C`,
`docs/ARCHITECTURE.md §6`, `docs/WINDOWS-API.md`.

## Phase 1 — The self-injection filter (pure, tested)  🎯 the P0 core
- [x] **T001** `core/idle.rs`: `IdleTracker` with `system_idle` + `human_idle`; `note_injection(tick)`
  and `observe(system_last_input_tick, now)` attributing a reset within tolerance to us. Pure.
- [x] **T002** Tests: human_idle keeps rising while injections happen; drops on a real input; wrap/
  clamp for the 49-day / non-monotonic case. MUST fail first.

## Phase 2 — Injector (behind the trait)
- [x] **T003** `platform` `InputInjector` trait (`virtual_jiggle`, `key`) + `MockInjector`.
- [x] **T004** `platform/windows/input.rs`: `SendInput` — virtual jiggle (relative move netting zero),
  `VK_F15` key down+up in one call, magic `dwExtraInfo` (gotchas 4/8).

## Phase 3 — Wire it in, off by default
- [x] **T005** Global `input_enabled` switch (config, default false); gate every dispatch at one place.
- [x] **T006** Scheduler phase 2: when enabled AND `human_idle` ≥ threshold, jiggle each interval;
  record `note_injection`; stand-down (C6) cancels/suppresses when human returns.
- [x] **T007** Blocked detection (C7): after a jiggle, if `system_idle` did not reset → Blocked state.
- [x] **T008** Surface: `human_idle` + `system_idle` + Blocked in `get_diagnostics`/UI; tray tooltip
  gains the Blocked state.

## Phase 4 — Honesty gate & verify
- [x] **T009** Enabling synthesis shows one honest paragraph; default key `VK_F15`, configurable.
- [~] **T010** CI grep: no "undetectable/human-like/natural" in strings/docs (PRODUCT §5 Test 3).
- [~] **T011** Exit criteria SC-001..SC-006; `cargo test` green (filter tested against a fake clock).

Dependencies: P1 (pure filter) → P2 (injector) → P3 (wire) → P4 (honesty/verify). P1 is the P0 core
and is fully testable with zero OS code.
