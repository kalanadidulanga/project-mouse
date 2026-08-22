# Implementation Plan: M1 — The Wake Engine

**Branch**: `001-wake-engine` | **Date**: 2026-08-22 | **Spec**: [spec.md](./spec.md)

**Input**: [spec.md](./spec.md); authoritative design in `docs/ARCHITECTURE.md`, `docs/FEATURES.md`,
`docs/WINDOWS-API.md`, `docs/TAURI-V2.md`; principles in `.specify/memory/constitution.md`;
stack validated by `docs/M0-RESULTS.md`.

## Summary

Ship the power-inhibition core: a tray-only Tauri v2 app (no window) that holds/releases the
correct power requests for three modes (Off / Keep running / Keep presenting), owned by one
long-lived scheduler thread and reconciled idempotently against desired state, released on every
exit path and in a panic hook, auditable via a `powercfg`-visible reason string. Plus the plumbing
everything later needs: the `platform/` trait boundary + `MockPlatform`, versioned atomic JSON
config, `tracing` rolling-file logging, single-instance, optional `HKCU\Run` autostart, and
portable mode. No rules, no UI window, no input synthesis.

## Technical Context

**Language/Version**: Rust 2021 (1.98 MSVC toolchain, `x86_64-pc-windows-msvc`)

**Primary Dependencies**: `tauri` v2 (`default-features = false`, features `wry`,
`common-controls-v6`, `tray-icon`, `image-ico`), `windows` 0.62 (Win32_System_Power,
_Threading, _SystemServices, _ProcessStatus, _Registry, Foundation), `tracing` +
`tracing-appender`, `serde`/`serde_json`, `tauri-plugin-single-instance`,
`tauri-plugin-autostart`. **No SQLite. No tokio at idle** (all commands synchronous).

**Storage**: single `config.json` — beside the exe in portable mode, else
`%APPDATA%\project-mouse`. Atomic write (temp + fsync + `MoveFileEx` replace), debounced 500 ms,
`schema_version` + migration chain.

**Testing**: `cargo test` — engine/config/migration logic unit-tested against `MockPlatform`,
zero Win32 in the suite. Win32 wrappers verified by the M0-style manual/integration checks and
the `powercfg` release checks.

**Target Platform**: Windows 10 1809+ / 11 (desktop, tray-only). macOS/Linux are compiling stubs
behind the platform traits.

**Project Type**: Desktop app (single process, Tauri shell + Rust core).

**Performance Goals**: idle CPU ≤ 0.05 %; idle private working set ≤ 8 MB tray-only; cold start ≤
250 ms; power state change reflected within one 1 s tick.

**Constraints**: zero runtime dependencies; never requires admin; no telemetry; no persistent
system-state mutation; release-on-exit guaranteed including panic.

**Scale/Scope**: tens of KB of config; one process, ~3 long-lived threads at rest (main/event
loop, scheduler, executor is dormant in M1); a handful of tray menu items.

## Constitution Check

*GATE: passes.*

- **I. Two mechanisms separate** — M1 ships the power engine only; input engine absent. Power is
  modelled as held *state* reconciled per tick, never as an event/action. ✅
- **II. Honest naming** — no input synthesis and no marketing surface in M1; tooltip states what is
  true of the machine. CI banned-word grep added in M5, but no offending strings introduced. ✅
- **III. Non-destructive & auditable** — `PowerCreateRequest` path only, reason string, release on
  exit + panic hook, no persistent-state writes. The `powercfg` clean-after-kill test is an exit
  criterion. ✅
- **IV. Platform boundary** — `PowerGuard`, `AutoStart` (+ stub monitors) behind `platform/`; no
  `#[cfg(windows)]` in `core/`; `MockPlatform` delivered. ✅
- **V. Test-first engine logic** — mode reconciliation, config migration, portable-path detection
  written test-first against `MockPlatform`. ✅
- **VI. Config safety** — `schema_version`, atomic write, no silent reset. ✅

No violations → Complexity Tracking empty.

## Project Structure

### Documentation (this feature)

```text
specs/001-wake-engine/
├── plan.md              # this file
├── spec.md              # the M1 spec
├── research.md          # decisions carried from M0 + open M1 questions
├── data-model.md        # WakeMode, Config, Platform traits
├── quickstart.md        # how to build/run/verify M1
└── tasks.md             # /speckit-tasks output
```

### Source Code (repository root)

M1 subset of `docs/ARCHITECTURE.md §4`. Directories exist only when M1 needs them; the rest
arrive with their milestone. No `#[cfg(windows)]` leaves `platform/`.

```text
src-tauri/src/
├── main.rs                 # bootstrap, single-instance, tray, run loop, prevent_exit, panic hook
├── ipc/
│   └── mod.rs              # thin Tauri commands (get_state, set_mode) — sync only
├── core/
│   ├── mod.rs
│   ├── engine.rs          # the 1 s tick: read desired mode → reconcile power
│   ├── state.rs           # RwLock<AppState>, change events (only when a window exists)
│   └── modes.rs           # WakeMode, combine-by-maximum
├── power/
│   ├── mod.rs             # desired-vs-actual reconciliation, single owner
│   └── request.rs         # (Windows) PowerCreateRequest handle + reason strings
├── platform/
│   ├── mod.rs             # traits: PowerGuard, AutoStart, (stub) monitors; Capabilities; Platform
│   ├── mock.rs            # MockPlatform for tests
│   └── windows/
│       ├── mod.rs
│       ├── power.rs       # PowerGuard impl (PowerCreateRequest/SetRequest/ClearRequest)
│       └── autostart.rs   # HKCU\Run
├── config/
│   ├── mod.rs
│   ├── model.rs          # serde types, schema_version
│   ├── store.rs          # atomic save (temp+fsync+rename), debounce, portable-path detection
│   └── migrate.rs        # versioned migration chain
├── timing/
│   └── ticker.rs         # CreateWaitableTimerExW + tolerable delay
└── logging/
    └── mod.rs            # tracing subscriber, rolling file

src-tauri/capabilities/main.json   # enumerated permissions (no core:default)
src-tauri/tauri.conf.json          # windows:[{create:false}], nsis currentUser, minimal features
src/                                # React/TS scaffold — present but unused until M3
```

**Structure Decision**: single Tauri desktop project at repo root; `core/` is Win32-free and
testable via `MockPlatform`; `platform/windows/` holds the only real implementation in M1. The
`src/` React app is scaffolded but no window is created in M1.

## Complexity Tracking

No constitution violations. (Section intentionally empty.)
