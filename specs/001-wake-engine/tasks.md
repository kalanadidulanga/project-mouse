# Tasks: M1 — The Wake Engine

**Feature**: `001-wake-engine` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Conventions: `[P]` = can run in parallel (different files, no ordering dep). Each user story is an
independently testable slice. TDD: within a story, the test task precedes its implementation task
and must fail first. `core/` and `config/` logic is tested against `MockPlatform` — **no Win32 in
the test suite** (constitution V).

---

## Phase 1 — Setup (project config)

- [ ] **T001** Rewrite `src-tauri/Cargo.toml` per plan: `tauri` v2 `default-features=false` features
  `["wry","common-controls-v6","tray-icon","image-ico"]`; `windows` 0.62 (Power, Threading,
  SystemServices, ProcessStatus, Registry, Foundation); `tracing`, `tracing-appender`,
  `tracing-subscriber`; `serde`/`serde_json`; `tauri-plugin-single-instance`,
  `tauri-plugin-autostart`; release profile (opt-level z, lto fat, panic abort, strip).
- [ ] **T002** Rewrite `src-tauri/tauri.conf.json` per `docs/TAURI-V2.md`: `windows:[{label:"main",
  create:false,...}]`, `withGlobalTauri:false`, `bundle.targets:["nsis"]`,
  `nsis.installMode:"currentUser"`, `webviewInstallMode:downloadBootstrapper`,
  `createUpdaterArtifacts` deferred to M5.
- [ ] **T003** `src-tauri/capabilities/main.json`: enumerate permissions (no `core:default`),
  `windows:["main"]`. `build.rs` restricts commands for `removeUnusedCommands`.
- [ ] **T004 [P]** `logging/mod.rs`: `tracing` subscriber → rolling file (1 MB × 3), `info` default;
  never log cursor coords at info (no coords exist yet — establish the rule).
- [ ] **T005 [P]** Install a panic hook in `main.rs` that releases power before abort (wired to the
  guard in Phase 3).

## Phase 2 — Foundational: platform boundary + MockPlatform (BLOCKS all stories)

- [ ] **T006** `platform/mod.rs`: define `PowerGuard` (`set_keep_awake(display,system)`, `release`),
  `AutoStart` (`is_enabled`, `set_enabled`), stub traits for later monitors, `Capabilities`, and the
  `Platform` bundle. No `#[cfg(windows)]` here.
- [ ] **T007** `platform/mock.rs`: `MockPlatform` recording power/autostart calls, for tests.
- [ ] **T008** `core/modes.rs`: `WakeMode { Off, KeepRunning, KeepPresenting }` with combine-by-max
  ordering (`KeepPresenting > KeepRunning > Off`).

## Phase 3 — User Story 1 (P1): keep awake from the tray  🎯 MVP

- [ ] **T009 [US1]** Test (`core/`): reconciler acquires the request once for a mode, is idempotent
  across repeated ticks, and clears on `Off` — against `MockPlatform`. MUST fail first.
- [ ] **T010 [US1]** Test (`core/`): two desired contributions combine by maximum; a weaker one
  never lowers a stronger held mode.
- [ ] **T011 [US1]** `power/mod.rs`: desired-vs-actual reconciliation, single owner, idempotent;
  computes the reason string naming the active mode. Make T009/T010 pass.
- [ ] **T012 [US1]** `platform/windows/power.rs`: `PowerGuard` impl — `PowerCreateRequest` +
  `PowerSetRequest(SystemRequired[+ExecutionRequired][+DisplayRequired])`, `PowerClearRequest` +
  `CloseHandle`; reason string via `REASON_CONTEXT`. (Ported/cleaned from the M0 spike.)
- [ ] **T013 [US1]** `core/state.rs` + `core/engine.rs`: `RwLock<AppState>` holding current mode;
  1 s tick reads desired mode and calls `power.reconcile(...)`. `timing/ticker.rs`:
  `CreateWaitableTimerExW` + 200 ms tolerable delay.
- [ ] **T014 [US1]** `main.rs`: tray icon (Active vs Off, greyscale-distinct) + native menu (Off /
  Keep running / Keep presenting / Quit) + tooltip stating what is true; run loop with
  `ExitRequested{code:None}→prevent_exit`; Quit → `app.exit(0)`; scheduler owns the guard; release
  on exit + panic hook (T005).
- [ ] **T015 [US1]** `ipc/mod.rs`: sync `get_state`, `set_mode` commands (thin over `core`).
- [ ] **CHECKPOINT US1**: verify SC-001, SC-002, SC-003, SC-004 by hand + elevated `powercfg`
  (reuse the M0 elevated helper).

## Phase 4 — User Story 2 (P2): portable, single-instance, autostart

- [ ] **T016 [US2]** Test (`config/`): portable-path detection — config beside exe when writable,
  else `%APPDATA%\project-mouse`. MUST fail first.
- [ ] **T017 [US2]** `config/store.rs`: implement portable-path detection (make T016 pass).
- [ ] **T018 [US2]** `tauri-plugin-single-instance` wired first in the builder; second launch
  signals the first (flash tray) and exits.
- [ ] **T019 [US2]** `platform/windows/autostart.rs`: `HKCU\...\Run` via `tauri-plugin-autostart`
  (or Registry), `--minimized`; idempotent enable/disable; tray menu toggle + persisted setting.
- [ ] **CHECKPOINT US2**: SC-006 with Process Monitor (zero registry writes until autostart on;
  runs from USB; no admin).

## Phase 5 — User Story 3 (P3): trustworthy config & logs

- [ ] **T020 [US3]** Test (`config/`): atomic write leaves only old-or-new complete file when killed
  mid-write; round-trip serialize/deserialize. MUST fail first.
- [ ] **T021 [US3]** Test (`config/`): a corrupt `config.json` surfaces an error and keeps the file
  — never resets to defaults; migration chain upgrades an older `schema_version`.
- [ ] **T022 [US3]** `config/model.rs` + `store.rs` + `migrate.rs`: `schema_version`, atomic
  write (temp+fsync+`MoveFileEx` replace) debounced 500 ms, corrupt-file handling, migration chain.
  Make T020/T021 pass.
- [ ] **T023 [US3]** Persist mode + settings through config; load on startup; wire debounced saves.
- [ ] **CHECKPOINT US3**: SC-005 (kill mid-write; corrupt-file) verified.

## Phase 6 — Polish & exit criteria

- [ ] **T024 [P]** `power/inspect`-free budget probe: measure **true private working set** (VMMap or
  `QueryWorkingSetEx`) + apply `EmptyWorkingSet` after any teardown; record vs SC-007 (M0 caveat).
- [ ] **T025 [P]** `quickstart.md`: build/run/verify M1, including the manual S0 test (SC-009).
- [ ] **T026** Full exit-criteria pass (SC-001…SC-008) + `cargo test` green (SC-008: all engine
  tests on `MockPlatform`, no Win32).

---

## Dependencies

- Phase 1 → Phase 2 → (Phase 3 = MVP) → Phase 4 → Phase 5 → Phase 6.
- Within a story, test task → impl task (TDD). `[P]` tasks touch different files and may overlap.
- US1 alone is a demoable MVP (keeps a machine awake, auditable, releases cleanly). US2/US3 harden it.

## Parallel opportunities

- T004, T005 in Phase 1.
- After T006/T007/T008, the US1 tests (T009/T010) and the Windows `PowerGuard` (T012) can proceed
  in parallel — tests use `MockPlatform`, T012 is the real impl.
- T024, T025 in Phase 6.
