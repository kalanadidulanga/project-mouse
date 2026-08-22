# Tasks: M3 — The UI

**Feature**: `003-settings-ui` · Spec: [spec.md](./spec.md) · Detailed design: `docs/UI-UX.md`.

## Phase 1 — Window lifecycle + IPC plumbing
- [x] **T001** `ipc/mod.rs`: sync commands `get_state`, `set_mode`, `pause_all`/`resume_all`,
  `get_diagnostics`, `get_logs`; serializable view types (React holds a projection only).
- [x] **T002** lib.rs: left-click tray → create `main` window from the `create:false` config;
  `CloseRequested` → `destroy()`. Register commands; emit `state:changed` only if a window exists.
- [x] **T003** Ring-buffer log sink (in-memory, 500) alongside the file, exposed via `get_logs`.

## Phase 2 — Diagnostics (mostly Rust)
- [~] **T004** `power/inspect.rs`: `CallNtPowerInformation(PowerRequestInfo)` → other processes'
  power requests; degrade gracefully unelevated (E1).
- [~] **T005** Diagnostics view: effect readout (sleep/display/lock blocked-or-allowed + why),
  memory (working set), system-idle clock.

## Phase 3 — React UI (invoke frontend-design first)
- [x] **T006** App shell: left rail (Status/Rules/Activity/Settings), Windows-native visual language,
  light/dark + system accent, motion budget, a11y.
- [x] **T007** Status page: one sentence + pause switch; profile; today's counts; memory.
- [~] **T008** "Why is my PC awake?" panel (E1) + effect readout (E3).
- [x] **T009** Activity/log view (reads the ring buffer).

## Phase 4 — Rule builder + persistence + first run
- [x] **T010** Config **v1→v2**: profiles + rules + active id; migration; atomic persist.
- [x] **T011** Rule builder — plain-language sentences, inline dropdowns, disabled-by-default;
  `upsert_rule`/`delete_rule`/`set_rule_enabled`; round-trips to JSON.
- [~] **T012** First-run: one question, three profiles, no input synthesis, self-destructs.
- [~] **T013** Exit criteria SC-001..SC-007; assert destroy-not-hide + working-set return.

Dependencies: P1 → P2 → P3 → P4. P1/P2 are Rust; P3 needs frontend-design.
