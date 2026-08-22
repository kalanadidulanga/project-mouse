# Feature Specification: M3 — The UI

**Feature Branch**: `003-settings-ui` · **Created**: 2026-08-22 · **Status**: Draft

**Input**: ROADMAP.md M3. The settings window, rule builder, diagnostics, first run. Authoritative
design: `docs/UI-UX.md` (interaction model, motion budget, layouts) + the mockup; `docs/ARCHITECTURE.md
§3` (window lifecycle — the memory discipline), `§8` (IPC surface); `docs/FEATURES.md Part E`
(diagnostics). This spec scopes M3; `docs/UI-UX.md` is the detailed source of truth.

## User Scenarios & Testing

### US1 — Open settings, see status, act (P1)
Left-click the tray → a small window opens (created on demand). It answers *is it working?* in one
sentence and offers pause/resume + profile. Closing it **destroys** the webview (memory returns to
the ~1 MB idle baseline). **Independent test:** open → interact ≤ 400 ms; close → working set back to
the M1 baseline within 30 s; assert the window is destroyed, never hidden.

### US2 — "Why is my PC awake?" + live effect (P1, flagship diagnostic)
A panel lists every process currently holding a power request (plain language), and shows what is
currently true of the machine (sleep/display/lock blocked or allowed, and why). **Independent test:**
compare the list against elevated `powercfg /requests`; say plainly when the list is partial because
we are not elevated.

### US3 — Rule builder (P2)
Rules read as English sentences with inline dropdowns (`Keep ▸ running · While ▸ msbuild.exe is
running`). No node graph. New rules are disabled by default. Rules persist (config v2). **Independent
test:** build a rule in the UI → verify the JSON → reload → unchanged.

### US4 — First run (P2)
One question, three answers (finish a long job / keep a screen up / set it up), each creating a
working profile in <10 s, synthesizing no input. Then the window closes and destroys itself.

### Edge cases
- Window destroyed mid-operation: all state lives in Rust; React holds only a projection.
- No power requests held by anyone → the panel says so, not an empty void.
- Unelevated → power-request list is partial; show the copyable elevated `powercfg` command.
- High-contrast mode + full keyboard nav + `prefers-reduced-motion` honoured.

## Requirements

- **FR-001**: Window created on demand, **destroyed** on close (never `hide()`); app stays alive with
  zero windows. Reuses the declared `create:false` window config.
- **FR-002**: Sync Tauri commands only (keep tokio dormant): `get_state`, `set_mode`, `pause_all`/
  `resume_all`, `get_diagnostics`, `get_logs`, `upsert_rule`/`delete_rule`/`set_rule_enabled`,
  `set_profile`. `state:changed` event emitted only when a window exists.
- **FR-003**: Status page — one sentence + one switch above the fold; shows profile, today's counts,
  and **memory** (the public promise, checkable in place).
- **FR-004 (E1)**: "Why is my PC awake?" enumerates other processes' power requests
  (`CallNtPowerInformation(PowerRequestInfo)`), degrading gracefully unelevated.
- **FR-005 (E3)**: Effect readout — what is currently true (sleep/display/lock blocked-or-allowed +
  why), not "the tool is running".
- **FR-006**: Rule builder as plain-language sentences; rules disabled-by-default; a rule that
  synthesizes input is labelled on its row (none do in M3).
- **FR-007**: Config **v1→v2** migration — persist profiles + rules + active profile; atomic.
- **FR-008**: First-run flow creates a working profile, synthesizes no input, then self-destructs.
- **FR-009 (visual)**: Follow Windows (Segoe UI, Fluent radius, system accent, light/dark). Motion
  budget per UI-UX §4 — no looping animation; `transform`/`opacity` only. Full a11y (keyboard,
  contrast, High Contrast, reduced-motion).

## Success Criteria (ROADMAP M3)

- **SC-001**: Window created on demand and destroyed on close (asserted); working set returns to the
  M1 baseline within 30 s; open-to-interactive ≤ 400 ms.
- **SC-002**: Rule builder round-trips (build → JSON → reload → unchanged).
- **SC-003**: "Why is my PC awake?" lists other processes' requests and states when partial (unelevated).
- **SC-004**: Both idle clocks visible and correct (system idle now; human idle full in M4).
- **SC-005**: Full keyboard nav; visible focus (no `outline:none`); correct in High Contrast.
- **SC-006**: Nothing loops/animates at rest.
- **SC-007**: First run creates a profile in <10 s and synthesizes no input.

## Assumptions
- Windows only. React + TS + Vite (scaffolded in M1); no UI framework beyond React + CSS.
- Human-idle clock is approximate until M4's self-injection filter; system-idle is exact now.
- Config v2 persistence lands here (M2 deferred it — now the UI creates rules to persist).
