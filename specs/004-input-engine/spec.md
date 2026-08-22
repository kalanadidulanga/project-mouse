# Feature Specification: M4 — The Input Engine

**Feature Branch**: `004-input-engine` · **Created**: 2026-08-22 · **Status**: Draft

**Input**: ROADMAP.md M4. The opt-in second engine — synthetic input. **Off by default**, enabled
only after one honest explanation. Authoritative design: `docs/FEATURES.md Part C`,
`docs/ARCHITECTURE.md §6` (self-injection feedback loop), `docs/WINDOWS-API.md` (gotchas 3-8).
This half is the riskiest; C6 (stand-down) and C7 (blocked detection) are **P0** — the conditions
under which shipping input synthesis at all is defensible.

## User Scenarios & Testing

### US1 — Defeat a session/presence timer, honestly (P1)
The user enables input synthesis (one plain paragraph explains it changes behaviour). While they are
genuinely idle, the app resets the session idle timer with a **virtual jiggle** — no visible cursor
movement. **Independent test:** with synthesis on and the user idle, the system idle clock resets
while the cursor does not move; with synthesis off (default), no input is ever produced.

### US2 — Never fight the user (P0, C6 stand-down)
The instant real human input arrives, any in-flight injection is cancelled and the next trigger is
suppressed until the idle threshold is met again. **Independent test:** the human-idle clock keeps
counting up while the app jiggles (proving our own input is filtered out), and drops to zero the
moment the real user moves.

### US3 — Tell the truth when it can't act (P0, C7 blocked detection)
When injection is silently discarded (UIPI: an elevated window or the lock screen owns the
foreground), the app detects it (the system idle timer did not reset after an injection) and shows a
first-class **Blocked** state. **Independent test:** focus an elevated window, confirm the idle timer
does not reset and the tray/UI report Blocked — never a false "working".

### US4 — Two idle clocks (P0, E2)
`system_idle` (what Windows/presence/the screensaver see) and `human_idle` (what the user actually
did, our injections filtered out) are both visible. This diagnoses the two commonest support
failures (self-injection confusion; the NVIDIA-GeForce idle-reset bug) at a glance.

### Edge cases
- 49-day `GetTickCount` wrap / non-monotonic `dwTime` → clamp (WINDOWS-API gotcha 1).
- `SendInput` reports success under UIPI but goes nowhere (gotcha 3) → verify via idle-reset, not
  the return value.
- Tag our own events with `dwExtraInfo` (gotcha 4); send down+up in one `SendInput` call (gotcha 8).
- Default key `VK_F15` (gotcha: not Shift/Scroll Lock); key is configurable (F15 breaks in PuTTY etc).

## Requirements

- **FR-001 (C1)**: A **virtual jiggle** — a relative `SendInput` that nets zero visible movement —
  is the default action when synthesis is enabled.
- **FR-002**: Input synthesis is **off by default**; a single global switch gates every dispatched
  event at exactly one place (not in the UI).
- **FR-003 (§6)**: The self-injection filter tracks the tick of our last injection; a system-idle
  reset within a tolerance of that tick is attributed to us and does **not** advance `human_idle`.
- **FR-004 (E2)**: Expose `system_idle_ms` and `human_idle_ms` separately.
- **FR-005 (C6, P0)**: Real input (human_idle below the stand-down threshold) cancels any in-flight
  sequence and suppresses the next trigger until idle again.
- **FR-006 (C7, P0)**: After an injection that should reset the idle timer, if `system_idle` did not
  drop, mark the injector **Blocked** and surface it (tray + diagnostics).
- **FR-007 (C3)**: Keystroke action defaults to `VK_F15`; configurable.
- **FR-008**: All injected events carry a magic `dwExtraInfo`; key/button down+up go in one call.
- **FR-009 (arch)**: `InputInjector` behind `platform/`; `core/` stays Win32-free; the filter is
  unit-tested against a fake clock/injector.

## Success Criteria (ROADMAP M4)

- **SC-001**: Synthesis off on a fresh install; enabling it shows one honest paragraph.
- **SC-002**: Virtual jiggle resets the system idle timer with **zero** visible cursor movement.
- **SC-003**: Stand-down cancels an in-flight sequence within 250 ms of real input.
- **SC-004**: The self-injection filter is correct — `human_idle` keeps rising while the app jiggles
  (unit-tested against a fake clock).
- **SC-005**: Blocked detection — an elevated foreground window yields a Blocked state, not a false OK.
- **SC-006**: No UI string / doc / release note calls any feature human-like, natural, or
  undetectable (grep gate).

## Assumptions
- Windows only. Visible movement/paths/click/scroll/randomisation are secondary to the P0 core
  (C6/C7/E2); the honest virtual jiggle + F15 + the filter are the milestone's spine.
- No global hooks (`LLMHF_INJECTED` unavailable) — the timestamp-window filter is the mechanism.
