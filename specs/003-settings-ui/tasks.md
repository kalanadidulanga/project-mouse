# Tasks: M3 — The UI

**Feature**: `003-settings-ui` · **Spec**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)
**Design**: [research.md](./research.md) · [data-model.md](./data-model.md) ·
[contracts/ipc.md](./contracts/ipc.md) · [quickstart.md](./quickstart.md) ·
detailed UI design: `docs/UI-UX.md`

Regenerated 2026-08-28. The previous list marked five tasks `[~]` and never covered spec FR-002
(`set_profile`) or FR-004 at all; this one closes that gap and absorbs the three `002-conditions`
tasks that live on the same surface.

**Status 2026-08-28**: 19 of 21 open tasks closed. T032 and T034 are `[~]` — their automatable
halves pass, their manual halves need someone looking at a window. Nothing is ticked that was not
run.

**Test policy**: constitution V is non-negotiable — engine logic is written **test-first** against
`MockPlatform`, no Win32 in the test suite. Win32 FFI wrappers cannot be unit-tested and are
named as manually verified in [quickstart.md](./quickstart.md).

---

## Phase 1 — Setup

- [X] T001 React + TS + Vite frontend scaffolded alongside the Rust core (done in M1)
- [X] T002 `tauri.conf.json` declares the `main` window with `create: false`

## Phase 2 — Foundational (blocking prerequisites)

**⚠️ T003 blocks every profile task. It is a live data-loss bug, not cleanup.**

- [X] T003 Fix `persist_current` in `src-tauri/src/lib.rs`: it writes `profiles: vec![profile]`,
      flattening the whole collection to the single profile the engine holds. Merge the engine's
      profile into the loaded collection instead. Hold the collection in a
      `Mutex<Vec<Profile>>` managed alongside `Persist`. (research R3)
- [X] T004 [P] Write the failing test first, then implement: `src-tauri/src/core/profiles.rs` —
      pure collection ops (upsert-by-id, delete-refusing-the-last, find-active, rename). No Win32,
      no `AppHandle`. (constitution V)
- [X] T005 Sync-only `#[tauri::command]` surface; React holds a projection, never state
- [X] T006 Ring-buffer log sink (in-memory, 500) exposed via `get_logs`

## Phase 3 — US1: Open settings, see status, act (P1)

**Independent test**: open → interactive ≤ 400 ms; close → working set returns to the M1
baseline within 30 s; the window is destroyed, never hidden.

- [X] T007 Left-click tray → create the `main` window; `CloseRequested` → `destroy()`
- [X] T008 App shell: left rail (Status/Rules/Activity/Settings), Windows visual language,
      light/dark, motion budget
- [X] T009 Status page: one sentence + pause switch; profile; memory; idle clocks
- [X] T010 Activity/log view reading the ring buffer

## Phase 4 — US2: "Why is my PC awake?" + live effect (P1, flagship)

**Independent test**: the panel states our own hold exactly, and states Windows' aggregate
verbatim on a separate line, without merging them. Cross-check the aggregate against elevated
`powercfg /requests`. (The "and it isn't us" subtraction was cut — see T014.)

**Design constraint from [research R1](./research.md): `GetPowerRequestList` refuses an
unelevated caller outright. Build on `SystemExecutionState`.**

- [X] T011 Write the failing test first, then implement `src-tauri/src/core/awake.rs` —
      `report(aggregate, ours) -> AwakeReport` from [data-model.md](./data-model.md). Cover:
      nothing held · each bit · unreadable · and the regression guard that our own mode never
      alters the reported aggregate. (constitution V)
- [X] T012 [P] `PowerInspector` trait in `src-tauri/src/platform/mod.rs` returning the machine
      aggregate as `Option<u32>` (`None` = Windows refused), with a `MockPlatform` impl in
      `src-tauri/src/platform/mock.rs`
- [X] T013 `src-tauri/src/platform/windows/inspect.rs` — `CallNtPowerInformation`
      `SystemExecutionState` (level 16). Returns `None` on any non-`STATUS_SUCCESS`; never panics.
- [X] T014 **MANUAL verification gate** — run, result recorded in
      [research.md](./research.md#t014-result--the-is-it-us-subtraction-is-cut). **Inconclusive**:
      the test machine's aggregate was saturated at `0x03` so no addition of ours was observable,
      and the one free channel (away mode) is not reported there at all. Subtraction **cut** — the
      asymmetric failure mode says never suppress a real third-party request on an unverified
      assumption.
- [X] T015 `why_awake` command in `src-tauri/src/ipc/mod.rs` per
      [contracts/ipc.md](./contracts/ipc.md). Never returns `Err`; a refused read is
      `readable: false`.
- [X] T016 "Why is my PC awake?" panel on the Status page in `src/App.tsx`: plain-language
      verdict, our own contribution named, and `powercfg /requests` as **copyable text** with the
      note that it needs an elevated prompt. Never runs the command.
- [X] T017 Effect readout (E3): sleep/display/lock blocked-or-allowed + why

## Phase 5 — US3: Rule builder (P2)

**Independent test**: build a rule in the UI → verify the JSON → reload → unchanged.

- [X] T018 Config **v1→v2**: profiles + rules + active id; migration chain; atomic persist
- [X] T019 Rule builder — plain-language sentences, every `Condition` variant reachable,
      disabled-by-default, `upsert_rule`/`delete_rule`/`set_rule_enabled` (`16c5ec1`)
- [X] T020 Timer on Status as an `ExpiryAt` rule that releases itself (`16c5ec1`)
- [X] T021 Input-engine settings (interval, idle threshold, what to send) persisted in `Config`,
      clamped in Rust, form shows what took effect (`16c5ec1`)

## Phase 6 — US4: First run (P2)

**Independent test**: with no config file, the window opens on one question; any answer creates a
working profile in under 10 s; Settings → Synthesize input is still Off and Activity shows no
injection.

- [X] T022 Latch "no config file existed at startup" at load time in `src-tauri/src/lib.rs`;
      expose `is_first_run` (contracts/ipc.md)
- [X] T023 `complete_first_run(choice)` — the three profiles from
      [contracts/ipc.md](./contracts/ipc.md#first-run-choices--profiles). Never touches
      `input_enabled`. (SC-007)
- [X] T024 `src/firstrun.tsx` — one question, three answers, then straight into the normal UI

## Phase 7 — Absorbed from `002-conditions`

Same surface, so they close here rather than in a milestone that is otherwise done.

- [X] T025 `list_profiles` / `set_profile` / `create_profile` / `delete_profile` commands
      (002 T017, spec FR-002). Depends on T003 + T004.
- [X] T026 Tray profile submenu in `src-tauri/src/lib.rs`, checkmarking the active one (002 T017)
- [X] T027 Profile switcher in the UI (`src/App.tsx`)
- [X] T028 Write the failing test first, then implement: expiry remaining-time in the tray
      tooltip (002 T020) — `Engine::expiry_remaining(&Snapshot) -> Option<u64>` is the pure part
      and is what the test pins; the tooltip string is the caller.
- [X] T029 [P] `src-tauri/src/platform/windows/session.rs` — `GetSystemMetrics(SM_REMOTESESSION)`
      behind a trait, sampled in the existing ~1 s tick; fills `Snapshot.remote_session` and drops
      its `#[allow(dead_code)]` (002 T012, research R2). No message pump, no window.

## Phase 8 — Polish & cross-cutting

- [X] T030 Tighten `src-tauri/capabilities/main.json` — it still says "M1 placeholder" and grants
      `core:default`. Enumerate only what the app uses. (001 T003)
- [X] T031 [P] Correct `docs/ARCHITECTURE.md` and `docs/WINDOWS-API.md`: `power/inspect.rs` on
      `GetPowerRequestList` is not buildable unelevated. The docs are the specification — they get
      fixed, not worked around. (constitution, Development Workflow)
- [~] T032 SC-005 accessibility pass. **Static half done**: every new control is a native
      `<button>`/`<select>`/`<input>` so keyboard nav is inherent; `:focus-visible` outlines are
      defined for `.rail button`, `.btn` and `.switch`; no `outline: none` in any shipped
      stylesheet (`src/App.css:91` has one but that file is never imported — dead, delete it).
      **Not done**: the manual tab-through and the High Contrast pass. Needs eyes on a window.
- [X] T033 Full gate run: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`,
      `tsc --noEmit`, `npm run build`, honesty grep, platform-boundary grep
- [~] T034 Quickstart walk — **headless half done**, against the built binary:
      startup clean · first-run latch correct (no config written until answered) · single-instance
      arg forwarding · engine reconcile to KeepPresenting/KeepRunning · config v2 persistence
      including the new `input` block · **T003 profile-merge verified live** (3 profiles survive a
      save that changes the mode) · clean process exit with no leaked request.
      **Not done**: everything needing a visible window — SC-001 (destroy-not-hide, working-set
      return, open ≤400 ms), SC-003 panel wording on screen, SC-005 High Contrast, SC-006
      no-animation-at-rest, SC-007 first-run click-through. Also T030's tightened capabilities are
      only proven not to break *startup*; the window's `listen` path is unverified.

---

## Dependencies

```
T003 ──┬─→ T004 ──→ T025 ──→ T026, T027
       └─────────────↗
T011 ──→ T015 ──→ T016
T012 ──→ T013 ──→ T014 ──→ T015
T022 ──→ T023 ──→ T024
T029, T030, T031  independent
everything ──→ T033 ──→ T034
```

## Parallel opportunities

- **T004 ∥ T011 ∥ T012** — three separate new files, all pure/mock, no shared state
- **T029 ∥ T030 ∥ T031** — platform file, capabilities JSON, docs; no overlap
- Phase 4 (US2) and Phase 6 (US4) touch different Rust modules and different React modules; only
  `ipc/mod.rs` and `App.tsx` are shared, so land them sequentially at those two files.

## Implementation strategy

**MVP is Phase 4 (US2).** It is the flagship, it is the reason to install the tool even if the
main function is never switched on, and it is the only part whose feasibility was in question.
Ship it, then US4, then Phase 7.
