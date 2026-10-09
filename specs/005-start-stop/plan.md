# M7 Start/Stop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make project-mouse an app you can see and understand. The window opens at launch.
One Start/Stop button moves the mouse along a whole visible path after N seconds with no
input, and keeps the PC awake. A countdown says when the next move happens. The tray shows
whether it is running.

**Architecture:** The two engines stay separate (constitution I). A new pure module,
`core/running.rs`, owns the single invariant: *running ⇔ manual mode ≠ Off*, and *input on ⇔
running ∧ move_mouse*. Every Start/Stop path (IPC, tray, hotkey, CLI) goes through one lib.rs
helper that applies it. Timing becomes "system idle ≥ interval". Each move traces a whole closed
path through a new `InputInjector::move_path`. The UI splits into Home / Settings / Advanced,
and the existing panels move to Advanced unchanged.

**Tech Stack:** Rust 2021 · Tauri 2.11 (`tauri-plugin-autostart`, `-single-instance`,
`-global-shortcut`, `-updater`) · windows-rs 0.62 · React 19 + TypeScript 5.8 + Vite 7.
**No new dependencies.**

**Spec:** `specs/005-start-stop/spec.md`. Amended docs: `docs/PRODUCT.md §0`,
`docs/UI-UX.md §0.5`, `docs/FEATURES.md` Part C / C2 / D1, `.specify/memory/constitution.md`
v1.1.0.

## Global Constraints

- Windows only. No new crates, no new npm packages.
- Constitution II: the strings `undetectable|human-like|looks human|natural motion` never
  appear in `src/` or `src-tauri/src/` (the CI grep).
- Constitution IV: no `cfg(windows)` outside `src-tauri/src/platform/`. `ipc/` stays thin
  wrappers over `core` and lib.rs helpers.
- Constitution V: engine logic is test-first against `platform::mock`. No Win32 in tests.
- Every `#[tauri::command]` stays synchronous. Anything that sleeps (the Test path) runs on a
  `std::thread`.
- The window is created on demand and `destroy()`-ed on close. Never `hide()`.
- Clamps: interval 5 s–1 h, distance 1–500 px, vary ≤ 50 %.
- UI copy may name Teams and Slack as apps that watch idle time. It never promises a status.
  Home carries *"Monitoring software can detect simulated input."*
- Motion budget: nothing loops, and the countdown is text updated at most once a second.
- Commits: the repo's git config is already the personal identity
  (`kalanadidulanga <dev.kalanadidulanga@gmail.com>`). Never pass `-c user.email`. End every
  message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Commands run from the repo root. Rust: `cargo test --manifest-path src-tauri/Cargo.toml <filter>`.

## Review Focus

1. **A config that is valid JSON but not an object** (`[]`, `5`, `"x"`). Today
   `value["schema_version"] = …` panics on it, and with `panic = "abort"` the app dies at
   launch. Expected: it is treated as corrupt (error logged, file kept, saving disabled).
   Pinned in Task 4.
2. **An interval/distance/vary box left empty, typed as text, negative or huge.** Expected:
   nothing reaches Rust as `null` or overflows `u32`/`u8`, and the box shows the value in
   effect. Pinned by `NumberField` in Task 8 and the clamp tests in Task 2.
3. **Windows discards or refuses the input** (an elevated window has focus). Expected: at most
   one attempt per interval, never one per second, and Home shows the warning. Pinned in
   Task 2, both the silent-discard and the hard-failure cases.
4. **Changing Move the mouse or Keep the screen on while running.** Expected: the change takes
   effect immediately and the app stays running. While stopped, it stays stopped. Pinned in
   Task 3.
5. **The cursor is at a screen edge when a move fires.** Windows clips one leg, so the cursor
   ends up displaced by at most one leg, once. Repeats must not walk it further. Windows'
   clipping cannot be unit-tested, so this is a manual check in Task 10, step 6.

---

### Task 1: The smooth closed path

**Files:**
- Modify: `src-tauri/src/core/motion.rs`, adding `MAX_PATH_STEPS`, `STEP_MS`, `path()` and tests
- Modify: `src-tauri/src/platform/mod.rs:67-78` (`InputInjector` gains `move_path`)
- Modify: `src-tauri/src/platform/windows/input.rs` (implement `move_path`)
- Modify: `src-tauri/src/platform/mock.rs:96-142` (`NoopInjector`, `MockInjector`)

**Interfaces:**
- Produces: `pub const MAX_PATH_STEPS: u32 = 40;` · `pub const STEP_MS: u32 = 10;` ·
  `pub fn path(motion: Motion, distance: i32) -> Vec<(i32, i32)>` ·
  `InputInjector::move_path(&self, steps: &[(i32, i32)], step_ms: u32) -> Result<u32>`
  (returns the elapsed ms).

- [ ] **Step 1: Write the failing tests.** Append to the `tests` module in `core/motion.rs`:

```rust
    fn sum(p: &[(i32, i32)]) -> (i32, i32) {
        p.iter().fold((0, 0), |(x, y), (dx, dy)| (x + dx, y + dy))
    }

    /// Spec 005 FR-005: one trigger traces the whole shape and ends where it started.
    #[test]
    fn every_path_returns_to_its_origin() {
        for m in [Motion::Line, Motion::Square, Motion::Circle] {
            for d in [1, 2, 7, 10, 33, 500] {
                assert_eq!(sum(&path(m, d)), (0, 0), "{m:?} at {d}px did not close");
            }
        }
    }

    #[test]
    fn a_path_glides_in_small_steps_and_stays_short() {
        for m in [Motion::Line, Motion::Square, Motion::Circle] {
            let p = path(m, 10);
            assert!(p.len() > m.steps() as usize, "{m:?} jumps instead of gliding: {p:?}");
            assert!(p.len() as u32 <= MAX_PATH_STEPS);
            assert!(path(m, 500).len() as u32 <= MAX_PATH_STEPS);
        }
    }

    #[test]
    fn a_square_path_goes_right_down_left_up() {
        let p = path(Motion::Square, 10);
        assert_eq!(p.len(), 20);
        assert!(p[..5].iter().all(|&s| s == (2, 0)));
        assert!(p[5..10].iter().all(|&s| s == (0, 2)));
        assert!(p[10..15].iter().all(|&s| s == (-2, 0)));
        assert!(p[15..].iter().all(|&s| s == (0, -2)));
    }

    /// Pointer acceleration sees the same speeds on the way out and the way back.
    #[test]
    fn opposite_legs_use_mirrored_steps() {
        let p = path(Motion::Line, 7);
        let (out, back) = p.split_at(p.len() / 2);
        let mirrored: Vec<_> = out.iter().map(|&(x, y)| (-x, -y)).collect();
        assert_eq!(back, &mirrored[..]);
    }

    #[test]
    fn invisible_and_zero_distance_have_no_path() {
        assert!(path(Motion::Virtual, 500).is_empty());
        for m in [Motion::Line, Motion::Square, Motion::Circle] {
            assert!(path(m, 0).is_empty(), "{m:?} moved with distance 0");
        }
    }
```

- [ ] **Step 2: Run them and confirm they fail.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml motion`
Expected: compile error, `cannot find function 'path'`.

- [ ] **Step 3: Implement.** In `core/motion.rs`, insert this between `impl Motion { … }` and
`fn xorshift`:

```rust
/// The most steps one trigger may take. At [`STEP_MS`] apart that is under half a second of
/// sleeping, which Windows' ~15.6 ms timer can stretch to about 0.6 s.
pub const MAX_PATH_STEPS: u32 = 40;

/// Milliseconds between the steps of a path.
pub const STEP_MS: u32 = 10;

/// One trigger's whole closed path (spec 005 FR-005): every leg of the cycle, split into steps
/// of about 2 px so the cursor glides rather than jumps. Each leg's steps sum exactly to that
/// leg (the split telescopes), and the legs already sum to zero, so the path ends where it
/// started. Opposite legs split into mirrored steps, so pointer acceleration treats the way out
/// and the way back alike. `Virtual` and a zero distance have no path.
pub fn path(motion: Motion, distance: i32) -> Vec<(i32, i32)> {
    if motion == Motion::Virtual {
        return Vec::new();
    }
    let legs = motion.steps();
    let per_leg = (MAX_PATH_STEPS / legs).max(1);
    let mut out = Vec::with_capacity(MAX_PATH_STEPS as usize);
    for i in 0..legs {
        let (dx, dy) = motion.step(i, distance);
        let len = dx.unsigned_abs().max(dy.unsigned_abs());
        let n = (len / 2).clamp(1, per_leg) as i32;
        for k in 0..n {
            out.push((dx * (k + 1) / n - dx * k / n, dy * (k + 1) / n - dy * k / n));
        }
    }
    out.retain(|&s| s != (0, 0));
    out
}
```

In `platform/mod.rs`, add this method to `trait InputInjector`, after `move_relative`:

```rust
    /// Trace a whole path of relative moves `step_ms` apart, and return how long it took in ms.
    /// The engine treats that whole span as its own input (spec 005 FR-006).
    fn move_path(&self, steps: &[(i32, i32)], step_ms: u32) -> Result<u32>;
```

In `platform/windows/input.rs`, add this to `impl InputInjector for WindowsInputInjector`:

```rust
    fn move_path(&self, steps: &[(i32, i32)], step_ms: u32) -> Result<u32> {
        let started = std::time::Instant::now();
        for (i, &(dx, dy)) in steps.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(step_ms as u64));
            }
            self.move_relative(dx, dy)?;
        }
        Ok(started.elapsed().as_millis() as u32)
    }
```

In `platform/mock.rs`, add to `impl InputInjector for NoopInjector`:

```rust
    fn move_path(&self, _steps: &[(i32, i32)], _step_ms: u32) -> Result<u32> {
        Ok(0)
    }
```

and to `impl InputInjector for MockInjector`:

```rust
    fn move_path(&self, steps: &[(i32, i32)], step_ms: u32) -> Result<u32> {
        *self.jiggles.lock().unwrap() += 1;
        self.moves.lock().unwrap().extend_from_slice(steps);
        Ok(steps.len() as u32 * step_ms)
    }
```

- [ ] **Step 4: Run the tests and confirm they pass.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml motion`
Expected: all `motion::tests` PASS, including the five new ones.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/core/motion.rs src-tauri/src/platform
git commit -m "feat(M7): one trigger traces a whole closed path (spec 005 FR-005)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The input engine: idle timing, whole-path filter, Test

**Files:**
- Modify: `src-tauri/src/core/idle.rs` (span-based self-injection filter, `in_span`)
- Rewrite: `src-tauri/src/core/input_engine.rs`
- Modify: `src-tauri/src/platform/mod.rs` (remove `move_relative` from the trait)
- Modify: `src-tauri/src/platform/windows/input.rs` (`move_relative` becomes a private inherent fn)
- Modify: `src-tauri/src/platform/mock.rs` (`MockInjector.fail`, drop `move_relative`)

**Interfaces:**
- Consumes: `motion::path`, `motion::STEP_MS`, `InputInjector::move_path` (Task 1).
- Produces: `idle::in_span(tick: u32, start: u32, end: u32) -> bool` ·
  `IdleTracker::note_injection(&mut self, start: u32, end: u32)` ·
  `InputSettings { interval_secs: u32, key: u16, motion: Motion, distance_px: u16, vary_pct: u8 }`
  (no `idle_threshold_secs`; the default is 60 s / key 0 / Square / 10 px / 0 %) ·
  `InputEngine::move_now(&mut self, now: u32)` ·
  `InputEngine::next_move_in_secs(&self) -> Option<u32>` · `MockInjector.fail: Arc<AtomicBool>`.

- [ ] **Step 1: Update the mock** so later tests can force failures. In `platform/mock.rs`,
replace the `MockInjector` struct, its `impl`, and its `impl InputInjector` with:

```rust
/// Test double: counts calls (failed ones included) and records every step of every path, so a
/// test can assert the cursor was put back where it started.
#[derive(Clone, Default)]
pub struct MockInjector {
    pub jiggles: Arc<Mutex<u32>>,
    pub moves: Arc<Mutex<Vec<(i32, i32)>>>,
    /// Make every call fail, as `SendInput` does on a desktop it cannot reach.
    pub fail: Arc<AtomicBool>,
}
impl MockInjector {
    /// Net displacement of every move so far. Should be (0, 0) after any whole path.
    pub fn net_move(&self) -> (i32, i32) {
        self.moves
            .lock()
            .unwrap()
            .iter()
            .fold((0, 0), |(x, y), (dx, dy)| (x + dx, y + dy))
    }
    fn call(&self) -> Result<()> {
        *self.jiggles.lock().unwrap() += 1;
        if self.fail.load(Ordering::SeqCst) {
            return Err(PlatformError("mock: input blocked".into()));
        }
        Ok(())
    }
}
impl InputInjector for MockInjector {
    fn virtual_jiggle(&self) -> Result<()> {
        self.call()
    }
    fn key(&self, _vk: u16) -> Result<()> {
        self.call()
    }
    fn move_path(&self, steps: &[(i32, i32)], step_ms: u32) -> Result<u32> {
        self.call()?;
        self.moves.lock().unwrap().extend_from_slice(steps);
        Ok(steps.len() as u32 * step_ms)
    }
}
```

Change the imports at the top of `mock.rs` to:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::{
    ForegroundMonitor, InputInjector, PlatformError, PowerGuard, PowerInspector, PowerSource,
    ProcessMonitor, Result, SessionMonitor,
};
```

Delete `fn move_relative` from `impl InputInjector for NoopInjector`. Delete the
`move_relative` declaration and its doc comment from `trait InputInjector` in
`platform/mod.rs`. In `platform/windows/input.rs`, take `fn move_relative` out of the trait
impl and move it, body unchanged, into the existing `impl WindowsInputInjector { … }` block
(the one holding `new`), with this doc comment:

```rust
    /// One relative step of a path. Goes through pointer acceleration, so the pixels are a
    /// request, not a promise. Paths close by construction instead (`core::motion::path`).
    fn move_relative(&self, dx: i32, dy: i32) -> Result<()> {
```

- [ ] **Step 2: Write the failing idle tests.** In `core/idle.rs`, update the existing tests:
change every `t.note_injection(X)` to `t.note_injection(X, X)` (three call sites). Then add:

```rust
    /// Spec 005 FR-006: a path takes a few hundred ms, and every step of it is ours.
    #[test]
    fn a_whole_path_is_ours() {
        let mut t = IdleTracker::new(0);
        t.note_injection(1_000, 1_400);
        t.observe(1_400); // Windows' last input = the path's last step
        assert_eq!(t.human_idle_ms(1_500), 1_500);
        t.observe(1_700); // 300 ms after the path, a person
        assert_eq!(t.human_idle_ms(1_700), 0);
    }

    #[test]
    fn the_span_survives_the_tick_wrap() {
        let s = u32::MAX - 100;
        let e = s.wrapping_add(400);
        assert!(in_span(s.wrapping_add(200), s, e));
        assert!(in_span(e, s, e));
        assert!(!in_span(e.wrapping_add(1_000), s, e));
        assert!(!in_span(s.wrapping_sub(1_000), s, e));
    }
```

- [ ] **Step 3: Write the failing engine tests.** Replace the whole `#[cfg(test)] mod tests`
in `core/input_engine.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::mock::MockInjector;
    use std::sync::atomic::Ordering;

    fn engine(m: &MockInjector) -> InputEngine {
        InputEngine::new(Arc::new(m.clone()), 0)
    }

    fn every(secs: u32, motion: Motion) -> InputSettings {
        InputSettings {
            interval_secs: secs,
            motion,
            ..InputSettings::default()
        }
    }

    fn calls(m: &MockInjector) -> u32 {
        *m.jiggles.lock().unwrap()
    }

    #[test]
    fn defaults_are_a_visible_square_after_a_minute() {
        let d = InputSettings::default();
        assert_eq!(
            (d.interval_secs, d.key, d.motion, d.distance_px, d.vary_pct),
            (60, 0, Motion::Square, 10, 0)
        );
    }

    #[test]
    fn off_by_default_never_injects() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.tick(0, 300_000);
        assert_eq!(calls(&m), 0);
        assert_eq!(e.next_move_in_secs(), None);
    }

    #[test]
    fn moves_once_the_pc_has_had_no_input_for_the_interval() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(every(60, Motion::Virtual));
        e.set_enabled(true);
        e.tick(0, 59_000);
        assert_eq!(calls(&m), 0);
        assert_eq!(e.next_move_in_secs(), Some(1));
        e.tick(0, 60_000);
        assert_eq!(calls(&m), 1);
        assert_eq!(e.next_move_in_secs(), Some(60));
    }

    #[test]
    fn real_input_restarts_the_countdown() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(every(60, Motion::Virtual));
        e.set_enabled(true);
        e.tick(0, 30_000);
        assert_eq!(e.next_move_in_secs(), Some(30));
        e.tick(30_000, 30_000); // the user touched the mouse at 30 s
        assert_eq!(e.next_move_in_secs(), Some(60));
        e.tick(30_000, 89_000);
        assert_eq!(calls(&m), 0, "moved before a full minute without the user");
        e.tick(30_000, 90_000);
        assert_eq!(calls(&m), 1);
    }

    #[test]
    fn each_move_traces_a_whole_square_and_ends_where_it_started() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(every(5, Motion::Square));
        e.set_enabled(true);
        e.tick(0, 5_000);
        assert_eq!(calls(&m), 1);
        assert_eq!(m.moves.lock().unwrap().len(), 20, "one trigger, one whole square");
        assert_eq!(m.net_move(), (0, 0));
    }

    /// UIPI: `SendInput` reports success, but the idle clock never moves (C7). SC-006.
    #[test]
    fn a_move_windows_discards_is_retried_once_per_interval_not_every_tick() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(every(60, Motion::Virtual));
        e.set_enabled(true);
        e.tick(0, 60_000);
        for t in 61..120 {
            e.tick(0, t * 1000);
        }
        assert_eq!(calls(&m), 1, "retried inside the interval");
        assert!(e.blocked, "a discarded move must be reported");
        e.tick(0, 120_000);
        assert_eq!(calls(&m), 2);
    }

    /// `SendInput` refusing outright (a desktop we cannot reach). SC-006.
    #[test]
    fn a_failing_injector_is_retried_once_per_interval_not_every_tick() {
        let m = MockInjector::default();
        m.fail.store(true, Ordering::SeqCst);
        let mut e = engine(&m);
        e.set_settings(every(60, Motion::Square));
        e.set_enabled(true);
        e.tick(0, 60_000);
        for t in 61..120 {
            e.tick(0, t * 1000);
        }
        assert_eq!(calls(&m), 1);
        assert!(e.blocked);
        e.tick(0, 120_000);
        assert_eq!(calls(&m), 2);
    }

    #[test]
    fn our_own_path_is_not_the_user_coming_back() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(every(5, Motion::Square));
        e.set_enabled(true);
        e.tick(0, 300_000); // 20 steps × 10 ms → the path ends at 300_200
        e.tick(300_200, 301_000); // Windows' last input is the path's last step
        assert!(!e.blocked);
        assert!(e.human_idle_ms >= 300_000, "the path reset the human clock");
    }

    #[test]
    fn input_after_the_path_is_the_user() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(every(5, Motion::Square));
        e.set_enabled(true);
        e.tick(0, 300_000);
        e.tick(300_200, 301_000);
        e.tick(301_000, 301_500); // 800 ms after the path ended, that is a person
        assert_eq!(e.human_idle_ms, 500);
    }

    /// Spec 005 FR-007.
    #[test]
    fn test_move_works_while_stopped_and_restarts_the_countdown() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(every(60, Motion::Square));
        e.move_now(1_000);
        assert_eq!(calls(&m), 1);
        assert_eq!(m.net_move(), (0, 0));
        e.set_enabled(true);
        e.tick(1_200, 2_000);
        assert_eq!(e.next_move_in_secs(), Some(60));
        assert_eq!(calls(&m), 1);
    }

    #[test]
    fn variation_is_drawn_once_per_cycle_so_the_countdown_is_steady() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            vary_pct: 50,
            ..every(60, Motion::Virtual)
        });
        e.set_enabled(true);
        e.tick(0, 60_000); // the first cycle is exactly the interval
        assert_eq!(calls(&m), 1);
        e.tick(60_000, 61_000);
        let a = e.next_move_in_secs().unwrap();
        e.tick(60_000, 62_000);
        let b = e.next_move_in_secs().unwrap();
        assert_eq!(a - b, 1, "the countdown jumped: {a} then {b}");
    }

    #[test]
    fn settings_round_trip_and_clamp() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            interval_secs: 1,
            key: 0x7E,
            motion: Motion::Circle,
            distance_px: 9_999,
            vary_pct: 90,
        });
        assert_eq!(
            e.settings(),
            InputSettings {
                interval_secs: 5,
                key: 0x7E,
                motion: Motion::Circle,
                distance_px: 500,
                vary_pct: 50
            }
        );
        e.set_settings(every(u32::MAX, Motion::Square));
        assert_eq!(e.settings().interval_secs, 3_600);
    }

    #[test]
    fn invisible_never_moves_the_cursor() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            distance_px: 500,
            ..every(5, Motion::Virtual)
        });
        e.set_enabled(true);
        e.tick(0, 5_000);
        assert_eq!(calls(&m), 1);
        assert!(m.moves.lock().unwrap().is_empty(), "Invisible moved the cursor");
    }

    #[test]
    fn a_key_wins_over_the_motion() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            key: 0x7E,
            ..every(5, Motion::Square)
        });
        e.set_enabled(true);
        e.tick(0, 5_000);
        assert_eq!(calls(&m), 1);
        assert!(m.moves.lock().unwrap().is_empty());
    }

    #[test]
    fn survives_the_49_day_tick_wrap() {
        let m = MockInjector::default();
        let start = u32::MAX - 30_000;
        let mut e = InputEngine::new(Arc::new(m.clone()), start);
        e.set_settings(every(60, Motion::Virtual));
        e.set_enabled(true);
        e.tick(start, start);
        e.tick(start, start.wrapping_add(60_000));
        assert_eq!(calls(&m), 1);
        let moved = start.wrapping_add(60_000);
        e.tick(moved, moved.wrapping_add(1_000));
        assert!(!e.blocked);
        assert_eq!(e.next_move_in_secs(), Some(59));
    }

    #[test]
    fn disabling_clears_the_countdown_and_the_warning() {
        let m = MockInjector::default();
        m.fail.store(true, Ordering::SeqCst);
        let mut e = engine(&m);
        e.set_settings(every(5, Motion::Virtual));
        e.set_enabled(true);
        e.tick(0, 5_000);
        assert!(e.blocked);
        e.set_enabled(false);
        assert!(!e.blocked);
        assert_eq!(e.next_move_in_secs(), None);
    }
}
```

- [ ] **Step 4: Run them and confirm they fail.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib core::`
Expected: compile errors (`in_span`, `move_now`, `next_move_in_secs`, and a `note_injection`
arity mismatch).

- [ ] **Step 5: Implement `idle.rs`.** Replace the `injected_at` field, `note_injection` and
`observe` with the following, and add `in_span` as a free function:

```rust
/// `tick` lies within `[start − TOL, end + TOL]`. Wrapping arithmetic, so the 49-day wrap of
/// `GetTickCount` cannot split a span.
pub fn in_span(tick: u32, start: u32, end: u32) -> bool {
    let lo = start.wrapping_sub(TOLERANCE_MS);
    tick.wrapping_sub(lo) <= end.wrapping_sub(start).wrapping_add(2 * TOLERANCE_MS)
}

pub struct IdleTracker {
    /// Tick of the last input believed to be human (our injections filtered out).
    human_last_input: u32,
    /// Start and end tick of our most recent injection. A path takes a few hundred ms, and every
    /// step of it is ours (spec 005 FR-006).
    injected: Option<(u32, u32)>,
}

impl IdleTracker {
    pub fn new(now: u32) -> Self {
        Self {
            human_last_input: now,
            injected: None,
        }
    }

    /// Record that we injected input from `start` to `end`.
    pub fn note_injection(&mut self, start: u32, end: u32) {
        self.injected = Some((start, end));
    }

    /// Observe the OS's last-input tick (`GetLastInputInfo.dwTime`). If it falls inside our last
    /// injection, it was us, leave the human clock alone. Otherwise a real human moved.
    pub fn observe(&mut self, system_last_input: u32) {
        let is_ours = self
            .injected
            .is_some_and(|(s, e)| in_span(system_last_input, s, e));
        if !is_ours {
            self.human_last_input = system_last_input;
        }
    }
```

`system_idle_ms`, `human_idle_ms`, `clamp` and `SANITY_MAX_MS` stay unchanged.
`wrapping_abs_diff` is now unused, so delete it.

- [ ] **Step 6: Implement `input_engine.rs`.** Replace everything above `#[cfg(test)]` with:

```rust
//! The input engine (FEATURES Part C). Enabled only while running with *Move the mouse* on
//! (spec 005 FR-003). It moves once the PC has had no input for the interval, traces a whole
//! closed path each time (FR-005), never mistakes its own path for the user (FR-006), and
//! reports when Windows silently discards the input (C7).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::idle::{self, IdleTracker};
use crate::core::motion::{self, Motion};
use crate::platform::InputInjector;

/// The user-settable input-engine knobs. Clamped in `InputEngine::set_settings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSettings {
    /// Move once the PC has had no input for this many seconds.
    pub interval_secs: u32,
    /// 0 = move the mouse; otherwise press this virtual-key code instead.
    pub key: u16,
    /// What the cursor does (C2). `Virtual` moves nothing visible.
    #[serde(default)]
    pub motion: Motion,
    /// Pixels per leg of a visible motion. Ignored by `Motion::Virtual`.
    #[serde(default = "default_distance")]
    pub distance_px: u16,
    /// Vary the interval and distance by up to this percent (C5). Purpose: a fixed interval
    /// synchronises badly with other periodic events, and a cursor that always lands on the same
    /// pixel eventually lands somewhere it should not. Nothing to do with looking human.
    #[serde(default)]
    pub vary_pct: u8,
}

fn default_distance() -> u16 {
    10
}

impl Default for InputSettings {
    /// A visible 10 px square after a minute with no input, what someone arriving from Move
    /// Mouse expects to see (spec 005; FEATURES Part C, amended).
    fn default() -> Self {
        Self {
            interval_secs: 60,
            key: 0,
            motion: Motion::Square,
            distance_px: default_distance(),
            vary_pct: 0,
        }
    }
}

pub struct InputEngine {
    injector: Arc<dyn InputInjector>,
    tracker: IdleTracker,
    enabled: bool,
    pub blocked: bool,
    pub system_idle_ms: u32,
    pub human_idle_ms: u32,
    interval_ms: u32,
    key: u16,
    motion: Motion,
    distance_px: u16,
    vary_pct: u8,
    /// This cycle's interval: `interval_ms` varied once, when the cycle starts, so the countdown
    /// counts down instead of jumping about from tick to tick.
    cycle_ms: u32,
    /// When the last move ended. Set on a failed attempt too, so nothing retries every second.
    last_move: Option<u32>,
    /// The last move's span, for the next tick to check that it reset the idle clock (C7).
    pending_verify: Option<(u32, u32)>,
    next_move_in_ms: Option<u32>,
}

impl InputEngine {
    pub fn new(injector: Arc<dyn InputInjector>, now: u32) -> Self {
        let mut e = Self {
            injector,
            tracker: IdleTracker::new(now),
            enabled: false,
            blocked: false,
            system_idle_ms: 0,
            human_idle_ms: 0,
            interval_ms: 0,
            key: 0,
            motion: Motion::Virtual,
            distance_px: 0,
            vary_pct: 0,
            cycle_ms: 0,
            last_move: None,
            pending_verify: None,
            next_move_in_ms: None,
        };
        e.set_settings(InputSettings::default());
        e
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        // Pressing Start is itself input, so a whole cycle is what is left.
        self.next_move_in_ms = on.then_some(self.cycle_ms);
        if !on {
            self.blocked = false;
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Clamped here so a bad config or a typo cannot produce a runaway injector: interval
    /// 5 s–1 h, distance 1–500 px, variation at most 50 %.
    pub fn set_settings(&mut self, s: InputSettings) {
        self.interval_ms = s.interval_secs.clamp(5, 3_600) * 1000;
        self.key = s.key;
        self.motion = s.motion;
        // A visible move is capped: this resets an idle timer, it does not fling the pointer.
        self.distance_px = s.distance_px.clamp(1, 500);
        self.vary_pct = s.vary_pct.min(50);
        self.cycle_ms = self.interval_ms;
    }

    pub fn settings(&self) -> InputSettings {
        InputSettings {
            interval_secs: self.interval_ms / 1000,
            key: self.key,
            motion: self.motion,
            distance_px: self.distance_px,
            vary_pct: self.vary_pct,
        }
    }

    /// Seconds until the next move, rounded up. `None` while disabled.
    pub fn next_move_in_secs(&self) -> Option<u32> {
        self.next_move_in_ms.map(|ms| ms.div_ceil(1000))
    }

    /// What actually gets synthesized: a key if one is set, else the motion. Returns how long it
    /// took, so the whole span can be recognised as ours.
    fn dispatch(&self, now: u32) -> crate::platform::Result<u32> {
        if self.key != 0 {
            return self.injector.key(self.key).map(|()| 0);
        }
        if self.motion == Motion::Virtual {
            return self.injector.virtual_jiggle().map(|()| 0);
        }
        let distance = motion::vary(self.distance_px as u32, self.vary_pct as u32, now) as i32;
        self.injector
            .move_path(&motion::path(self.motion, distance), motion::STEP_MS)
    }

    /// One move, right now: the scheduled one, or Test (spec 005 FR-007).
    pub fn move_now(&mut self, now: u32) {
        match self.dispatch(now) {
            Ok(elapsed) => {
                let end = now.wrapping_add(elapsed);
                self.tracker.note_injection(now, end);
                self.pending_verify = Some((now, end));
                self.last_move = Some(end);
            }
            Err(e) => {
                self.blocked = true;
                self.last_move = Some(now);
                tracing::warn!("injection failed: {e}");
            }
        }
        // C5: vary once per cycle, so the countdown is steady. Seeded from the tick, no RNG state.
        self.cycle_ms = motion::vary(self.interval_ms, self.vary_pct as u32, now);
    }

    /// One tick. `last_input_tick` = `GetLastInputInfo.dwTime`, `now` = `GetTickCount` (same domain).
    pub fn tick(&mut self, last_input_tick: u32, now: u32) {
        // C7: did the last move actually reset the idle clock? If Windows' last-input tick is not
        // inside our span, the input was silently discarded (UIPI).
        if let Some((start, end)) = self.pending_verify.take() {
            self.blocked = !idle::in_span(last_input_tick, start, end);
        }
        self.tracker.observe(last_input_tick);
        self.system_idle_ms = self.tracker.system_idle_ms(last_input_tick, now);
        self.human_idle_ms = self.tracker.human_idle_ms(now);

        if !self.enabled {
            self.next_move_in_ms = None;
            return;
        }
        // Quiet = time since anything happened: real input, or our own last move (spec 005
        // FR-004). Counting our last move as well means a move Windows silently discarded is
        // retried once per interval, not every second (SC-006).
        let since_move = self.last_move.map_or(u32::MAX, |t| now.wrapping_sub(t));
        let quiet = self.system_idle_ms.min(since_move);
        if quiet < self.cycle_ms {
            self.next_move_in_ms = Some(self.cycle_ms - quiet);
            return;
        }
        self.move_now(now);
        self.next_move_in_ms = Some(self.cycle_ms);
    }
}
```

- [ ] **Step 7: Run all tests and confirm they pass.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: everything PASSES. `lib.rs` still compiles because `tick`, `enabled`, `blocked`,
`settings` and `set_settings` keep their signatures.

- [ ] **Step 8: Run clippy and confirm it is clean.**
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit.**

```bash
git add src-tauri/src/core/idle.rs src-tauri/src/core/input_engine.rs src-tauri/src/platform
git commit -m "feat(M7): move after N s with no input; whole path is ours; Test (FR-004/006/007)

Retries a discarded or refused move once per interval, not every tick (SC-006).
Variation is drawn once per cycle so the countdown is steady. idle_threshold
is gone: 'after N seconds with no input' is the one knob.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `core::running`, the one Start/Stop invariant

**Files:**
- Create: `src-tauri/src/core/running.rs`
- Modify: `src-tauri/src/core/mod.rs` (add `pub mod running;`)

**Interfaces:**
- Consumes: `Engine::{manual, set_manual}`, `InputEngine::{set_enabled, enabled}` (Task 2).
- Produces: `RunSettings { move_mouse: bool, keep_screen_on: bool, start_on_launch: bool }`
  (`Default` = true/true/false, `#[serde(default)]`) · `RunSettings::start_mode(&self) -> WakeMode` ·
  `is_running(&Engine) -> bool` ·
  `apply(&mut Engine, &mut InputEngine, &RunSettings, WakeMode)` ·
  `set_running(&mut Engine, &mut InputEngine, &RunSettings, bool)` ·
  `enum StatusKind { Stopped, StoppedButRuleHolds, Running, RunningBlocked, RunningPowerOnly }`
  (serde `snake_case`) · `status_kind(running: bool, move_mouse: bool, blocked: bool, effective: WakeMode) -> StatusKind`.

- [ ] **Step 1: Write the module with its tests and stub bodies.** Create
`src-tauri/src/core/running.rs`:

```rust
//! Start/Stop (spec 005 FR-003). One running state over the two engines, which stay separate
//! (constitution I): Start holds a power mode and, unless *Move the mouse* is off, enables the
//! input engine. Everything that starts or stops, window, tray, hotkey, CLI, goes through here.

use serde::{Deserialize, Serialize};

use crate::core::engine::Engine;
use crate::core::input_engine::InputEngine;
use crate::core::modes::WakeMode;

/// What Start means. Saved as `run` in config v3. Whether it is running is deliberately not
/// saved: `start_on_launch` is the explicit way to come back running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunSettings {
    /// Synthesize input while running. Off = a power-only Start.
    pub move_mouse: bool,
    /// Hold the display on as well as the system.
    pub keep_screen_on: bool,
    /// Start as soon as the app opens.
    pub start_on_launch: bool,
}

impl Default for RunSettings {
    fn default() -> Self {
        todo!()
    }
}

impl RunSettings {
    /// The power mode Start holds.
    pub fn start_mode(&self) -> WakeMode {
        todo!()
    }
}

/// Running ⇔ the manual mode is not Off.
pub fn is_running(engine: &Engine) -> bool {
    todo!()
}

/// Set the manual mode and bring the input engine into line: on ⇔ running ∧ Move the mouse.
pub fn apply(engine: &mut Engine, input: &mut InputEngine, settings: &RunSettings, mode: WakeMode) {
    todo!()
}

/// Start or stop with what Start means right now. Calling it with the current state re-applies
/// changed settings without changing whether it runs.
pub fn set_running(engine: &mut Engine, input: &mut InputEngine, settings: &RunSettings, on: bool) {
    todo!()
}

/// What Home says (spec 005 FR-010). The words live in the UI; the decision lives here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    Stopped,
    /// Stopped, but a rule (Advanced) holds power anyway.
    StoppedButRuleHolds,
    Running,
    /// Running, but Windows discarded or refused the last move.
    RunningBlocked,
    /// Running with Move the mouse off.
    RunningPowerOnly,
}

pub fn status_kind(running: bool, move_mouse: bool, blocked: bool, effective: WakeMode) -> StatusKind {
    todo!()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::platform::mock::{MockInjector, MockPowerGuard};

    fn engines() -> (Engine, InputEngine) {
        (
            Engine::new(Arc::new(MockPowerGuard::default())),
            InputEngine::new(Arc::new(MockInjector::default()), 0),
        )
    }

    #[test]
    fn defaults_move_the_mouse_and_keep_the_screen_on_but_do_not_autostart() {
        let s = RunSettings::default();
        assert!(s.move_mouse && s.keep_screen_on && !s.start_on_launch);
    }

    #[test]
    fn start_holds_power_and_enables_moves() {
        let (mut e, mut i) = engines();
        set_running(&mut e, &mut i, &RunSettings::default(), true);
        assert!(is_running(&e));
        assert_eq!(e.manual(), WakeMode::KeepPresenting);
        assert!(i.enabled());
    }

    #[test]
    fn stop_releases_both() {
        let (mut e, mut i) = engines();
        let s = RunSettings::default();
        set_running(&mut e, &mut i, &s, true);
        set_running(&mut e, &mut i, &s, false);
        assert!(!is_running(&e));
        assert_eq!(e.manual(), WakeMode::Off);
        assert!(!i.enabled());
    }

    #[test]
    fn move_the_mouse_off_is_a_power_only_start() {
        let (mut e, mut i) = engines();
        let s = RunSettings { move_mouse: false, ..RunSettings::default() };
        set_running(&mut e, &mut i, &s, true);
        assert!(is_running(&e));
        assert!(!i.enabled());
    }

    #[test]
    fn keep_the_screen_on_off_starts_keep_running() {
        let (mut e, mut i) = engines();
        let s = RunSettings { keep_screen_on: false, ..RunSettings::default() };
        set_running(&mut e, &mut i, &s, true);
        assert_eq!(e.manual(), WakeMode::KeepRunning);
    }

    /// Review Focus 4.
    #[test]
    fn new_settings_take_effect_while_running_and_keep_it_running() {
        let (mut e, mut i) = engines();
        let mut s = RunSettings::default();
        set_running(&mut e, &mut i, &s, true);
        s.move_mouse = false;
        let on = is_running(&e);
        set_running(&mut e, &mut i, &s, on);
        assert!(is_running(&e));
        assert!(!i.enabled());
        s.keep_screen_on = false;
        let on = is_running(&e);
        set_running(&mut e, &mut i, &s, on);
        assert_eq!(e.manual(), WakeMode::KeepRunning);
    }

    #[test]
    fn new_settings_do_not_start_a_stopped_app() {
        let (mut e, mut i) = engines();
        let s = RunSettings { keep_screen_on: false, ..RunSettings::default() };
        let on = is_running(&e);
        set_running(&mut e, &mut i, &s, on);
        assert!(!is_running(&e));
        assert!(!i.enabled());
    }

    /// `--keep running` from the CLI picks the mode; Move the mouse still decides the input.
    #[test]
    fn an_explicit_mode_still_follows_move_the_mouse() {
        let (mut e, mut i) = engines();
        apply(&mut e, &mut i, &RunSettings::default(), WakeMode::KeepRunning);
        assert!(i.enabled());
        let off = RunSettings { move_mouse: false, ..RunSettings::default() };
        apply(&mut e, &mut i, &off, WakeMode::KeepRunning);
        assert!(!i.enabled());
        apply(&mut e, &mut i, &RunSettings::default(), WakeMode::Off);
        assert!(!i.enabled());
    }

    #[test]
    fn status_says_what_is_true() {
        use StatusKind::*;
        use WakeMode::*;
        assert_eq!(status_kind(false, true, false, Off), Stopped);
        assert_eq!(status_kind(false, true, false, KeepRunning), StoppedButRuleHolds);
        assert_eq!(status_kind(true, true, false, KeepPresenting), Running);
        assert_eq!(status_kind(true, true, true, KeepPresenting), RunningBlocked);
        assert_eq!(status_kind(true, false, false, KeepPresenting), RunningPowerOnly);
        // A stale `blocked` flag means nothing once moves are off.
        assert_eq!(status_kind(true, false, true, KeepPresenting), RunningPowerOnly);
    }
}
```

Add `pub mod running;` to `src-tauri/src/core/mod.rs`, after `pub mod rule;`.

- [ ] **Step 2: Run the tests and confirm they fail.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml running`
Expected: FAIL, panicking at `not yet implemented`.

- [ ] **Step 3: Replace the `todo!()` bodies.**

```rust
impl Default for RunSettings {
    fn default() -> Self {
        Self {
            move_mouse: true,
            keep_screen_on: true,
            start_on_launch: false,
        }
    }
}

impl RunSettings {
    /// The power mode Start holds.
    pub fn start_mode(&self) -> WakeMode {
        if self.keep_screen_on {
            WakeMode::KeepPresenting
        } else {
            WakeMode::KeepRunning
        }
    }
}

pub fn is_running(engine: &Engine) -> bool {
    engine.manual() != WakeMode::Off
}

pub fn apply(engine: &mut Engine, input: &mut InputEngine, settings: &RunSettings, mode: WakeMode) {
    engine.set_manual(mode);
    input.set_enabled(mode != WakeMode::Off && settings.move_mouse);
}

pub fn set_running(engine: &mut Engine, input: &mut InputEngine, settings: &RunSettings, on: bool) {
    let mode = if on { settings.start_mode() } else { WakeMode::Off };
    apply(engine, input, settings, mode);
}

pub fn status_kind(running: bool, move_mouse: bool, blocked: bool, effective: WakeMode) -> StatusKind {
    match (running, move_mouse, blocked) {
        (false, _, _) if effective != WakeMode::Off => StatusKind::StoppedButRuleHolds,
        (false, _, _) => StatusKind::Stopped,
        (true, false, _) => StatusKind::RunningPowerOnly,
        (true, true, true) => StatusKind::RunningBlocked,
        (true, true, false) => StatusKind::Running,
    }
}
```

- [ ] **Step 4: Run the tests and confirm they pass.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml running`
Expected: 9 PASS. Then run `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`.
Nothing outside the tests uses `running` yet, so dead-code warnings are expected. Add
`#![allow(dead_code)] // wired in Task 4/5` as the first line after the module doc, and delete
that line again in Task 5, step 5.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/core/running.rs src-tauri/src/core/mod.rs
git commit -m "feat(M7): core::running: one Start/Stop invariant over both engines (FR-003)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Config v3 and startup wiring

**Files:**
- Modify: `src-tauri/src/config/model.rs` (v3 shape)
- Rewrite: `src-tauri/src/config/migrate.rs`
- Modify: `src-tauri/src/config/store.rs:104-199` (tests)
- Modify: `src-tauri/src/lib.rs` (load, `SharedRun`, startup apply, `persist_current`)

**Interfaces:**
- Consumes: `RunSettings`, `running::apply` (Task 3), `InputSettings::default()` (Task 2).
- Produces: `Config { schema_version, profiles, active_profile, input: InputSettings, run: RunSettings, auto_update }`,
  `CURRENT_SCHEMA_VERSION = 3`, and in lib.rs
  `pub(crate) type SharedRun = Arc<Mutex<RunSettings>>` (managed state). `SharedEngine`,
  `SharedInput` and `SharedProfiles` become `pub(crate)`.

- [ ] **Step 1: Write the failing migration tests.** Replace the `tests` module in
`config/migrate.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::motion::Motion;
    use crate::core::running::RunSettings;

    fn v2_input(enabled: bool) -> Value {
        json!({
            "schema_version": 2,
            "mode": "KeepRunning",
            "input_enabled": enabled,
            "input": {
                "interval_secs": 200, "idle_threshold_secs": 30, "key": 0,
                "motion": "Line", "distance_px": 25, "vary_pct": 10
            },
            "profiles": [{ "id": "a", "name": "A", "rules": [] }],
            "active_profile": "a",
            "auto_update": false
        })
    }

    #[test]
    fn migrates_legacy_unversioned() {
        let cfg = migrate(json!({ "mode": "KeepRunning" })).unwrap();
        assert_eq!(cfg.schema_version, 3);
        assert_eq!(cfg.input, InputSettings::default());
        assert_eq!(cfg.run, RunSettings::default());
    }

    #[test]
    fn migrates_v1() {
        let cfg = migrate(json!({ "schema_version": 1, "mode": "KeepPresenting" })).unwrap();
        assert_eq!(cfg.schema_version, 3);
        assert!(cfg.profiles.is_empty());
    }

    /// SC-007: input never switched on → nobody chose those settings; take v3's visible defaults.
    #[test]
    fn v2_with_input_never_enabled_gets_the_v3_defaults() {
        let cfg = migrate(v2_input(false)).unwrap();
        assert_eq!(cfg.input, InputSettings::default());
        assert!(cfg.run.move_mouse);
    }

    /// SC-007: someone who turned it on keeps what they set.
    #[test]
    fn v2_with_input_enabled_keeps_its_settings() {
        let cfg = migrate(v2_input(true)).unwrap();
        assert_eq!(
            cfg.input,
            InputSettings { interval_secs: 200, key: 0, motion: Motion::Line, distance_px: 25, vary_pct: 10 }
        );
    }

    #[test]
    fn v2_keeps_profiles_and_the_update_switch() {
        let cfg = migrate(v2_input(false)).unwrap();
        assert_eq!(cfg.active().map(|p| p.name.as_str()), Some("A"));
        assert!(!cfg.auto_update);
    }

    #[test]
    fn loads_current_version() {
        let v = json!({
            "schema_version": 3,
            "run": { "move_mouse": false, "keep_screen_on": false, "start_on_launch": true }
        });
        let cfg = migrate(v).unwrap();
        assert_eq!(
            cfg.run,
            RunSettings { move_mouse: false, keep_screen_on: false, start_on_launch: true }
        );
    }

    #[test]
    fn a_partial_run_block_fills_in_defaults() {
        let cfg = migrate(json!({ "schema_version": 3, "run": { "start_on_launch": true } })).unwrap();
        assert!(cfg.run.start_on_launch && cfg.run.move_mouse && cfg.run.keep_screen_on);
    }

    #[test]
    fn rejects_newer_version() {
        assert!(migrate(json!({ "schema_version": 999 })).is_err());
    }

    /// Review Focus 1: indexing a non-object panics, and the release build aborts on panic.
    #[test]
    fn rejects_a_config_that_is_not_an_object() {
        for v in [json!([1]), json!(5), json!("x"), Value::Null] {
            assert!(migrate(v.clone()).is_err(), "{v} was accepted");
        }
    }
}
```

- [ ] **Step 2: Run them and confirm they fail.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml migrate`
Expected: compile errors (`cfg.run`, and `Value`/`json` not in scope).

- [ ] **Step 3: Implement.** Replace the body of `config/model.rs` above `impl Config`:

```rust
//! Persisted config. Versioned, with `#[serde(default)]` on every field so a config written by an
//! older build still deserializes (FEATURES D8). v2 added profiles; v3 (spec 005) replaced the
//! saved mode and the input switch with Start/Stop and its settings.

use serde::{Deserialize, Serialize};

use crate::core::input_engine::InputSettings;
use crate::core::rule::Profile;
use crate::core::running::RunSettings;

pub const CURRENT_SCHEMA_VERSION: u32 = 3;

fn default_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub schema_version: u32,
    /// User-defined profiles (rule sets). Empty on a fresh install.
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Id of the active profile, or empty for none.
    #[serde(default)]
    pub active_profile: String,
    /// How the mouse moves while running.
    #[serde(default)]
    pub input: InputSettings,
    /// What Start means.
    #[serde(default)]
    pub run: RunSettings,
    /// Check for updates in the background (UPDATES.md §6). Default on, and a switch, because
    /// some people run this on machines where outbound requests get noticed.
    #[serde(default = "default_true")]
    pub auto_update: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            profiles: Vec::new(),
            active_profile: String::new(),
            input: InputSettings::default(),
            run: RunSettings::default(),
            auto_update: true,
        }
    }
}
```

In `impl Config`, delete `with_mode` and keep `active`. Replace the code above `#[cfg(test)]`
in `config/migrate.rs`:

```rust
//! Versioned migration chain (FEATURES D8). A parse failure here is surfaced, never silently
//! reset.

use serde_json::{json, Value};

use super::model::{Config, CURRENT_SCHEMA_VERSION};
use crate::core::input_engine::InputSettings;

/// Migrate a parsed JSON value up to the current `Config`.
/// - v0 (unversioned), v1 (bare `mode`), v2 (profiles + the input switch) → v3.
/// - v3 → deserialize directly.
/// - newer than current → error (a newer build wrote it; do not silently downgrade or lose data).
pub fn migrate(mut value: Value) -> Result<Config, String> {
    // Indexing anything else panics, and a release build aborts on panic (Review Focus 1).
    if !value.is_object() {
        return Err("config is not a JSON object".into());
    }
    let version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;

    match version {
        0..=2 => {
            // v3 stopped saving whether it was running (`mode`) and replaced the input switch
            // with Start. If that switch was never on, nobody chose the input settings, they
            // are v2's invisible defaults, so v3's visible ones replace them. Someone who did
            // turn it on keeps what they set (spec 005 FR-014).
            let input_was_on = value
                .get("input_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !input_was_on {
                value["input"] =
                    serde_json::to_value(InputSettings::default()).map_err(|e| e.to_string())?;
            }
            if let Some(obj) = value.as_object_mut() {
                obj.remove("mode");
                obj.remove("input_enabled");
            }
            value["schema_version"] = json!(CURRENT_SCHEMA_VERSION);
            serde_json::from_value(value)
                .map_err(|e| format!("migrate v{version}→v{CURRENT_SCHEMA_VERSION}: {e}"))
        }
        3 => serde_json::from_value(value).map_err(|e| format!("parse v3: {e}")),
        v => Err(format!(
            "config schema v{v} is newer than supported v{CURRENT_SCHEMA_VERSION}; refusing to load"
        )),
    }
}
```

In `config/store.rs` tests, delete `use crate::core::modes::WakeMode;`, add
`use crate::core::running::RunSettings;`, and replace three tests:

```rust
    #[test]
    fn a_utf8_bom_does_not_make_a_config_corrupt() {
        let path = temp_path();
        let json = r#"{"schema_version":3,"run":{"start_on_launch":true}}"#;
        std::fs::write(&path, format!("\u{FEFF}{json}")).unwrap();
        let cfg = load(&path).expect("a BOM is an encoding artefact, not corruption");
        assert!(cfg.run.start_on_launch);
        let _ = std::fs::remove_file(&path);
    }
```

```rust
    #[test]
    fn round_trips() {
        let path = temp_path();
        let cfg = Config {
            run: RunSettings { start_on_launch: true, ..RunSettings::default() },
            ..Config::default()
        };
        save_atomic(&path, &cfg).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, cfg);
        // no temp left behind
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_file(&path);
    }
```

```rust
    #[test]
    fn save_replaces_existing_atomically() {
        let path = temp_path();
        save_atomic(&path, &Config::default()).unwrap();
        save_atomic(&path, &Config { auto_update: false, ..Config::default() }).unwrap();
        assert!(!load(&path).unwrap().auto_update);
        let _ = std::fs::remove_file(&path);
    }
```

Add a store test for Review Focus 1, end to end:

```rust
    #[test]
    fn a_json_array_is_corrupt_not_a_crash() {
        let path = temp_path();
        std::fs::write(&path, b"[]").unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
        let _ = std::fs::remove_file(&path);
    }
```

Now wire it in `lib.rs`:

1. Imports: add `use crate::core::running::{self, RunSettings};`.
2. Make the aliases crate-visible and add one:

```rust
pub(crate) type SharedEngine = Arc<Mutex<Engine>>;
pub(crate) type SharedInput = Arc<Mutex<InputEngine>>;
/// Every profile on disk. The engine holds one of them; this is the set (research R3).
pub(crate) type SharedProfiles = Arc<Mutex<Vec<Profile>>>;
/// What Start means (spec 005).
pub(crate) type SharedRun = Arc<Mutex<RunSettings>>;
```

3. In `persist_current`, replace everything from `let engine = …` to the `let cfg = Config { … };`
with:

```rust
    let profile = app.state::<SharedEngine>().lock().unwrap().profile().clone();
    let input = app.state::<SharedInput>().lock().unwrap().settings();
    let run = *app.state::<SharedRun>().lock().unwrap();
    let p = app.state::<Mutex<Persist>>();
    let p = p.lock().unwrap();
    if !p.enabled {
        return;
    }
    // Merge the live profile into the stored collection. Writing `vec![profile]` here, as this
    // did until 2026-08-28, destroyed every other profile on the next save (research R3).
    let all = match app.try_state::<SharedProfiles>() {
        Some(state) => {
            let mut list = state.lock().unwrap();
            profiles::upsert(&mut list, profile.clone());
            list.clone()
        }
        None => vec![profile.clone()],
    };
    let cfg = Config {
        active_profile: profile.id.clone(),
        profiles: all,
        input,
        run,
        auto_update: auto_update_enabled(),
        ..Config::default()
    };
```

4. In `run()`, replace the `let ( initial_mode, … ) = match store::load(…) { … };` block and
everything down to (and including) `let input_engine: SharedInput = Arc::new(Mutex::new(input_engine));`
with:

```rust
    let (initial_profile, initial_input, run_settings, stored, save_enabled) =
        match store::load(&cfg_path) {
            Ok(c) => (c.active().cloned(), c.input, c.run, c.profiles.clone(), true),
            Err(e) => {
                tracing::error!("config load failed ({e}); starting stopped and preserving the file");
                (None, InputSettings::default(), RunSettings::default(), Vec::new(), false)
            }
        };

    let mut engine = Engine::new(power);
    if let Some(p) = initial_profile {
        engine.set_profile(p);
    }
    if let Some(profile) = profile_from_args() {
        tracing::info!("CLI --while-process overrides the active profile for this session");
        engine.set_profile(profile);
    }
    let mut input_engine = InputEngine::new(platform.input.clone(), platform::tick_now());
    input_engine.set_settings(initial_input);

    // Spec 005 FR-014: the running state is not saved. `--keep` picks a mode for this launch;
    // otherwise Start on launch decides.
    let initial_mode = match cli_keep_mode() {
        Some(m) => m,
        None if run_settings.start_on_launch => run_settings.start_mode(),
        None => WakeMode::Off,
    };
    running::apply(&mut engine, &mut input_engine, &run_settings, initial_mode);

    let engine: SharedEngine = Arc::new(Mutex::new(engine));
    // The engine always holds a profile, so the collection is never empty.
    let mut stored = stored;
    profiles::upsert(&mut stored, engine.lock().unwrap().profile().clone());
    let stored_profiles: SharedProfiles = Arc::new(Mutex::new(stored));
    let input_engine: SharedInput = Arc::new(Mutex::new(input_engine));
    let run_state: SharedRun = Arc::new(Mutex::new(run_settings));
```

5. Add `.manage(run_state.clone())` after `.manage(stored_profiles.clone())`.
6. In `setup`, `let restored = engine.lock().unwrap().manual();` still works. Leave it until
Task 7.

- [ ] **Step 4: Run all tests and confirm they pass.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all PASS. `ipc::set_input_enabled` and `ipc::import_move_mouse` still compile.

- [ ] **Step 5: Run clippy and confirm it is clean.**
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: clean. If `RunSettings` items are reported dead, the Task 3 allow still covers them.

- [ ] **Step 6: Commit.**

```bash
git add src-tauri/src/config src-tauri/src/lib.rs
git commit -m "feat(M7): config v3: run settings, no saved mode; v2 input defaults reset (FR-014)

Also: a config that is JSON but not an object is now 'corrupt', not a panic
(release builds abort on panic, so it killed the app at launch).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Shell helpers and the IPC surface

**Files:**
- Modify: `src-tauri/src/lib.rs` (helpers, hotkey, forwarded args, handler list; remove
  first run and `set_manual`)
- Modify: `src-tauri/src/ipc/mod.rs` (add and remove commands)
- Modify: `src-tauri/src/core/engine.rs` (remove pause)
- Modify: `src-tauri/src/core/running.rs` (drop the temporary `allow`)

**Interfaces:**
- Consumes: `running::*` (Task 3), `SharedRun` (Task 4).
- Produces, in lib.rs: `pub(crate) fn apply_mode(&AppHandle, WakeMode)` ·
  `pub(crate) fn set_running(&AppHandle, bool)` · `pub(crate) fn is_running(&AppHandle) -> bool` ·
  `pub(crate) fn set_run_settings(&AppHandle, RunSettings)` ·
  `pub(crate) fn set_autostart(&AppHandle, bool) -> Result<bool, String>`.
- Produces, as IPC commands: `get_status -> Status { kind: StatusKind, running: bool, next_move_in_secs: Option<u32>, keep_screen_on: bool }` ·
  `start` · `stop` · `test_move` · `get_run_settings -> RunSettings` ·
  `set_run_settings(settings: RunSettings)` · `get_autostart -> bool` ·
  `set_autostart(enabled: bool) -> Result<bool, String>`.
- Removes these IPC commands: `get_state`, `set_mode`, `pause_all`, `resume_all`,
  `set_input_enabled`, `is_first_run`, `complete_first_run`.

The tests here are compile-time. These are Tauri glue functions over already-tested `core`
calls, and running them needs a live `AppHandle`, so they are exercised in Task 10.

- [ ] **Step 1: Remove pause from the engine.** In `core/engine.rs`, delete the `paused` field,
its initialiser, `set_paused` and `paused`. Replace `tick`'s body and doc comment with:

```rust
    /// Recompute desired = max(manual, rules) and reconcile. Idempotent across identical ticks.
    pub fn tick(&mut self, snap: &Snapshot) {
        let desired = self.manual.max(desired_mode(&self.profile, snap));
        if desired != self.last {
            tracing::info!(?desired, "reconciling wake mode");
            self.last = desired;
        }
        if let Err(e) = self.reconciler.reconcile(desired) {
            tracing::error!("reconcile failed: {e}");
        }
    }
```

- [ ] **Step 2: Add the lib.rs helpers.** Delete `FIRST_RUN`, `is_first_run`,
`clear_first_run`, the line `FIRST_RUN.store(!cfg_path.exists(), Ordering::SeqCst);` and
`set_manual`. Add, where `set_manual` was:

```rust
/// Every start and stop goes through here (spec 005 FR-003): set the manual mode and bring the
/// input engine into line, reconcile power now rather than on the next tick (so Stop is
/// immediate, SC-004), then tell the tray and any open window. Lock order: run → engine → input.
pub(crate) fn apply_mode(app: &tauri::AppHandle, mode: WakeMode) {
    let settings = *app.state::<SharedRun>().lock().unwrap();
    let snap = app.state::<Arc<Sampler>>().last();
    {
        let engine = app.state::<SharedEngine>();
        let input = app.state::<SharedInput>();
        let mut e = engine.lock().unwrap();
        let mut ie = input.lock().unwrap();
        running::apply(&mut e, &mut ie, &settings, mode);
        e.tick(&snap);
    }
    after_change(app);
}

/// Start or stop, using whatever Start means right now.
pub(crate) fn set_running(app: &tauri::AppHandle, on: bool) {
    let mode = if on {
        app.state::<SharedRun>().lock().unwrap().start_mode()
    } else {
        WakeMode::Off
    };
    apply_mode(app, mode);
}

pub(crate) fn is_running(app: &tauri::AppHandle) -> bool {
    running::is_running(&app.state::<SharedEngine>().lock().unwrap())
}

/// New settings take effect at once, running or not, and are saved.
pub(crate) fn set_run_settings(app: &tauri::AppHandle, settings: RunSettings) {
    *app.state::<SharedRun>().lock().unwrap() = settings;
    let on = is_running(app);
    set_running(app, on);
    persist_current(app);
}

/// Turn autostart on or off and report what actually took effect, keeping the tray's check item
/// and the Settings switch in step.
pub(crate) fn set_autostart(app: &tauri::AppHandle, on: bool) -> Result<bool, String> {
    let mgr = app.autolaunch();
    let res = if on { mgr.enable() } else { mgr.disable() };
    rebuild_tray_menu(app);
    res.map_err(|e| e.to_string())?;
    tracing::info!(enabled = on, "autostart set");
    Ok(mgr.is_enabled().unwrap_or(on))
}

/// Tell the tray and an open window that the state changed.
fn after_change(app: &tauri::AppHandle) {
    rebuild_tray_menu(app);
    if app.get_webview_window("main").is_some() {
        let _ = app.emit("state:changed", ());
    }
}
```

Replace `toggle_autostart`'s body with:

```rust
fn toggle_autostart(app: &tauri::AppHandle) {
    let now = app.autolaunch().is_enabled().unwrap_or(false);
    if let Err(e) = set_autostart(app, !now) {
        tracing::error!("autostart toggle failed: {e}");
    }
}
```

In `apply_forwarded`, change `set_manual(app, WakeMode::Off)` to `apply_mode(app, WakeMode::Off)`
and `set_manual(app, m)` to `apply_mode(app, m)`. Replace the global-shortcut handler body with:

```rust
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        set_running(app, !is_running(app));
                    }
                })
```

In the tray `on_menu_event`, change the three `set_manual(app, X)` calls to `apply_mode(app, X)`.
Task 7 replaces that menu.

- [ ] **Step 3: Rewrite the IPC surface.** In `ipc/mod.rs`:

  - Delete `mode_from_str`, `StateView`, `get_state`, `set_mode`, `pause_all`, `resume_all`,
    `is_first_run`, `complete_first_run` and `set_input_enabled`.
  - Change `use crate::core::rule::{Condition, Profile, Rule};` to
    `use crate::core::rule::{Profile, Rule};`.
  - Add these imports:
    ```rust
    use tauri_plugin_autostart::ManagerExt;
    use crate::core::running::{self, RunSettings, StatusKind};
    ```
  - Add the alias `type SharedRun = Arc<Mutex<RunSettings>>;`.
  - Add the new commands below `get_diagnostics`:

```rust
/// What Home shows (spec 005 FR-008/FR-010).
#[derive(Serialize)]
pub struct Status {
    pub kind: StatusKind,
    pub running: bool,
    /// Seconds to the next move; `None` when stopped or when Move the mouse is off.
    pub next_move_in_secs: Option<u32>,
    pub keep_screen_on: bool,
}

#[tauri::command]
pub fn get_status(
    engine: State<'_, SharedEngine>,
    input: State<'_, SharedInput>,
    run: State<'_, SharedRun>,
) -> Status {
    let settings = *run.lock().unwrap();
    let (on, effective) = {
        let e = engine.lock().unwrap();
        (running::is_running(&e), e.mode())
    };
    let (blocked, next_move_in_secs) = {
        let ie = input.lock().unwrap();
        (ie.enabled() && ie.blocked, ie.next_move_in_secs())
    };
    Status {
        kind: running::status_kind(on, settings.move_mouse, blocked, effective),
        running: on,
        next_move_in_secs,
        keep_screen_on: settings.keep_screen_on,
    }
}

#[tauri::command]
pub fn start(app: AppHandle) {
    crate::set_running(&app, true);
}

#[tauri::command]
pub fn stop(app: AppHandle) {
    crate::set_running(&app, false);
}

/// Test (spec 005 FR-007). On its own thread: a path takes a few hundred milliseconds, and a
/// synchronous command runs on the main thread, which would freeze the window that long.
#[tauri::command]
pub fn test_move(input: State<'_, SharedInput>) {
    let input = input.inner().clone();
    std::thread::spawn(move || input.lock().unwrap().move_now(platform::tick_now()));
}

#[tauri::command]
pub fn get_run_settings(run: State<'_, SharedRun>) -> RunSettings {
    *run.lock().unwrap()
}

#[tauri::command]
pub fn set_run_settings(app: AppHandle, settings: RunSettings) {
    crate::set_run_settings(&app, settings);
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    crate::set_autostart(&app, enabled)
}
```

  - Leave `import_move_mouse` as it is. It still compiles, and Task 6 rewrites it.

- [ ] **Step 4: Update the handler list.** In lib.rs `generate_handler![…]`, remove
`ipc::get_state`, `ipc::set_mode`, `ipc::pause_all`, `ipc::resume_all`, `ipc::set_input_enabled`,
`ipc::is_first_run` and `ipc::complete_first_run`. Add `ipc::get_status`, `ipc::start`,
`ipc::stop`, `ipc::test_move`, `ipc::get_run_settings`, `ipc::set_run_settings`,
`ipc::get_autostart` and `ipc::set_autostart`.

- [ ] **Step 5: Build, test and lint.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml` → all PASS.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → clean.
Then remove the temporary `#![allow(dead_code)]` from `core/running.rs` and run clippy again.
It stays clean because everything in that module is now used.

- [ ] **Step 6: Run fmt.**
Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`

- [ ] **Step 7: Commit.**

```bash
git add src-tauri/src
git commit -m "feat(M7): one Start/Stop path for UI, tray, hotkey, CLI; Home's IPC (FR-003/007/011/016)

Removes first run, pause and the mode buttons' commands (FR-012/013). Stop
reconciles power immediately (SC-004). Test runs its path off the main thread.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: The importer brings the movement across and finds Settings.xml

**Files:**
- Modify: `src-tauri/src/config/import_movemouse.rs`
- Modify: `src-tauri/src/ipc/mod.rs` (`import_move_mouse`)
- Modify: `specs/005-start-stop/spec.md` (FR-012 line)

**Interfaces:**
- Consumes: `InputSettings`, `Motion`, `crate::set_run_settings` (Task 5).
- Produces: `Imported { profile: Profile, input: Option<InputSettings>, report: Vec<String> }` ·
  `pub fn default_paths(appdata: Option<&Path>, local_appdata: Option<&Path>) -> Vec<PathBuf>`.

- [ ] **Step 1: Write the failing tests.** In the importer's `tests` module, add the imports
`use crate::core::input_engine::InputSettings; use crate::core::motion::Motion; use std::path::Path;`,
delete the line `assert!(!r.input_enabled); // power-only default`, replace
`reports_dropped_actions`, and add:

```rust
    /// Kalana's own Settings.xml, trimmed to what matters (spec 005 Input).
    const KALANA: &str = r#"<Settings>
      <Actions><MoveMouseCursorAction>
        <IsEnabled>true</IsEnabled><Direction>Square</Direction><Distance>10</Distance>
      </MoveMouseCursorAction></Actions>
      <LowerInterval>200</LowerInterval><UpperInterval>200</UpperInterval>
    </Settings>"#;

    #[test]
    fn the_cursor_action_becomes_homes_movement() {
        let r = import(KALANA).unwrap();
        assert_eq!(
            r.input,
            Some(InputSettings { interval_secs: 200, key: 0, motion: Motion::Square, distance_px: 10, vary_pct: 0 })
        );
        assert!(!r.report.iter().any(|l| l.contains("not imported")), "{:?}", r.report);
    }

    #[test]
    fn stealth_becomes_invisible() {
        let xml = KALANA.replace("<Direction>Square</Direction>", "<Direction>None</Direction>");
        assert_eq!(import(&xml).unwrap().input.unwrap().motion, Motion::Virtual);
    }

    #[test]
    fn an_unmatched_direction_is_approximated_and_said_so() {
        let xml = KALANA.replace("<Direction>Square</Direction>", "<Direction>NorthEast</Direction>");
        let r = import(&xml).unwrap();
        assert_eq!(r.input.unwrap().motion, Motion::Square);
        assert!(r.report.iter().any(|l| l.contains("no exact match")), "{:?}", r.report);
    }

    #[test]
    fn a_disabled_cursor_action_is_not_used() {
        let xml = KALANA.replace("<IsEnabled>true</IsEnabled>", "<IsEnabled>false</IsEnabled>");
        assert_eq!(import(&xml).unwrap().input, None);
    }

    #[test]
    fn no_cursor_action_leaves_movement_alone() {
        let r = import("<Settings/>").unwrap();
        assert_eq!(r.input, None);
        assert!(r.report.iter().any(|l| l.contains("unchanged")), "{:?}", r.report);
    }

    #[test]
    fn reports_dropped_actions() {
        let xml = r#"<Settings><Actions><MoveMouseCursorAction/><ClickMouseAction/></Actions></Settings>"#;
        let r = import(xml).unwrap();
        assert!(r.input.is_some());
        assert!(
            r.report.iter().any(|l| l.contains("1 other Move Mouse action(s) not imported")),
            "{:?}",
            r.report
        );
    }

    #[test]
    fn looks_in_the_portable_place_then_the_store_place() {
        let p = default_paths(Some(Path::new("C:/R")), Some(Path::new("C:/L")));
        assert_eq!(p[0], Path::new("C:/R/Ellanet/Move Mouse/Settings.xml"));
        assert_eq!(
            p[1],
            Path::new("C:/L/Packages/1258EllAbi.MoveMouse_hjfwaxvfbwh7t/LocalCache/Roaming/Ellanet/Move Mouse/Settings.xml")
        );
        assert!(default_paths(None, None).is_empty());
    }
```

- [ ] **Step 2: Run them and confirm they fail.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml import_movemouse`
Expected: compile errors (`r.input`, `default_paths`).

- [ ] **Step 3: Implement.** In `import_movemouse.rs`:

  - Update the module doc's last sentence to: *"Move Mouse's cursor action becomes Home's
    movement (spec 005). Start now does what Move Mouse did, plus the power request it never
    had."*
  - Add these imports:
    ```rust
    use std::path::{Path, PathBuf};
    use crate::core::input_engine::InputSettings;
    use crate::core::motion::Motion;
    ```
  - Replace the `Imported` struct with:

```rust
pub struct Imported {
    pub profile: Profile,
    /// Move Mouse's cursor action as Home's movement, or `None` if it had no enabled one, in
    /// which case the current movement settings are left alone.
    pub input: Option<InputSettings>,
    pub report: Vec<String>,
}

/// Where Move Mouse keeps `Settings.xml`: the GitHub/portable build, then the Store build
/// (MOVE-MOUSE.md §7). The caller supplies `%APPDATA%` and `%LOCALAPPDATA%`.
pub fn default_paths(appdata: Option<&Path>, local_appdata: Option<&Path>) -> Vec<PathBuf> {
    let tail = ["Ellanet", "Move Mouse", "Settings.xml"];
    let mut out = Vec::new();
    if let Some(a) = appdata {
        out.push(tail.iter().fold(a.to_path_buf(), |p, s| p.join(s)));
    }
    if let Some(l) = local_appdata {
        let store = l
            .join("Packages")
            .join("1258EllAbi.MoveMouse_hjfwaxvfbwh7t")
            .join("LocalCache")
            .join("Roaming");
        out.push(tail.iter().fold(store, |p, s| p.join(s)));
    }
    out
}

/// Move Mouse's `Direction` → the nearest closed motion here, and whether that is exact.
fn motion_of(direction: &str) -> (Motion, bool) {
    match direction {
        "Square" => (Motion::Square, true),
        "None" => (Motion::Virtual, true), // Stealth
        "LeftAndRight" | "RightAndLeft" => (Motion::Line, true),
        _ => (Motion::Square, false),
    }
}
```

  - In `import()`, delete the first `report.push(…"replaced by 'Keep running'"…)`.
  - Replace the `if n_actions > 0 { report.push(…) }` block with:

```rust
    // The first enabled cursor action becomes Home's movement (spec 005).
    let cursor = root.descendants().find(|n| {
        n.tag_name().name() == "MoveMouseCursorAction"
            && child_text(*n, "IsEnabled").is_none_or(|v| v.eq_ignore_ascii_case("true"))
    });
    let input = cursor.map(|a| {
        let direction = child_text(a, "Direction").unwrap_or_else(|| "Square".into());
        let (motion, exact) = motion_of(&direction);
        let distance_px = child_text(a, "Distance")
            .and_then(|d| d.parse().ok())
            .unwrap_or(10);
        let interval_secs = child_text(root, "LowerInterval")
            .and_then(|t| t.parse().ok())
            .unwrap_or(30);
        report.push(format!(
            "Mouse movement → Home: {direction}, {distance_px} px, after {interval_secs} s with no \
             input. Press Start to use it."
        ));
        if !exact {
            report.push(format!(
                "'{direction}' has no exact match here, so it became a small square."
            ));
        }
        if desc_flag(root, "RandomInterval").unwrap_or(false) {
            report.push("A random interval was not carried over. Settings → Vary does the same job.".into());
        }
        InputSettings { interval_secs, key: 0, motion, distance_px, vary_pct: 0 }
    });
    if input.is_none() {
        report.push("No enabled cursor action found, so Home's movement settings are unchanged.".into());
    }
    let others = n_actions - usize::from(cursor.is_some());
    if others > 0 {
        report.push(format!(
            "{others} other Move Mouse action(s) not imported, click, scroll, keys and commands \
             have no equivalent here."
        ));
    }
    if !conditions.is_empty() {
        report.push(
            "The conditions above became a rule in Advanced → Rules, switched off. It decides \
             when to keep the PC awake, not when to move."
                .into(),
        );
    }
```

  - In the final `Ok(Imported { … })`, replace `input_enabled: false,` with `input,`.

In `ipc/mod.rs`, add `run: State<'_, SharedRun>,` to `import_move_mouse`'s parameters, add
`use std::path::PathBuf;` at the top, and replace its body:

```rust
    // Empty → look where Move Mouse keeps it. Quotes from Explorer's "Copy as path" are stripped.
    let path = match path.trim().trim_matches('"') {
        "" => {
            let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
            let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
            crate::config::import_movemouse::default_paths(appdata.as_deref(), local.as_deref())
                .into_iter()
                .find(|p| p.exists())
                .ok_or("Move Mouse's Settings.xml is not in either usual place. Paste its full path.")?
        }
        p => PathBuf::from(p),
    };
    let xml = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let imported = crate::config::import_movemouse::import(&xml)?;
    engine.lock().unwrap().set_profile(imported.profile);
    if let Some(s) = imported.input {
        input.lock().unwrap().set_settings(s);
        let settings = RunSettings { move_mouse: true, ..*run.lock().unwrap() };
        crate::set_run_settings(&app, settings); // re-applies and saves
    } else {
        crate::persist_current(&app);
    }
    Ok(imported.report)
```

In `specs/005-start-stop/spec.md` FR-012, add this sentence after "The existing panels move
here unchanged.": *"The exception is the Move Mouse importer: it now carries the cursor
action's interval, direction and distance into Home and turns Move the mouse on. Left empty,
it finds `Settings.xml` in either of Move Mouse's usual places."*

- [ ] **Step 4: Run the tests and confirm they pass.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml` → all PASS.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → clean.
Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src specs/005-start-stop/spec.md
git commit -m "feat(M7): importer brings Move Mouse's movement to Home and finds Settings.xml

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: The tray (two icons, Start|Stop, countdown tooltip) and launch behaviour

**Files:**
- Create: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/lib.rs` (`mod tray;`, setup, scheduler tooltip, launch, forwarded,
  `open_window`; remove the old tray and tooltip code; add a `tests` module)
- Modify: `src-tauri/src/ipc/mod.rs` (the three `crate::rebuild_tray_menu` calls)

**Interfaces:**
- Consumes: `StatusKind`, `status_kind` (Task 3), `is_running`, `set_running` (Task 5),
  `InputEngine::next_move_in_secs` (Task 2).
- Produces: `tray::Icons::new(Image<'static>) -> Icons` · `tray::greyscale(&[u8]) -> Vec<u8>` ·
  `tray::countdown(u32) -> String` ·
  `tray::tooltip(StatusKind, Option<u32>, Option<u64>, Option<&str>) -> String` ·
  `tray::menu(&AppHandle, bool) -> tauri::Result<Menu<Wry>>` · `tray::sync(&AppHandle)` ·
  lib.rs `fn opens_window_at_launch(&[String]) -> bool` ·
  `fn forwarded_opens_window(&[String]) -> bool`.

- [ ] **Step 1: Write the failing tests.** Create `src-tauri/src/tray.rs` containing only the
tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greyscale_keeps_the_size_and_dims_the_icon() {
        let g = greyscale(&[255, 0, 0, 255, 10, 20, 30, 0]);
        assert_eq!(g.len(), 8);
        assert_eq!(&g[..4], &[76, 76, 76, 153]);
        assert_eq!(g[7], 0, "transparent stays transparent");
    }

    #[test]
    fn grey_stays_grey() {
        assert_eq!(greyscale(&[128, 128, 128, 255]), vec![128, 128, 128, 153]);
    }

    #[test]
    fn the_countdown_reads_like_a_clock() {
        assert_eq!(countdown(0), "0:00");
        assert_eq!(countdown(42), "0:42");
        assert_eq!(countdown(200), "3:20");
        assert_eq!(countdown(3_600), "60:00");
    }

    #[test]
    fn the_tooltip_says_what_is_true() {
        use StatusKind::*;
        assert_eq!(tooltip(Stopped, None, None, None), "project-mouse: Stopped");
        assert_eq!(
            tooltip(Running, Some(42), None, None),
            "project-mouse: Running · next move in 0:42"
        );
        assert!(tooltip(RunningBlocked, None, None, None).contains("blocked"));
        assert!(tooltip(RunningPowerOnly, None, None, None).contains("keeping the PC awake"));
        assert!(tooltip(StoppedButRuleHolds, None, Some(4_000), None).contains("1h 6m"));
        assert!(tooltip(Stopped, None, None, Some("0.3.0")).ends_with("\nUpdate 0.3.0 available"));
    }

    /// Windows cuts a tray tooltip at 127 characters.
    #[test]
    fn every_tooltip_fits() {
        use StatusKind::*;
        for k in [Stopped, StoppedButRuleHolds, Running, RunningBlocked, RunningPowerOnly] {
            let t = tooltip(k, Some(3_600), Some(86_399), Some("10.10.10"));
            assert!(t.chars().count() <= 127, "{} chars: {t}", t.chars().count());
        }
    }
}
```

Append to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_plain_launch_opens_the_window() {
        assert!(opens_window_at_launch(&args(&["project-mouse.exe"])));
    }

    #[test]
    fn autostart_stays_in_the_tray() {
        assert!(!opens_window_at_launch(&args(&["project-mouse.exe", "--minimized"])));
    }

    #[test]
    fn launching_again_shows_the_window() {
        assert!(forwarded_opens_window(&args(&["project-mouse.exe"])));
    }

    #[test]
    fn a_control_flag_from_a_script_does_not() {
        assert!(!forwarded_opens_window(&args(&["pm.exe", "--keep", "running"])));
        assert!(!forwarded_opens_window(&args(&["pm.exe", "--release"])));
        assert!(!forwarded_opens_window(&args(&["pm.exe", "--minimized"])));
    }

    #[test]
    fn show_always_shows() {
        assert!(forwarded_opens_window(&args(&["pm.exe", "--release", "--show"])));
    }
}
```

Add `mod tray;` after `mod sampler;` in lib.rs.

- [ ] **Step 2: Run them and confirm they fail.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml tray`
Expected: compile errors (`greyscale`, `opens_window_at_launch`, …).

- [ ] **Step 3: Implement `tray.rs`.** Put this above the tests:

```rust
//! The tray (spec 005 FR-009): colour while running, grey while stopped, a Start|Stop menu, and a
//! tooltip that says what is true right now.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::core::running::StatusKind;
use crate::{SharedEngine, SharedProfiles};

/// The running and stopped icons, made once at startup.
pub struct Icons {
    pub on: Image<'static>,
    pub off: Image<'static>,
}

impl Icons {
    pub fn new(on: Image<'static>) -> Self {
        let off = Image::new_owned(greyscale(on.rgba()), on.width(), on.height());
        Self { on, off }
    }
}

/// The stopped icon: the same mark, desaturated and dimmed. The shape carries the meaning and the
/// tooltip says it in words, so the state never rests on colour alone (UI-UX §7).
pub fn greyscale(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|p| {
            let l = ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000) as u8;
            [l, l, l, (p[3] as u32 * 3 / 5) as u8]
        })
        .collect()
}

/// `m:ss`, as Home shows it.
pub fn countdown(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn remaining(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        _ => format!("{}h {}m", secs / 3600, (secs % 3600) / 60),
    }
}

/// The tooltip: what is true now, when the next move is, how long a timed rule has left, and
/// whether an update is waiting.
pub fn tooltip(
    kind: StatusKind,
    next_move_secs: Option<u32>,
    rule_left_secs: Option<u64>,
    update: Option<&str>,
) -> String {
    let state = match kind {
        StatusKind::Stopped => "Stopped".to_string(),
        StatusKind::StoppedButRuleHolds => "Stopped · a rule is keeping the PC awake".to_string(),
        StatusKind::Running => match next_move_secs {
            Some(n) => format!("Running · next move in {}", countdown(n)),
            None => "Running".to_string(),
        },
        StatusKind::RunningPowerOnly => "Running · keeping the PC awake".to_string(),
        StatusKind::RunningBlocked => "Running · Windows blocked the last move".to_string(),
    };
    let mut s = format!("project-mouse: {state}");
    if let Some(left) = rule_left_secs {
        s.push_str(&format!(" · rule ends in {}", remaining(left)));
    }
    if let Some(v) = update {
        s.push_str(&format!("\nUpdate {v} available"));
    }
    s
}

/// The menu, rebuilt rather than patched: the Start|Stop label, the autostart check and the
/// profile list must all stay truthful, and regenerating is cheaper than tracking each.
pub fn menu(app: &AppHandle, running: bool) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let toggle = item("toggle", if running { "Stop" } else { "Start" })?;
    let open = item("open", "Open project-mouse")?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start with Windows",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let update = item("check_update", "Check for updates…")?;
    let quit = item("quit", "Quit")?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let profiles = profiles_menu(app)?;

    let mut items: Vec<&dyn IsMenuItem<Wry>> = Vec::with_capacity(8);
    items.push(&toggle);
    items.push(&open);
    items.push(&sep1);
    if let Some(p) = &profiles {
        items.push(p);
    }
    items.push(&autostart);
    items.push(&update);
    items.push(&sep2);
    items.push(&quit);
    Menu::with_items(app, &items)
}

/// Profiles earn a place in the tray only once there is more than one to switch between.
fn profiles_menu(app: &AppHandle) -> tauri::Result<Option<Submenu<Wry>>> {
    let active = app
        .try_state::<SharedEngine>()
        .map(|e| e.lock().unwrap().profile().id.clone())
        .unwrap_or_default();
    let stored = app
        .try_state::<SharedProfiles>()
        .map(|p| p.lock().unwrap().clone())
        .unwrap_or_default();
    if stored.len() < 2 {
        return Ok(None);
    }
    let entries = stored
        .iter()
        .map(|p| {
            CheckMenuItem::with_id(
                app,
                format!("profile:{}", p.id),
                &p.name,
                true,
                p.id == active,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let refs: Vec<&dyn IsMenuItem<Wry>> = entries
        .iter()
        .map(|i| i as &dyn IsMenuItem<Wry>)
        .collect();
    Submenu::with_items(app, "Profile", true, &refs).map(Some)
}

/// Bring the icon and the menu in line with the running state.
pub fn sync(app: &AppHandle) {
    let running = crate::is_running(app);
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    if let Some(icons) = app.try_state::<Icons>() {
        let icon = if running { &icons.on } else { &icons.off };
        let _ = tray.set_icon(Some(icon.clone()));
    }
    match menu(app, running) {
        Ok(m) => {
            if let Err(e) = tray.set_menu(Some(m)) {
                tracing::error!("tray menu update failed: {e}");
            }
        }
        Err(e) => tracing::error!("tray menu build failed: {e}"),
    }
}
```

- [ ] **Step 4: Wire it into `lib.rs`.**

  1. Delete `tooltip_for`, `tooltip_text`, `fmt_remaining`, `build_tray_menu` and
     `rebuild_tray_menu`. Replace every remaining `rebuild_tray_menu(app)` call in lib.rs with
     `tray::sync(app)`. That covers `switch_profile`, `set_autostart` and `after_change`. In
     `ipc/mod.rs`, replace the three `crate::rebuild_tray_menu(&app);` calls with
     `crate::tray::sync(&app);`.
  2. Remove `CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu` and `Wry` from
     lib.rs's `use tauri::…` lines.
  3. Add the launch helpers below `cli_keep_mode`:

```rust
/// Spec 005 FR-001: a launch shows the window, except autostart, which passes `--minimized` and
/// belongs in the tray.
fn opens_window_at_launch(args: &[String]) -> bool {
    !args.iter().any(|a| a == "--minimized")
}

/// A second launch carrying only a control flag (`--keep`, `--release`) is a script talking to
/// us. Anything else is a person opening the app again, and they should see it.
fn forwarded_opens_window(argv: &[String]) -> bool {
    argv.iter().any(|a| a == "--show")
        || !argv
            .iter()
            .any(|a| matches!(a.as_str(), "--keep" | "--release" | "--minimized"))
}
```

  4. Replace `apply_forwarded` with:

```rust
/// Apply what a *second* invocation forwarded to the running instance (single-instance): flags
/// control it (D10), and a plain launch brings the window back (spec 005 FR-001).
fn apply_forwarded(app: &tauri::AppHandle, argv: &[String]) {
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--release" => apply_mode(app, WakeMode::Off),
            "--keep" => {
                if let Some(m) = it.next().and_then(|v| mode_from_cli(v)) {
                    apply_mode(app, m);
                }
            }
            _ => {}
        }
    }
    if forwarded_opens_window(argv) {
        open_window(app);
    }
}
```

  5. In `open_window`, replace the early `if let Some(w) = … { let _ = w.set_focus(); return; }` with:

```rust
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
```

  6. In `setup`, replace from `let icon = app.default_window_icon()…` through the end of the
     `TrayIconBuilder` chain (`.build(app)?;`) with:

```rust
            let icons = tray::Icons::new(
                app.default_window_icon()
                    .cloned()
                    .expect("bundled window icon")
                    .to_owned(),
            );
            let running_now = is_running(app.handle());
            let first = if running_now { icons.on.clone() } else { icons.off.clone() };
            app.manage(icons);
            let _tray = TrayIconBuilder::with_id("main")
                .icon(first)
                .tooltip("project-mouse")
                .menu(&tray::menu(app.handle(), running_now)?)
                .show_menu_on_left_click(false) // left = open window, right = menu
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        open_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => set_running(app, !is_running(app)),
                    "open" => open_window(app),
                    "autostart" => toggle_autostart(app),
                    "check_update" => {
                        tauri::async_runtime::spawn(check_and_install(app.clone(), false));
                    }
                    "quit" => {
                        release_all(app);
                        app.exit(0);
                    }
                    id => {
                        if let Some(pid) = id.strip_prefix("profile:") {
                            switch_profile(app, pid);
                        }
                    }
                })
                .build(app)?;
```

  7. In the scheduler thread, add `let sched_run = run_state.clone();` next to the other
     clones. Replace the closure body between `let snap = …` and `true` with:

```rust
                    let snap = sched_sampler.snapshot();
                    let (on, effective, remaining) = {
                        let mut e = sched_engine.lock().unwrap();
                        e.tick(&snap);
                        (running::is_running(&e), e.mode(), soonest_expiry_secs(e.profile(), &snap))
                    };
                    let (blocked, next) = {
                        let mut ie = sched_input.lock().unwrap();
                        ie.tick(platform::last_input_tick(), platform::tick_now());
                        (ie.enabled() && ie.blocked, ie.next_move_in_secs())
                    };
                    let move_mouse = sched_run.lock().unwrap().move_mouse;
                    let kind = running::status_kind(on, move_mouse, blocked, effective);
                    let tip = tray::tooltip(kind, next, remaining, update_available().as_deref());
                    // Pushed only when the text changes. While running that is once a second,
                    // which is also what keeps an open window's countdown live.
                    if tip != last_tip {
                        last_tip = tip.clone();
                        if let Some(t) = sched_app.tray_by_id("main") {
                            let _ = t.set_tooltip(Some(&tip));
                        }
                        // Notify the UI only when a window is actually alive (ARCHITECTURE §8).
                        if sched_app.get_webview_window("main").is_some() {
                            let _ = sched_app.emit("state:changed", ());
                        }
                    }
                    true
```

     Also move `let run_state` above the `tauri::Builder` chain if it is not there already
     (Task 4 put it there). The closure is `move`, so `sched_run` must be a clone.
  8. Replace the `if std::env::args().any(|a| a == "--show") { … }` block with:

```rust
            let args: Vec<String> = std::env::args().collect();
            if opens_window_at_launch(&args) {
                open_window(app.handle());
            }
```

  9. Update the crate doc at the top of lib.rs to: *"project-mouse: Start/Stop over a wake
     engine and an input engine (spec 005). A scheduler thread ticks ~1 s: it samples state,
     evaluates the active profile, reconciles power and runs the input engine."*

- [ ] **Step 5: Run the tests and confirm they pass.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml` → all PASS.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → clean.
Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`.
Run the boundary lint: `rg -n "cfg\(windows\)" src-tauri/src | rg -v "src-tauri/src/platform/"`
→ no output.

- [ ] **Step 6: Commit.**

```bash
git add src-tauri/src
git commit -m "feat(M7): window opens at launch; tray shows running/stopped + countdown (FR-001/008/009)

A second launch brings the window back. Autostart (--minimized) stays in the
tray. The stopped icon is the same mark desaturated, made at runtime.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: The UI (Home, Settings, Advanced)

**Files:**
- Create: `src/types.ts`, `src/controls.tsx`, `src/updates.tsx`, `src/home.tsx`,
  `src/settings.tsx`, `src/advanced.tsx`
- Rewrite: `src/App.tsx`
- Delete: `src/firstrun.tsx`
- Modify: `src/rules.tsx` (`<h1>` → `<h2>`, one comment), `src/styles.css`, `index.html` (`<title>`)

**Interfaces:**
- Consumes the IPC from Task 5: `get_status`, `start`, `stop`, `test_move`,
  `get_run_settings`, `set_run_settings`, `get_autostart`, `set_autostart`, plus the existing
  `get_input_settings`, `set_input_settings`, `get_diagnostics`, `why_awake`,
  `list_profiles`, `set_profile`, `create_profile`, `delete_profile`, `get_rules`,
  `upsert_rule`, `delete_rule`, `get_logs`, `import_move_mouse`, `get_update_status`,
  `set_auto_update`, `check_for_update` and `install_update`.

There is no TS test runner in this repo, and adding one would be a new dependency. These steps
are gated by `tsc` and `vite build`. The behaviour is verified in Task 10.

- [ ] **Step 1: Shared types and controls.** Create `src/types.ts`:

```ts
// Mirrors of the Rust IPC types: ipc/mod.rs (Status), core/running.rs (RunSettings, StatusKind),
// core/input_engine.rs (InputSettings), core/motion.rs (Motion).
export type StatusKind =
  | "stopped"
  | "stopped_but_rule_holds"
  | "running"
  | "running_blocked"
  | "running_power_only";

export type Status = {
  kind: StatusKind;
  running: boolean;
  next_move_in_secs: number | null;
  keep_screen_on: boolean;
};

export type RunSettings = { move_mouse: boolean; keep_screen_on: boolean; start_on_launch: boolean };

export type Motion = "Virtual" | "Line" | "Square" | "Circle";

export type InputSettings = {
  interval_secs: number;
  key: number;
  motion: Motion;
  distance_px: number;
  vary_pct: number;
};

/** Virtual-key codes worth offering. 0 means "move the mouse". F15 is the category's convention
 *  (Caffeine), and it is also the one that breaks in PuTTY, PowerPoint and Google Docs, which is
 *  why the choice is the user's. */
export const KEYS: [number, string][] = [
  [0, "Mouse movement"],
  [0x7e, "F15 key press"],
  [0x91, "Scroll Lock key press"],
  [0x10, "Shift key press"],
];
```

Create `src/controls.tsx`:

```tsx
import { useEffect, useState, type ReactNode } from "react";

/** An on/off switch with the keyboard behaviour of a real one. */
export function Switch({ on, onChange, label }: { on: boolean; onChange: (next: boolean) => void; label: string }) {
  return (
    <div
      className={`switch ${on ? "on" : ""}`}
      role="switch"
      aria-checked={on}
      aria-label={label}
      tabIndex={0}
      onClick={() => onChange(!on)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onChange(!on);
        }
      }}
    >
      <span className="track">
        <span className="thumb" />
      </span>
      <span>{on ? "On" : "Off"}</span>
    </div>
  );
}

/** A number box that commits on blur or Enter. Anything that is not a number puts back the value
 *  in effect, so an empty box never reaches Rust as `null`, and the value is held to 0..max so it
 *  cannot overflow the Rust type. Rust applies the real range (Review Focus 2). */
export function NumberField({
  value,
  max,
  onCommit,
  label,
}: {
  value: number;
  max: number;
  onCommit: (n: number) => void;
  label: string;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const n = Math.round(Number(text));
    if (text.trim() === "" || !Number.isFinite(n)) {
      setText(String(value));
      return;
    }
    const clamped = Math.min(Math.max(n, 0), max);
    if (clamped === value) setText(String(value));
    else onCommit(clamped);
  };
  return (
    <input
      className="btn num"
      inputMode="numeric"
      aria-label={label}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

/** One settings line: what it is and what it does on the left, the control on the right. */
export function SettingRow({ title, note, children }: { title: string; note?: string; children: ReactNode }) {
  return (
    <div className="setting">
      <div>
        <div className="setting-title">{title}</div>
        {note && <div className="note">{note}</div>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}
```

- [ ] **Step 2: Updates.** Create `src/updates.tsx`. Move `UpdateBanner` (App.tsx L248-280,
with its doc comment) into it verbatim, and export it. Then add `UpdateSettings`, rewritten to
use `Switch` and a section:

```tsx
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingRow, Switch } from "./controls";

type UpdateStatus = { current: string; available: string | null; auto_check: boolean };

// …UpdateBanner, moved verbatim and exported: `export function UpdateBanner() { … }`

/** Settings → Updates. The switch is required by UPDATES.md §6: some people run this on
 *  locked-down machines where an outbound request gets noticed, and they deserve the choice. */
export function UpdateSettings() {
  const [u, setU] = useState<UpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [checked, setChecked] = useState(false);

  const read = useCallback(
    () => invoke<UpdateStatus>("get_update_status").then(setU).catch(() => {}),
    [],
  );
  useEffect(() => {
    read();
  }, [read]);

  if (!u) return null;

  const checkNow = () => {
    setChecking(true);
    setChecked(false);
    invoke("check_for_update").finally(() => {
      // The check runs in the background; give it a moment, then re-read.
      window.setTimeout(() => {
        read().then(() => {
          setChecking(false);
          setChecked(true);
        });
      }, 2500);
    });
  };

  return (
    <section className="section">
      <h2>Updates</h2>
      <SettingRow title="This version">
        <span>{u.current}</span>
      </SettingRow>
      <SettingRow title="Check for updates automatically" note="A check only tells you an update exists. It never installs one on its own.">
        <Switch
          label="Check for updates automatically"
          on={u.auto_check}
          onChange={(v) => invoke("set_auto_update", { enabled: v }).then(read)}
        />
      </SettingRow>
      <div className="cond-row" style={{ marginTop: 8 }}>
        <button className="btn" onClick={checkNow} disabled={checking}>
          {checking ? "Checking…" : "Check now"}
        </button>
        <span className="note">
          {u.available
            ? `Version ${u.available} is available. Install it from Home.`
            : checked
              ? "You are up to date."
              : "Checks run about every six hours."}
        </span>
      </div>
    </section>
  );
}
```

- [ ] **Step 3: Home.** Create `src/home.tsx`:

```tsx
// Home (spec 005 FR-010, UI-UX §0.5): what is true right now, one button, and the two settings
// people actually touch.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { KEYS, type InputSettings, type Motion, type Status } from "./types";
import { NumberField } from "./controls";
import { UpdateBanner } from "./updates";

const MOVEMENTS: [Motion, string][] = [
  ["Square", "Small square"],
  ["Line", "Back and forth"],
  ["Circle", "Small circle"],
  ["Virtual", "Invisible (the cursor doesn't move)"],
];

const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

function describe(s: Status): { title: string; detail: string; tone: "on" | "off" | "warn" } {
  switch (s.kind) {
    case "running":
      return {
        title: "Running",
        detail: s.next_move_in_secs == null ? "Starting…" : `Next move in ${clock(s.next_move_in_secs)}`,
        tone: "on",
      };
    case "running_blocked":
      return {
        title: "Running, but Windows blocked the last move",
        detail: "An app running as administrator is in front. Click another window and the next move will land.",
        tone: "warn",
      };
    case "running_power_only":
      return { title: "Running", detail: "Keeping the PC awake. Mouse moves are off in Settings.", tone: "on" };
    case "stopped_but_rule_holds":
      return { title: "Stopped", detail: "A rule in Advanced is still keeping the PC awake.", tone: "off" };
    default:
      return { title: "Stopped", detail: "Your PC can sleep and lock as normal.", tone: "off" };
  }
}

export default function Home() {
  const [s, setS] = useState<Status | null>(null);
  const [input, setInput] = useState<InputSettings | null>(null);
  const [testing, setTesting] = useState(false);

  const read = useCallback(() => {
    invoke<Status>("get_status").then(setS).catch(() => {});
  }, []);

  useEffect(() => {
    read();
    invoke<InputSettings>("get_input_settings").then(setInput).catch(() => {});
    // Once a second, as text (UI-UX §4). The event makes Start/Stop from the tray show at once.
    const t = window.setInterval(read, 1000);
    const un = listen("state:changed", read);
    return () => {
      window.clearInterval(t);
      un.then((f) => f());
    };
  }, [read]);

  const save = (next: InputSettings) =>
    invoke<InputSettings>("set_input_settings", { settings: next })
      .then((applied) => {
        setInput(applied); // Rust clamps; show what took effect
        read();
      })
      .catch(() => {});

  const test = () => {
    setTesting(true);
    invoke("test_move").catch(() => {});
    window.setTimeout(() => setTesting(false), 800); // one path at a time, not a queue of them
  };

  const running = s?.running ?? false;
  const d = s ? describe(s) : null;
  const key = input && input.key !== 0 ? KEYS.find(([k]) => k === input.key)?.[1] : null;

  return (
    <>
      <UpdateBanner />
      <div className={`status ${d?.tone ?? "off"}`} role="status" aria-live="polite">
        <div className="status-title">
          <span className="dot" aria-hidden="true" />
          {d?.title ?? "\u00a0"}
        </div>
        <div className="status-detail">{d?.detail ?? "\u00a0"}</div>
        {running && s && (
          <div className="status-detail">
            {s.keep_screen_on ? "PC won't sleep · screen stays on" : "PC won't sleep · the screen may turn off"}
          </div>
        )}
      </div>

      <button
        className={`btn big ${running ? "" : "primary"}`}
        onClick={() => invoke(running ? "stop" : "start").then(read).catch(() => {})}
      >
        {running ? "■  Stop" : "▶  Start"}
      </button>

      {input && (
        <div className="fields">
          <div className="field">
            <span>{key ? "Press the key after" : "Move the mouse after"}</span>
            <span className="inline">
              <NumberField
                label="Seconds with no input before a move"
                value={input.interval_secs}
                max={86_400}
                onCommit={(n) => save({ ...input, interval_secs: n })}
              />
              seconds with no input
            </span>
          </div>
          <div className="field">
            <span>Movement</span>
            <span className="inline">
              {key ? (
                <span className="note">{key} (change it in Settings)</span>
              ) : (
                <select
                  className="btn"
                  aria-label="Movement"
                  value={input.motion}
                  onChange={(e) => save({ ...input, motion: e.target.value as Motion })}
                >
                  {MOVEMENTS.map(([id, label]) => (
                    <option key={id} value={id}>
                      {label}
                    </option>
                  ))}
                </select>
              )}
              <button className="btn" onClick={test} disabled={testing}>
                Test
              </button>
            </span>
          </div>
        </div>
      )}

      <p className="note">
        When running, this moves your mouse a few pixels once your PC has had no input for that many
        seconds. Windows, the screen lock and apps that watch for idle time, such as Teams and Slack,
        see that as activity. It also keeps the PC from sleeping. Monitoring software can detect
        simulated input.
      </p>
      <p className="note">Closing this window keeps project-mouse running in the system tray, next to the clock.</p>
    </>
  );
}
```

- [ ] **Step 4: Settings.** Create `src/settings.tsx`:

```tsx
// Settings (spec 005 US4). Everything applies immediately, running or not.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { KEYS, type InputSettings, type RunSettings } from "./types";
import { NumberField, SettingRow, Switch } from "./controls";
import { UpdateSettings } from "./updates";

export default function Settings() {
  const [run, setRun] = useState<RunSettings | null>(null);
  const [input, setInput] = useState<InputSettings | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [autostartErr, setAutostartErr] = useState<string | null>(null);

  useEffect(() => {
    invoke<RunSettings>("get_run_settings").then(setRun).catch(() => {});
    invoke<InputSettings>("get_input_settings").then(setInput).catch(() => {});
    invoke<boolean>("get_autostart").then(setAutostart).catch(() => {});
  }, []);

  const saveRun = (next: RunSettings) => {
    setRun(next);
    invoke("set_run_settings", { settings: next }).catch(() => {});
  };
  const saveInput = (next: InputSettings) =>
    invoke<InputSettings>("set_input_settings", { settings: next }).then(setInput).catch(() => {});
  const toggleAutostart = (on: boolean) => {
    setAutostartErr(null);
    invoke<boolean>("set_autostart", { enabled: on })
      .then(setAutostart)
      .catch((e) => setAutostartErr(String(e)));
  };

  if (!run || !input) return <h1>Settings</h1>;
  const visible = input.key === 0 && input.motion !== "Virtual";

  return (
    <>
      <h1>Settings</h1>

      <section className="section">
        <h2>When running</h2>
        <SettingRow
          title="Move the mouse"
          note="Off: Start only keeps the PC awake. The screen can still lock, and Teams or Slack can still show you as away."
        >
          <Switch label="Move the mouse" on={run.move_mouse} onChange={(v) => saveRun({ ...run, move_mouse: v })} />
        </SettingRow>
        <SettingRow title="Keep the screen on" note="Off: the PC stays awake, but the screen may turn off.">
          <Switch label="Keep the screen on" on={run.keep_screen_on} onChange={(v) => saveRun({ ...run, keep_screen_on: v })} />
        </SettingRow>
        <SettingRow
          title="What to send"
          note="A key press instead of a mouse move. F15 is a key no keyboard has, but a few apps (PuTTY, PowerPoint, Google Docs) still react to it."
        >
          <select
            className="btn"
            aria-label="What to send"
            value={input.key}
            onChange={(e) => saveInput({ ...input, key: Number(e.target.value) })}
          >
            {KEYS.map(([code, label]) => (
              <option key={code} value={code}>
                {label}
              </option>
            ))}
          </select>
        </SettingRow>
        {visible && (
          <SettingRow
            title="Distance"
            note="How far each side of the movement goes. Pointer speed settings can stretch it a little. It always comes back to where it started."
          >
            <NumberField label="Distance in pixels" value={input.distance_px} max={500} onCommit={(n) => saveInput({ ...input, distance_px: n })} />
            px
          </SettingRow>
        )}
        <SettingRow
          title="Vary by"
          note="Changes the wait and the distance a little each time, so the move doesn't line up with other things on a timer and the cursor doesn't land on the same pixel. 0 keeps them fixed."
        >
          <NumberField label="Vary by percent" value={input.vary_pct} max={50} onCommit={(n) => saveInput({ ...input, vary_pct: n })} />%
        </SettingRow>
      </section>

      <section className="section">
        <h2>Starting</h2>
        <SettingRow title="Start automatically when project-mouse opens">
          <Switch
            label="Start automatically when project-mouse opens"
            on={run.start_on_launch}
            onChange={(v) => saveRun({ ...run, start_on_launch: v })}
          />
        </SettingRow>
        <SettingRow title="Start project-mouse with Windows" note="It opens in the tray, without this window.">
          <Switch label="Start project-mouse with Windows" on={autostart} onChange={toggleAutostart} />
        </SettingRow>
        {autostartErr && <p className="note error">{autostartErr}</p>}
        <p className="note">
          Ctrl+Alt+K starts and stops it from anywhere.
        </p>
      </section>

      <UpdateSettings />

      <section className="section">
        <h2>About</h2>
        <p className="note">Source and issues: github.com/kalanadidulanga/project-mouse</p>
        <p className="note">
          It does not change your power plan, and it lets go of everything when you quit. With Move the
          mouse off it sends no input at all, so it cannot keep the screen from locking or keep a chat
          status active.
        </p>
      </section>
    </>
  );
}
```

- [ ] **Step 5: Advanced.** Create `src/advanced.tsx`. Move these from the current
`src/App.tsx` (as of commit `2c44cbc`) **verbatim**, keeping their doc comments:

  - `fmtIdle` (L60-65)
  - `TIMER_ID`, `DURATIONS`, `Timer` (L154-246)
  - `ELEVATED_CMD`, `WhyAwake` (L357-431), with three edits: the outer `<div className="why">`
    becomes `<>`, its closing `</div>` becomes `</>`, and
    `<strong style={{ fontSize: 13 }}>Why is my PC awake?</strong>` becomes
    `<h2>Why is my PC awake?</h2>`
  - `ProfileSwitcher` (L433-467)
  - `ProfileManager` (L469-521)
  - the types `Diagnostics`, `AwakeReport` and `ProfileSummary` (L15-37)

Then add the new glue:

```tsx
// Advanced (spec 005 FR-012): everything the engine can do beyond Start/Stop, unchanged. Rules
// and the timer hold power on their own conditions, even while Home says Stopped.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import RulesPage, { Mode, Rule, ProfileView, modeWord } from "./rules";

// …moved: Diagnostics, AwakeReport, ProfileSummary, fmtIdle, TIMER_ID, DURATIONS, Timer,
//    ELEVATED_CMD, WhyAwake, ProfileSwitcher, ProfileManager

/** What Windows is being asked for right now, and the idle clocks (FEATURES E2/E3). */
function Readout({ diag }: { diag: Diagnostics | null }) {
  const state = (blocked: boolean | undefined) => (
    <span className={`state ${blocked ? "blocked" : "allowed"}`}>{blocked ? "blocked" : "allowed"}</span>
  );
  return (
    <>
      <div className="effect">
        <span className="label">System sleep</span>
        {state(diag?.system_sleep_blocked)}
        <span className="label">Display off</span>
        {state(diag?.display_blocked)}
        <span className="label">Screen lock</span>
        {state(diag?.lock_blocked)}
        <span className="label">Mouse moves</span>
        <span className="state" style={diag?.input_blocked ? { color: "var(--error)" } : undefined}>
          {diag?.input_enabled ? (diag.input_blocked ? "blocked" : "on") : "off"}
        </span>
      </div>
      {diag?.input_blocked && (
        <p className="note error" style={{ marginTop: 12 }}>
          Input is being discarded: an app running as administrator has focus, so the move goes nowhere.
        </p>
      )}
      <div style={{ marginTop: 12 }}>
        <div className="row"><span className="k">Memory</span><span className="v">{diag ? `${diag.memory_mb.toFixed(1)} MB` : "-"}</span></div>
        <div className="row"><span className="k">System idle</span><span className="v">{diag ? fmtIdle(diag.system_idle_secs) : "-"}</span></div>
        <div className="row"><span className="k">Your idle</span><span className="v">{diag ? fmtIdle(diag.human_idle_secs) : "-"}</span></div>
        {diag?.remote_session && (
          <div className="row"><span className="k">Session</span><span className="v">remote (RDP or similar)</span></div>
        )}
      </div>
    </>
  );
}

function Activity() {
  const [logs, setLogs] = useState<string[]>([]);
  const load = useCallback(() => {
    invoke<string[]>("get_logs", { limit: 100 }).then(setLogs).catch(() => {});
  }, []);
  useEffect(load, [load]);
  return (
    <>
      <div className="log">{logs.length ? logs.join("\n") : "No activity yet."}</div>
      <button className="btn" style={{ marginTop: 8 }} onClick={load}>
        Refresh
      </button>
    </>
  );
}

function ImportMoveMouse() {
  const [path, setPath] = useState("");
  const [report, setReport] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const run = () => {
    setError(null);
    setReport(null);
    invoke<string[]>("import_move_mouse", { path }).then(setReport).catch((e) => setError(String(e)));
  };
  return (
    <>
      <div style={{ display: "flex", gap: 8 }}>
        <input
          className="btn"
          style={{ flex: 1 }}
          placeholder="Leave empty to find it, or paste the path to Settings.xml"
          value={path}
          onChange={(e) => setPath(e.target.value)}
        />
        <button className="btn primary" onClick={run}>
          Import
        </button>
      </div>
      {error && <p className="note error">{error}</p>}
      {report && (
        <ul className="note" style={{ marginTop: 8, paddingLeft: 18 }}>
          {report.map((l, i) => (
            <li key={i} style={{ marginBottom: 4 }}>
              {l}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

export default function Advanced() {
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const refresh = useCallback(() => {
    invoke<Diagnostics>("get_diagnostics").then(setDiag).catch(() => {});
  }, []);
  useEffect(() => {
    refresh();
    const t = window.setInterval(refresh, 2000);
    return () => window.clearInterval(t);
  }, [refresh]);

  return (
    <>
      <h1>Advanced</h1>
      <p className="note">
        These work alongside Start/Stop. Rules and the timer keep the PC awake on their own conditions,
        even while Home says Stopped. None of them move the mouse.
      </p>
      <section className="section">
        <h2>Profiles</h2>
        <ProfileSwitcher onChange={refresh} />
        <ProfileManager onChange={refresh} />
      </section>
      <section className="section">
        <h2>Keep awake for a while</h2>
        <Timer onChange={refresh} />
      </section>
      <section className="section">
        <RulesPage />
      </section>
      <section className="section">
        <h2>What Windows is being asked for</h2>
        <Readout diag={diag} />
      </section>
      <section className="section">
        <WhyAwake />
      </section>
      <section className="section">
        <h2>Activity</h2>
        <Activity />
      </section>
      <section className="section">
        <h2>Import from Move Mouse</h2>
        <ImportMoveMouse />
      </section>
    </>
  );
}
```

In `src/rules.tsx`, change `<h1>Rules</h1>` to `<h2>Rules</h2>`. Change the comment
`// The timer rule is owned by the Status page; it would only be confusing here.` to
`// The timer rule is owned by Advanced → Keep awake for a while; it would only be confusing here.`

- [ ] **Step 6: The shell.** Replace `src/App.tsx` entirely:

```tsx
// The window (spec 005, UI-UX §0.5): three pages, Home first. All state lives in Rust; each
// page reads what it shows.
import { useState } from "react";
import Home from "./home";
import Settings from "./settings";
import Advanced from "./advanced";
import "./styles.css";

type Page = "home" | "settings" | "advanced";

const PAGES: [Page, string][] = [
  ["home", "Home"],
  ["settings", "Settings"],
  ["advanced", "Advanced"],
];

export default function App() {
  const [page, setPage] = useState<Page>("home");
  return (
    <div className="app">
      <nav className="rail" aria-label="Pages">
        <div className="brand">project-mouse</div>
        {PAGES.map(([id, label]) => (
          <button
            key={id}
            className={page === id ? "active" : ""}
            aria-current={page === id ? "page" : undefined}
            onClick={() => setPage(id)}
          >
            {label}
          </button>
        ))}
      </nav>
      <main className="content">
        {page === "home" && <Home />}
        {page === "settings" && <Settings />}
        {page === "advanced" && <Advanced />}
      </main>
    </div>
  );
}
```

Delete `src/firstrun.tsx`. In `index.html`, change the `<title>` to `project-mouse`.

- [ ] **Step 7: Styles.** In `src/styles.css`:

  - Delete these rules: `.status-card`, `.status-line`, `.status-line.active`, `.status-sub`,
    `.btn-group`, `.btn.selected`, the whole `/* First run … */` block (`.firstrun`,
    `.choices`, `.choice*`) and `.why`.
  - In `.switch`, change `margin-top: 16px;` to `margin-top: 0;`.
  - Append:

```css
/* Home + sections (spec 005, UI-UX §0.5). Layout only, no motion. */
.content h2 { font-size: 13px; font-weight: 600; margin: 0 0 8px; }
.section { margin-top: 20px; border-top: 1px solid var(--border); padding-top: 16px; }

.status { padding: 4px 0 16px; }
.status-title { display: flex; align-items: center; gap: 8px; font-size: 18px; font-weight: 600; }
.status .dot { width: 10px; height: 10px; border-radius: 50%; border: 2px solid var(--text-2); flex-shrink: 0; }
.status.on .dot { background: var(--active); border-color: var(--active); }
.status.warn .dot { background: var(--error); border-color: var(--error); }
.status.warn .status-title { color: var(--error); }
.status-detail { color: var(--text-2); font-size: 13px; margin-top: 4px; font-variant-numeric: tabular-nums; }

.btn.big { display: block; width: 100%; text-align: center; padding: 12px 0; font-size: 15px; font-weight: 600; }
.btn:disabled { opacity: .5; }

.fields { margin: 16px 0 8px; }
.field { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 8px 0; font-size: 13px; }
.field .inline, .setting-control { display: flex; align-items: center; gap: 8px; flex-shrink: 0; font-size: 13px; }
.btn.num { width: 64px; text-align: right; font-variant-numeric: tabular-nums; }

.setting { display: flex; justify-content: space-between; align-items: center; gap: 16px; padding: 10px 0; }
.setting-title { font-size: 13px; }
.note.error { color: var(--error); }
```

- [ ] **Step 8: Typecheck and build.**
Run: `npx tsc --noEmit` → no errors. Run: `npm run build` → succeeds.
Run the honesty gate: `rg -in "undetectable|human-like|looks human|natural motion" src src-tauri/src`
→ no output.

- [ ] **Step 9: Commit.**

```bash
git add -A src index.html
git commit -m "feat(M7): Home / Settings / Advanced: Start/Stop you can see (FR-010/011/012/013)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: README, CHANGELOG, CLAUDE.md

**Files:**
- Modify: `README.md` (lines 1-11, the intro)
- Modify: `CHANGELOG.md` (new top section)
- Modify: `CLAUDE.md` (point at this plan)

- [ ] **Step 1: README.** Replace everything from `# project-mouse` through the
`*(project-mouse is a working title …)*` line with:

```markdown
# project-mouse

Moves your mouse a few pixels when your PC has had no input for a while, and keeps the PC awake
while it runs. Windows, the screen lock and apps that watch for idle time see activity. One
button: **Start**. Underneath is a wake lock with a rules engine.

**Using it:** open the app and press **Start**. *Next move in m:ss* counts down and restarts
whenever you touch the mouse or keyboard. Close the window and it keeps running in the tray,
next to the clock. **Stop**, the tray menu or Ctrl+Alt+K turns it off.

It does not change your power plan, and it releases everything on exit. Monitoring software can
detect simulated input. With *Move the mouse* off (Settings) it sends no input at all, and then
it cannot defeat a screen lock or a chat presence indicator.

*(`project-mouse` is a working title, see [PRODUCT.md §9](docs/PRODUCT.md#9-the-name).)*

The rest of this README explains the engine underneath.
```

- [ ] **Step 2: CHANGELOG.** Insert above `## v0.2.1`:

```markdown
## Unreleased

project-mouse is now something you can see. v0.2.1 opened no window on a normal launch, so all
you got was a tray icon (often hidden), and moving the mouse was a switch buried in Settings.

### Changed

- **The window opens when you start the app**, on **Home**: one **Start/Stop** button, what is
  true right now, and *Next move in m:ss*. Closing it keeps the app running in the tray. Opening
  the app again brings the window back. Autostart still starts in the tray.
- **Start moves the mouse and keeps the PC awake**, together. The mouse moves once the PC has
  had no input for the number of seconds you set (60 by default), so it never moves while you
  are working. *Move the mouse* and *Keep the screen on* are in Settings.
- **Each move traces a whole small square** (or a line or a circle) and comes back to where it
  started. Before, one 10 px side moved per interval, which looked like nothing. **Test**
  shows it straight away.
- **The tray icon is grey while stopped**, the menu is Start/Stop, and the tooltip shows the
  countdown. Ctrl+Alt+K starts and stops it.
- The old Status, Rules and Activity pages, and the rest of Settings, are under **Advanced**,
  unchanged. Off / Keep running / Keep presenting and Pause are gone, because Start/Stop does
  their job. The first-run question is gone too.
- **Import from Move Mouse** now brings your movement settings across, and finds
  `Settings.xml` by itself (including the Store version's).

### Fixed

- A Windows-blocked move is retried once per interval instead of every second.
- A config file that is valid JSON but not an object no longer crashes the app at launch. It is
  treated as corrupt and kept.

### Notes

- Config moves to v3 on first launch. If you never switched input on in v0.2.x, your movement
  settings start fresh at the new defaults. Whether it was running is no longer remembered:
  use *Start automatically when project-mouse opens*.
```

- [ ] **Step 3: CLAUDE.md.** Change `specs/003-settings-ui/plan.md` to
`specs/005-start-stop/plan.md`, and change the sentence about the files beside it to:
*"(with `spec.md` beside it; earlier milestones' `research.md`, `data-model.md` and
`contracts/` are in `specs/003-settings-ui/`)"*.

- [ ] **Step 4: Commit.**

```bash
git add README.md CHANGELOG.md CLAUDE.md
git commit -m "docs(M7): README, CHANGELOG and CLAUDE.md for Start/Stop

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Verify it for real

**Files:** none in the repo. Helper scripts go in the session scratchpad.

- [ ] **Step 1: All the gates.**

```bash
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
rg -in "undetectable|human-like|looks human|natural motion" src src-tauri/src   # no output
rg -n "cfg\(windows\)" src-tauri/src | rg -v "src-tauri/src/platform/"          # no output
```

Expected: every one clean. Record the test count.

- [ ] **Step 2: Clear the way.** Quit any running project-mouse first: the installed v0.2.1
and any old debug build. The single-instance plugin would otherwise hand our launch to the old
instance and exit. Use `Get-Process project-mouse -ErrorAction SilentlyContinue | Stop-Process`.
Start the dev build with `npm run tauri dev` in the background. Its exe sits in
`src-tauri/target/debug/`, a writable folder, so its config is `src-tauri/target/debug/config.json`
and the installed app's config is untouched.

- [ ] **Step 3: SC-001, the window at launch.** In a scratchpad `windows.ps1`:

```powershell
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public static class W {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public static int Count(uint pid, string title) {
    int c = 0;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && IsWindowVisible(h)) { var sb = new StringBuilder(256); GetWindowTextW(h, sb, 256); if (sb.ToString() == title) c++; }
      return true; }, IntPtr.Zero);
    return c;
  }
}
"@
$p = Get-Process project-mouse | Select-Object -First 1
"visible 'project-mouse' windows: " + [W]::Count([uint32]$p.Id, "project-mouse")
```

Check each of these:
- After launch: `1`.
- Close the window (X), then run the script again: `0`, and the process is still alive.
- Run `src-tauri/target/debug/project-mouse.exe` again: `1` (second launch → window).
- Quit, relaunch with `--minimized`: `0`.

Screenshot Home with `PrintWindow` and `PW_RENDERFULLCONTENT` (flag 2), then look at it. It
should show *Stopped*, **Start**, the interval and movement rows, and both notes, with nothing
clipped at 640×480. Check light and dark.

- [ ] **Step 4: SC-002/003, the move itself.** Write a BOM-less config
(`[IO.File]::WriteAllText(path, json, (New-Object Text.UTF8Encoding $false))`) with interval
5 s and start on launch:
`{"schema_version":3,"input":{"interval_secs":5,"key":0,"motion":"Square","distance_px":10,"vary_pct":0},"run":{"move_mouse":true,"keep_screen_on":true,"start_on_launch":true}}`.
Relaunch. **Ask Kalana not to touch the mouse or keyboard for 15 s.** Sample the cursor:

```powershell
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class C { [StructLayout(LayoutKind.Sequential)] public struct P { public int X; public int Y; }
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out P p); }
"@
$seen = [ordered]@{}; $first = $null; $p = New-Object C+P
for ($i = 0; $i -lt 240; $i++) { [void][C]::GetCursorPos([ref]$p); $k = "$($p.X),$($p.Y)"; if (-not $first) { $first = $k }; $seen[$k] = 1; Start-Sleep -Milliseconds 50 }
"start $first  end $($p.X),$($p.Y)  distinct positions: $($seen.Count)"
```

Expected: the start and end positions are equal, and there are more than 5 distinct positions
(the path was traced). Screenshot Home and confirm *Next move in 0:0x* counts down. Then move
the mouse and confirm the countdown is back to 0:05.

- [ ] **Step 5: SC-004/005, Stop, tray, hotkey.**
  - Press **Stop** on Home, open Advanced within 2 s, and screenshot it. The readout shows
    *System sleep: allowed*, and "Why is my PC awake?" says *project-mouse is holding nothing*.
  - The tray icon is grey while stopped and colour while running. Press Ctrl+Alt+K and the
    icon changes. Right-click the menu: it reads Start/Stop and toggles. Left-click opens the
    window.
  - Hover the tray tooltip while running: it shows the countdown.

  The tray needs a human: ask Kalana to confirm the icon colour and the menu, or screenshot
  the notification area.

- [ ] **Step 6: Review Focus 5, the screen edge.** With the 5 s config running, use a
scratchpad P/Invoke `SetCursorPos(right edge, middle)` on the primary monitor. Sample as in
step 4 over three moves. Expected: after the first move the cursor sits at most one leg (10 px)
left of the edge, and it stays there across the next moves without drifting further.

- [ ] **Step 7: Memory (SC-009).** Close the window, wait 30 s, and read the private working
set (`(Get-Process project-mouse).PrivateMemorySize64`). Expected: ≤ 8 MB (debug build; the
budget is for release, so note the build type with the number).

- [ ] **Step 8: Review.** Run `/code-review` on the branch diff since `2c44cbc`. Fix every
CONFIRMED finding test-first and commit each fix (`fix(M7): …`).

- [ ] **Step 9: Hand over to Kalana.** These cannot be automated:
  - **SC-010:** with the interval at 200 s, leave the PC idle for 10+ minutes and confirm
    Teams stays Available.
  - Install the NSIS build (`npm run tauri build`) and check that launching from the Start
    menu opens the window.

Report every result with the actual output, including anything that failed.
