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
- [X] **T010** CI grep: no "undetectable/human-like/natural" in strings/docs (PRODUCT §5 Test 3).
      Live in `.github/workflows/ci.yml`, scoped to shipped code/UI (docs legitimately *discuss*
      the banned words as the line not to cross).
- [~] **T011** Exit criteria SC-001..SC-006; `cargo test` green (filter tested against a fake clock).
      Automatable ones pass. **Not run**: multi-monitor negative-origin, mixed-DPI, and the 49-day
      `GetTickCount` wrap — all need hardware or a clock the test suite cannot move.

## Phase 5 — C2 and C5, which Phase 1–4 never tasked

Added 2026-08-28. Part C of `docs/FEATURES.md` specifies seven items; the original four phases
scoped **C1, C3, C6, C7** and simply never wrote tasks for the rest. Same failure the M3 list had.
Prompted by comparing the shipped UI against Move Mouse's action editor — its Distance, Direction
and Random fields are all our own spec, unbuilt.

- [X] **T012** `core/motion.rs` (test-first): `Motion` = Virtual · Line · Square · Circle, and
      `step(index, distance)`. Every path is **closed** — a full cycle sums to zero, so the cursor
      ends where the user left it. Guaranteed by construction (the circle's second half is the
      negation of its first), not by arithmetic on the way back. C2.
- [X] **T013** `motion::vary(value, pct, seed)` (test-first) — C5. Interval and distance variation
      via a tiny xorshift seeded from the tick, so no RNG state and no new dependency. Tests pin
      the band, that it actually varies, and that it never returns 0 (an interval of 0 is a busy
      loop). **Framing is fixed by PRODUCT §5 Test 3**: it ships as "vary the movement so it is
      less intrusive" and no string may call it human-like or undetectable.
- [X] **T014** `InputInjector::move_relative(dx, dy)` + `SendInput` implementation, tagged with the
      same magic `dwExtraInfo` as the rest. `MockInjector` records every move so a test can assert
      the cursor came back.
- [X] **T015** Engine dispatch: key wins over motion, `Motion::Virtual` stays the default, the step
      index advances even on a failed injection so a blocked cycle does not restart mid-path and
      leave the cursor displaced. Distance clamped 1–500 px, variation clamped to 50%.
- [X] **T016** Settings UI: What to send · Movement · Distance (shown only for a visible motion) ·
      Vary by. Persisted in `Config`. Verified on screen: choosing "Around a square" reveals the
      Distance row and writes `"motion": "Square"`.

### Deliberately not built

- **C4 click and scroll.** Specified, and specified *with a warning*: a synthetic click lands on
  whatever is under the cursor, so it can answer a dialog the user left focused. FEATURES requires
  it be off by default and gated behind a foreground-application condition. That gate is real work
  and the feature is genuinely destructive without it, so it is not being slipped in alongside
  movement. Move Mouse's own remote-session guide ("hover it over the Start Button") is the shape
  to copy when it is built.
- **Move Mouse's "action list" model.** Constitution I forbids it in as many words: the power
  engine holds a *state*, the input engine fires *events*, and "modelling both as actions is
  forbidden". Move Mouse is mechanism (B) only, so a flat action list is coherent *for it*. Here
  the equivalent is rules plus one input engine.
- **The on-screen widget / mascot.** Not in `docs/` at all. It conflicts with UI-UX §4 (nothing
  loops or animates at rest) and with the idle-CPU budget, and the tray icon plus tooltip already
  carry the state it would show.

Dependencies: P1 (pure filter) → P2 (injector) → P3 (wire) → P4 (honesty/verify). P1 is the P0 core
and is fully testable with zero OS code.
