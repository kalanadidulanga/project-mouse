# Implementation Plan: M3 — The UI (leftovers)

**Branch**: `main` | **Date**: 2026-08-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-settings-ui/spec.md`

## Summary

M3 shipped its shell, status page, activity log, config v2, and (in `16c5ec1`) a rule builder
covering every `Condition` variant plus a timer and the input-engine settings. Five tasks were
left `[~]`. This plan closes them, plus the three `[~]` tasks in `002-conditions` that belong to
the same surface (profile switching, tray tooltip expiry, remote-session sampling).

The one design question that needed answering — can the flagship "Why is my PC awake?" panel
work without admin — is resolved in [research.md](./research.md): **not via
`GetPowerRequestList`, which refuses an unelevated caller outright**, but via
`SystemExecutionState`, which returns the machine-wide aggregate and does work. The panel ships
answering "is something holding this machine awake, and is it us?" and hands over the elevated
command that names the offender.

## Technical Context

**Language/Version**: Rust 2021 · TypeScript 5 / React 18

**Primary Dependencies**: Tauri v2 (`default-features = false`), windows-rs 0.62, serde, tracing.
No new dependency is introduced by this plan.

**Storage**: JSON config, `schema_version` 2, atomic write (existing `config/store.rs`)

**Testing**: `cargo test` against `MockPlatform` for all pure logic; Win32 wrappers verified by
the `quickstart.md` manual passes, which are named as such

**Target Platform**: Windows 10/11, unelevated, no runtime dependencies

**Project Type**: Desktop app — tray-resident Rust core, on-demand WebView2 settings window

**Performance Goals**: window open→interactive ≤ 400 ms; nothing loops or animates at rest

**Constraints**: idle working set ≤ 8 MB tray-only · idle CPU ≤ 0.05 % · **never requires admin**
· window `destroy()`-ed on close, never hidden · every `#[tauri::command]` synchronous

**Scale/Scope**: 4 pages, ~8 new IPC commands, 2 new platform trait methods

## Constitution Check

*GATE: passed before Phase 0, re-checked after Phase 1 design.*

| Principle | Check | Verdict |
|---|---|---|
| **I** — two mechanisms kept separate | Nothing here fires input events. The first-run flow creates power-only profiles and the spec requires it synthesize no input (SC-007). | ✅ |
| **II** — honest naming | E1 is the principle applied to ourselves: it reports what is true, says plainly what it cannot see, and does not pretend a partial answer is complete. CI grep still gates the strings. | ✅ |
| **III** — non-destructive and auditable | `SystemExecutionState` and `SM_REMOTESESSION` are **reads**. No persistent system state is touched. | ✅ |
| **IV** — platform boundary | `SystemExecutionState` and `SM_REMOTESESSION` go behind existing/new `platform/` traits with `MockPlatform` implementations. No `#[cfg(windows)]` escapes `platform/`; CI lints it. | ✅ |
| **V** — test-first for engine logic | Profile collection management, the E1 "is it us" subtraction, and expiry-remaining formatting are pure logic → tests first against `MockPlatform`. The two Win32 reads are FFI wrappers, named as manually verified. | ✅ |
| **VI** — config safety | `set_profile` and the first-run flow write through the existing atomic path. **The `persist_current` collection-flattening bug (research R3) is fixed before any second profile can exist**, or this principle is violated on the first save. | ⚠️ → fixed by T014 |

**No violations require justification.** The Complexity Tracking table is therefore empty.

## Project Structure

### Documentation (this feature)

```text
specs/003-settings-ui/
├── plan.md              # This file
├── research.md          # Phase 0 — the E1 privilege finding, and R2/R3
├── data-model.md        # Phase 1 — new types crossing the IPC boundary
├── quickstart.md        # Phase 1 — the runnable validation passes (SC-001..SC-007)
├── contracts/
│   └── ipc.md           # Phase 1 — the command surface this feature adds
├── spec.md
└── tasks.md
```

### Source Code (repository root)

```text
src-tauri/src/
├── platform/
│   ├── mod.rs                   PowerInspector + SessionMonitor traits (+ MockPlatform impls)
│   ├── mock.rs                  mock implementations for the test suite
│   └── windows/
│       ├── inspect.rs           NEW — CallNtPowerInformation(SystemExecutionState)
│       └── session.rs           NEW — GetSystemMetrics(SM_REMOTESESSION)
├── core/
│   ├── awake.rs                 NEW — pure "who is holding what" reduction (tested)
│   ├── engine.rs                expiry-remaining accessor for the tray tooltip
│   └── profiles.rs              NEW — the profile collection (tested)
├── config/
│   └── store.rs                 unchanged; `lib.rs::persist_current` stops flattening
├── ipc/mod.rs                   + why_awake, list_profiles, set_profile, complete_first_run
└── lib.rs                       tray profile submenu; tooltip with remaining time

src/
├── App.tsx                      + WhyAwake panel, profile switcher, first-run gate
├── rules.tsx                    unchanged
├── firstrun.tsx                 NEW — one question, three answers
└── styles.css
```

**Structure Decision**: the existing layout is unchanged — this feature adds two files under
`platform/windows/`, three under `core/`, and one React module. It matches
[ARCHITECTURE.md §"Source layout"](../../docs/ARCHITECTURE.md), except that the file that
architecture names `power/inspect.rs` lands at `platform/windows/inspect.rs`, because after
research R1 it is a **platform read behind a trait**, not a power-engine component. Recording the
divergence here so the next reader is not hunting for a file that does not exist.

## Complexity Tracking

> No Constitution Check violations. Nothing to justify.
