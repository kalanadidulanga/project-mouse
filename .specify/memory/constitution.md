# project-mouse Constitution

The non-negotiable principles every spec, plan, task, and implementation is checked
against. Derived from `docs/` — when this file and the docs disagree, the docs are the
source and this file is fixed to match. Full rationale lives in the docs referenced
per principle.

## Core Principles

### I. Two mechanisms, kept separate — power by default, input opt-in

The product is the distinction between **power inhibition** (mechanism A: `PowerCreateRequest`/
`PowerSetRequest`, sanctioned, ~zero policy risk) and **input synthesis** (mechanism B:
`SendInput`, high risk). The power engine is **on by default**; the input engine is **off by
default, opt-in, enabled only after one honest explanation**. The two are architecturally
separate all the way down: the power engine holds a *state* (reconciled per tick, idempotent,
released on exit); the input engine fires *events* (dispatched from rule firings). Modelling
both as "actions" is forbidden. There is exactly one place that gates every input event on the
global input-synthesis switch, and it is not in the UI. (`docs/PRODUCT.md §2`, `docs/ARCHITECTURE.md §0`)

### II. Honest naming (NON-NEGOTIABLE)

If a feature cannot be described accurately in a UI string, it does not ship. No string, doc
line, release note, or marketing copy ever describes any feature as *undetectable*, *human-like*,
*natural*, or as evading monitoring. A feature that only makes sense as detection evasion fails
review and is cut — this already cost us "human-like motion." Randomisation ships only as "vary
the movement so it is less intrusive." CI greps for the banned words. (`docs/PRODUCT.md §5`)

### III. Non-destructive and auditable

Never modify persistent system state — no power-plan edits, no policy-key writes, no
`SPI_SETSCREENSAVEACTIVE`, no volume changes. Every power request carries a specific
`REASON_CONTEXT` string so it appears in `powercfg /requests` attributed to us by name. The
request is released on every exit path **and** in a panic hook; a leaked request is a
correctness bug, not a nicety. "Does not modify your power plan; releases everything on exit"
must stay literally true. (`docs/FEATURES.md A3`, `docs/WINDOWS-API.md gotcha 12`)

### IV. Platform boundary from commit one

All OS-specific code lives behind the `platform/` traits (`InputInjector`, `IdleMonitor`,
`PowerGuard`, `ProcessMonitor`, `ForegroundMonitor`, `SessionMonitor`, `AutoStart`, …). No
`#[cfg(windows)]` outside `platform/` — it is a CI lint. This yields a `MockPlatform` for free,
which is what makes the engine unit-testable without Win32. `ipc/` contains no logic; every
command is a thin wrapper over a `core` call. (`docs/CROSS-PLATFORM.md §2`, `docs/ARCHITECTURE.md §4`)

### V. Test-first for engine logic (NON-NEGOTIABLE)

Rule engine, condition/trigger evaluation, cooldowns, the self-injection filter, idle-time
wrap/clamp arithmetic, and config migrations are pure logic and are written test-first against
`MockPlatform` — no Win32 in the test suite. Red → green → refactor. Win32 FFI wrappers that
cannot be unit-tested are exercised by the M0-style manual/integration checks and named as such.
(`docs/ARCHITECTURE.md §11`)

### VI. Config safety

`schema_version` from the very first release, with a migration chain. Atomic write (temp +
fsync + `MoveFileEx` replace), debounced. On a parse error, **never silently fall back to
defaults** — surface the error, keep the broken file, offer a reset. No SQLite. (`docs/ARCHITECTURE.md §8`, `docs/FEATURES.md D8`)

## Technology & Footprint Constraints

- **Stack:** Rust 2021 · Tauri v2 (`default-features = false`, minimal features) · windows-rs ·
  React+TS loaded only when a window exists · JSON config · `tracing`.
- **Memory discipline:** the settings window is **created on demand and `destroy()`-ed on close,
  never hidden** (M0-validated: `destroy()` frees the WebView2 children; `hide()` does not). No
  application state lives in the frontend. Every `#[tauri::command]` is synchronous so tokio stays
  dormant at idle.
- **Budgets (CI-gated, fail the build on regression):** idle private working set ≤ 8 MB tray-only ·
  idle CPU ≤ 0.05 % · installed size ≤ 8 MB · cold start ≤ 250 ms · **zero runtime dependencies** ·
  **never requires admin**. (`docs/README.md`, `docs/M0-RESULTS.md`)
- **No telemetry, ever.** Not opt-in, not anonymous, not crash reports. A tool that watches input
  must not phone home. No cloud sync. (`docs/FEATURES.md`, `docs/ROADMAP.md`)

## Development Workflow

- The `docs/` are the specification. spec-kit specs/plans/tasks reference them; they do not
  restate or override them.
- Milestones ship in ROADMAP order (M0 spike done → M1 wake engine → …), each with the
  ROADMAP's checkable exit criteria. A milestone is done when someone other than the author can
  verify it.
- Five things are conditions on every commit, never "later": platform boundary, power-request
  release, config migrations, no telemetry, honest naming. (`docs/ROADMAP.md`)
- Commit as work progresses, in logical units.

## Governance

This constitution supersedes convenience. Any deviation must be justified against the docs in the
plan's Complexity Tracking, or the plan is wrong. Complexity that is not demanded by a documented
requirement is removed (YAGNI). Amendments follow the docs: change the docs first, then this file.

**Version**: 1.0.0 | **Ratified**: 2026-08-22 | **Last Amended**: 2026-08-22
