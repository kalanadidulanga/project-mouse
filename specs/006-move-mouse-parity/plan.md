# M8 Move Mouse Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give project-mouse what Move Mouse has, organised the way Move Mouse organises it.
- One movement with the full direction list, plus random distance and interval, speed, and
  abort-if-I-move.
- Run for, automatic pauses, schedules and blackouts.
- Always on top, a taskbar dot, notifications and live idle time.
- A seven-tab UI that a non-technical user can find their way around.

**Architecture:**
- Two new pure core modules:
  - `core::autopilot` decides Start/Stop from schedules and the Run-for deadline, and decides
    pauses from battery, lock, presenting and blackouts.
  - `core::apps` turns "keep awake while these apps run" into one process rule.
- `core::running` gains the pause overlay. Running state never changes for a pause: moves stop,
  and only a battery pause also releases power.
- The platform `move_path` switches to absolute moves, so paths return exactly and abort detection
  is exact.
- The UI moves from Home/Settings/Advanced to seven tabs.

**Tech Stack:** Rust 2021 · Tauri 2.11 (+ `tauri-plugin-notification` 2, the only new
dependency) · windows-rs 0.62 · React 19 + TypeScript 5.8 + Vite 7.

**Spec:** `specs/006-move-mouse-parity/spec.md` (binding). Base: M7, `specs/005-start-stop/spec.md`.

## Global Constraints

- **No em dash (U+2014) anywhere:** code, comments, strings, docs, commit messages. The owner
  forbids it. Use a comma, colon, full stop or parentheses instead.
- **One new crate:** `tauri-plugin-notification = "2"` (Task 8). No new npm packages.
- Constitution:
  - **I:** the two engines stay separate.
  - **II:** honest naming. Never "undetectable", "human-like", "looks human" or "natural motion".
    Randomness is described as keeping moves from lining up with other timers.
  - **III:** no persistent system changes, and the power request is released on every exit.
  - **IV:** no `cfg(windows)` outside `src-tauri/src/platform/`, and `ipc/` stays thin.
  - **V:** engine logic is test-first against `platform::mock`.
  - **VI:** config is versioned, corrupt files are kept, and rules are never dropped silently.
- **Clamps:**
  - interval 5–3600 s
  - distance 1–500 px
  - custom speed 1–50 ms per step
  - a path never exceeds 40 steps
  - times of day 0–1439
- **Window:** fixed 760×540, not maximisable. It is destroyed on close, never hidden. Nothing
  loops visually, and text countdowns update at most once a second.
- **Tauri invoke arguments are camelCase in JS:** a Rust parameter `run_for` is sent as
  `{ runFor }`.
- **UI copy:**
  - It may say Windows, the screen lock and apps such as Teams and Slack watch idle time.
  - It never promises a status.
  - Home keeps *Monitoring software can detect simulated input.*
- **Commits:** the repo's git config is the personal identity. Never pass `-c user.*`. End every
  commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Commands** run from the repo root:
  - `cargo test --manifest-path src-tauri/Cargo.toml <filter>`
  - `npx tsc --noEmit`
  - `npm run build`
  - Never run `cargo build`, because a dev instance may hold the exe. `cargo check` is fine.

## Review Focus

1. **Schedules around sleep, wake and midnight.** A PC asleep across 09:00 should fire the Start
   on wake. Yesterday's edges never fire. Two edges in one gap resolve to the later one, and a tie
   resolves to Stop. Pinned in Task 4.
2. **Pause transitions** (lock and unlock, plug and unplug, a blackout starting while running).
   Moves stop and resume. Power is released only for battery. Start/Stop state never changes, and
   the countdown and blocked flag reset cleanly on resume. Pinned in Tasks 4 and 5.
3. **Odd timetable values** (a time ≥ 1440 from a hand-edited config, a blackout with `from ==
   to`). These are clamped or ignored, never a panic and never a silent non-firing. Pinned in Task
   4 (`sanitised`, `active_until`) and Task 12 (the UI refuses `from == to`).
4. **Abort and exact return under pointer acceleration and DPI scaling.** Absolute moves make both
   exact. Pinned by Task 2's normalisation tests, plus a manual check in Task 15.
5. **Notifications that fail or are suppressed** (Focus Assist, the plugin erroring). Nothing else
   may be affected. Pinned by Task 8: errors are ignored (`let _ =`) and the setting gates every
   toast.

---

### Task 1: Motion: every Move Mouse direction, speed, and range picking

**Files:**
- Rewrite: `src-tauri/src/core/motion.rs`. Every line above `fn vary` is replaced; `vary` and its
  tests stay until Task 3.
- Modify: `src-tauri/src/core/input_engine.rs` (`dispatch` only)
- Modify: `src-tauri/src/config/import_movemouse.rs` (`motion_of` and one test)

**Interfaces:**
- Produces:
  - `enum Motion { Virtual, Square, Circle, RightAndLeft (serde alias "Line"), LeftAndRight, UpAndDown, DownAndUp, North, NorthEast, East, SouthEast, South, SouthWest, West, NorthWest, Random }`
  - `Motion::ALL: [Motion; 16]`
  - `pub fn path(motion: Motion, distance: i32, seed: u32) -> Vec<(i32, i32)>`
  - `enum Speed { Slow, Normal (default), Fast, Custom }`, with `Speed::step_ms(self, custom: u8) -> u32`
  - `pub fn pick(lo: u32, hi: u32, seed: u32) -> u32`
  - `pub const MAX_PATH_STEPS: u32 = 40`
  - `STEP_MS` is removed.

- [ ] **Step 1: Write the failing tests.** Replace the whole `#[cfg(test)] mod tests` in
`core/motion.rs` with this. Keep the four `variation_*` tests from the old module at the end,
unchanged; Task 3 deletes them.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn sum(p: &[(i32, i32)]) -> (i32, i32) {
        p.iter().fold((0, 0), |(x, y), (dx, dy)| (x + dx, y + dy))
    }

    /// Spec 006 FR-002: every direction ends where it started, whatever the distance or seed.
    #[test]
    fn every_direction_returns_to_its_origin() {
        for m in Motion::ALL {
            for d in [1, 2, 7, 10, 33, 500] {
                for seed in [0, 1, 7, 12_345] {
                    assert_eq!(sum(&path(m, d, seed)), (0, 0), "{m:?} at {d}px, seed {seed}");
                }
            }
        }
    }

    #[test]
    fn a_path_glides_in_small_steps_and_stays_short() {
        for m in Motion::ALL.into_iter().filter(|&m| m != Motion::Virtual) {
            let p = path(m, 10, 3);
            assert!(p.len() > 2, "{m:?} jumps instead of gliding: {p:?}");
            assert!(p.len() as u32 <= MAX_PATH_STEPS);
            assert!(path(m, 500, 3).len() as u32 <= MAX_PATH_STEPS);
        }
    }

    #[test]
    fn a_square_goes_right_down_left_up() {
        let p = path(Motion::Square, 10, 0);
        assert_eq!(p.len(), 20);
        assert!(p[..5].iter().all(|&s| s == (2, 0)));
        assert!(p[5..10].iter().all(|&s| s == (0, 2)));
        assert!(p[10..15].iter().all(|&s| s == (-2, 0)));
        assert!(p[15..].iter().all(|&s| s == (0, -2)));
    }

    #[test]
    fn one_direction_goes_out_then_comes_back() {
        let east = path(Motion::East, 10, 0);
        assert_eq!(east, [vec![(2, 0); 5], vec![(-2, 0); 5]].concat());
        assert_eq!(path(Motion::North, 10, 0)[0], (0, -2), "north is up the screen");
        let ne = path(Motion::NorthEast, 10, 0)[0];
        assert!(ne.0 > 0 && ne.1 < 0, "north-east goes right and up: {ne:?}");
        let sw = path(Motion::SouthWest, 10, 0)[0];
        assert!(sw.0 < 0 && sw.1 > 0, "south-west goes left and down: {sw:?}");
    }

    #[test]
    fn back_and_forth_starts_in_the_named_direction() {
        assert_eq!(path(Motion::RightAndLeft, 10, 0)[0], (2, 0));
        assert_eq!(path(Motion::LeftAndRight, 10, 0)[0], (-2, 0));
        assert_eq!(path(Motion::UpAndDown, 10, 0)[0], (0, -2));
        assert_eq!(path(Motion::DownAndUp, 10, 0)[0], (0, 2));
    }

    #[test]
    fn random_picks_a_compass_direction_and_changes_with_the_seed() {
        use Motion::*;
        let compass: Vec<(i32, i32)> = [North, NorthEast, East, SouthEast, South, SouthWest, West, NorthWest]
            .iter()
            .map(|&m| path(m, 10, 0)[0])
            .collect();
        let starts: HashSet<(i32, i32)> = (0..64).map(|s| path(Random, 10, s)[0]).collect();
        assert!(starts.len() >= 4, "only {} directions in 64 seeds", starts.len());
        assert!(starts.iter().all(|s| compass.contains(s)), "{starts:?}");
    }

    /// Pointer acceleration sees the same speeds on the way out and the way back.
    #[test]
    fn opposite_legs_use_mirrored_steps() {
        let p = path(Motion::RightAndLeft, 7, 0);
        let (out, back) = p.split_at(p.len() / 2);
        let mirrored: Vec<_> = out.iter().map(|&(x, y)| (-x, -y)).collect();
        assert_eq!(back, &mirrored[..]);
    }

    #[test]
    fn invisible_and_zero_distance_have_no_path() {
        assert!(path(Motion::Virtual, 500, 1).is_empty());
        for m in Motion::ALL {
            assert!(path(m, 0, 1).is_empty(), "{m:?} moved with distance 0");
        }
    }

    #[test]
    fn configs_from_before_m8_keep_their_back_and_forth() {
        let m: Motion = serde_json::from_str("\"Line\"").unwrap();
        assert_eq!(m, Motion::RightAndLeft);
    }

    #[test]
    fn speed_sets_the_gap_between_steps() {
        assert_eq!(Speed::Slow.step_ms(0), 20);
        assert_eq!(Speed::Normal.step_ms(0), 10);
        assert_eq!(Speed::Fast.step_ms(0), 4);
        assert_eq!(Speed::Custom.step_ms(15), 15);
        assert_eq!(Speed::Custom.step_ms(0), 1);
        assert_eq!(Speed::Custom.step_ms(200), 50);
        assert_eq!(Speed::default(), Speed::Normal);
    }

    #[test]
    fn pick_stays_in_range_and_varies() {
        let v: HashSet<u32> = (0..200).map(|s| pick(10, 20, s)).collect();
        assert!(v.iter().all(|x| (10..=20).contains(x)), "{v:?}");
        assert!(v.len() > 5, "only {} values", v.len());
        assert_eq!(pick(30, 30, 9), 30);
        assert_eq!(pick(40, 30, 9), 40, "lo >= hi returns lo");
    }

    // (keep the four existing `variation_*` tests here, unchanged, until Task 3)
}
```

- [ ] **Step 2: Run the tests and confirm they fail.**
Run `cargo test --manifest-path src-tauri/Cargo.toml motion`.
Expected: compile errors (`Motion::ALL`, `Speed`, `pick`, and `path` called with 3 arguments).

- [ ] **Step 3: Implement.** Replace everything in `core/motion.rs` above `/// Deterministic, tiny`
(the `xorshift` doc comment) with the block below. Then add `pick` after `xorshift`, and keep
`vary` as it is.

```rust
//! Cursor movement (FEATURES C2, spec 006 FR-001 to FR-004) and the honest kind of randomness.
//!
//! Pure: the actual `SendInput` lives behind `platform::InputInjector`. Every path here is a
//! **closed** one. Its steps sum to zero, so the cursor ends where the user left it.
//!
//! Randomness here exists so moves do not line up with other timers and the cursor does not land
//! on the same pixel every time. It is not here to look human, and no string in this file or the
//! UI may say that it is (PRODUCT §5, constitution II).

use serde::{Deserialize, Serialize};

/// What the cursor does on each move: Move Mouse's full Direction list (spec 006 FR-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Motion {
    /// C1 virtual jiggle: resets the idle timer, and the cursor does not move. Not the default
    /// for new settings (`InputSettings` uses Square); this `Default` only fills a missing
    /// `motion` field in old configs.
    #[default]
    Virtual,
    Square,
    Circle,
    /// Right, then back. Configs written before M8 call this `Line`.
    #[serde(alias = "Line")]
    RightAndLeft,
    LeftAndRight,
    UpAndDown,
    DownAndUp,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
    /// One of the eight compass directions, out and back, picked afresh for every move.
    Random,
}

/// The eight compass directions in tenths of the distance, with screen y growing downwards:
/// N, NE, E, SE, S, SW, W, NW.
const COMPASS: [(i32, i32); 8] = [
    (0, -10),
    (7, -7),
    (10, 0),
    (7, 7),
    (0, 10),
    (-7, 7),
    (-10, 0),
    (-7, -7),
];

impl Motion {
    /// Every direction, for tests that must cover them all.
    pub const ALL: [Motion; 16] = [
        Motion::Virtual,
        Motion::Square,
        Motion::Circle,
        Motion::RightAndLeft,
        Motion::LeftAndRight,
        Motion::UpAndDown,
        Motion::DownAndUp,
        Motion::North,
        Motion::NorthEast,
        Motion::East,
        Motion::SouthEast,
        Motion::South,
        Motion::SouthWest,
        Motion::West,
        Motion::NorthWest,
        Motion::Random,
    ];

    /// The legs of one move at `d` px. `seed` matters only to `Random`.
    ///
    /// ⚠️ Relative distances pass through pointer acceleration when sent as relative moves. The
    /// platform sends absolute moves instead (spec 006 FR-005), so the pixels land as asked.
    fn legs(self, d: i32, seed: u32) -> Vec<(i32, i32)> {
        let out_and_back = |i: usize| {
            let (x, y) = COMPASS[i];
            let (a, b) = (x * d / 10, y * d / 10);
            vec![(a, b), (-a, -b)]
        };
        match self {
            Motion::Virtual => Vec::new(),
            Motion::Square => vec![(d, 0), (0, d), (-d, 0), (0, -d)],
            Motion::Circle => {
                // The second half negates the first, so it closes however the octant rounds.
                const OCTANT: [(i32, i32); 4] = [(7, 3), (3, 7), (-3, 7), (-7, 3)];
                let half: Vec<(i32, i32)> = OCTANT
                    .iter()
                    .map(|&(x, y)| (x * d / 10, y * d / 10))
                    .collect();
                half.iter()
                    .copied()
                    .chain(half.iter().map(|&(x, y)| (-x, -y)))
                    .collect()
            }
            Motion::RightAndLeft => vec![(d, 0), (-d, 0)],
            Motion::LeftAndRight => vec![(-d, 0), (d, 0)],
            Motion::UpAndDown => vec![(0, -d), (0, d)],
            Motion::DownAndUp => vec![(0, d), (0, -d)],
            Motion::North => out_and_back(0),
            Motion::NorthEast => out_and_back(1),
            Motion::East => out_and_back(2),
            Motion::SouthEast => out_and_back(3),
            Motion::South => out_and_back(4),
            Motion::SouthWest => out_and_back(5),
            Motion::West => out_and_back(6),
            Motion::NorthWest => out_and_back(7),
            Motion::Random => out_and_back((xorshift(seed) % 8) as usize),
        }
    }
}

/// How fast the cursor travels a path (spec 006 FR-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Speed {
    Slow,
    #[default]
    Normal,
    Fast,
    Custom,
}

impl Speed {
    /// Milliseconds between path steps. `custom` applies to `Custom` only and is held to 1–50.
    pub fn step_ms(self, custom: u8) -> u32 {
        match self {
            Speed::Slow => 20,
            Speed::Normal => 10,
            Speed::Fast => 4,
            Speed::Custom => custom.clamp(1, 50) as u32,
        }
    }
}

/// The most steps one move may take, so a path stays well under a second at Normal speed.
pub const MAX_PATH_STEPS: u32 = 40;

/// One move's whole closed path: every leg, split into steps of about 2 px so the cursor glides
/// rather than jumps. Each leg's steps sum exactly to that leg (the split telescopes), and the
/// legs already sum to zero, so the path ends where it started. Opposite legs split into mirrored
/// steps. `Virtual` and a zero distance have no path.
pub fn path(motion: Motion, distance: i32, seed: u32) -> Vec<(i32, i32)> {
    let legs = motion.legs(distance, seed);
    if legs.is_empty() {
        return Vec::new();
    }
    let per_leg = (MAX_PATH_STEPS / legs.len() as u32).max(1);
    let mut out = Vec::with_capacity(MAX_PATH_STEPS as usize);
    for (dx, dy) in legs {
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

Add this after `fn xorshift`:

```rust
/// A value in `[lo, hi]`, drawn from `seed` (spec 006 FR-003, FR-006). `lo >= hi` returns `lo`.
pub fn pick(lo: u32, hi: u32, seed: u32) -> u32 {
    if hi <= lo {
        return lo;
    }
    lo + xorshift(seed) % (hi - lo + 1)
}
```

In `core/input_engine.rs`, change the last line of `dispatch` to the following, and remove any
`motion::STEP_MS` use:

```rust
        self.injector.move_path(
            &motion::path(self.motion, distance, now.rotate_left(16)),
            motion::Speed::Normal.step_ms(0),
        )
```

In `config/import_movemouse.rs`, replace `motion_of` with:

```rust
/// Move Mouse's `Direction` → ours. Every Move Mouse value has an exact match since M8; anything
/// else becomes a small square, and the report says so.
fn motion_of(direction: &str) -> (Motion, bool) {
    let m = match direction {
        "Square" => Motion::Square,
        "None" => Motion::Virtual, // Stealth
        "Random" => Motion::Random,
        "North" => Motion::North,
        "NorthEast" => Motion::NorthEast,
        "East" => Motion::East,
        "SouthEast" => Motion::SouthEast,
        "South" => Motion::South,
        "SouthWest" => Motion::SouthWest,
        "West" => Motion::West,
        "NorthWest" => Motion::NorthWest,
        "UpAndDown" => Motion::UpAndDown,
        "DownAndUp" => Motion::DownAndUp,
        "LeftAndRight" => Motion::LeftAndRight,
        "RightAndLeft" => Motion::RightAndLeft,
        _ => return (Motion::Square, false),
    };
    (m, true)
}
```

In `config/migrate.rs` tests, the test `v2_with_input_enabled_keeps_its_settings` names
`Motion::Line`. Change it to `Motion::RightAndLeft`; the JSON there still says `"Line"`, which the
serde alias reads.

In the importer's tests, the test `an_unmatched_direction_is_approximated_and_said_so` uses
`NorthEast`, which now matches exactly. Change it to `<Direction>Spiral</Direction>`, and add:

```rust
    #[test]
    fn every_move_mouse_direction_maps_exactly() {
        for d in [
            "Square", "None", "Random", "North", "NorthEast", "East", "SouthEast", "South",
            "SouthWest", "West", "NorthWest", "UpAndDown", "DownAndUp", "LeftAndRight", "RightAndLeft",
        ] {
            assert!(motion_of(d).1, "{d} should map exactly");
        }
        assert_eq!(motion_of("None").0, Motion::Virtual);
    }
```

- [ ] **Step 4: Run the tests and confirm they pass.**
Run `cargo test --manifest-path src-tauri/Cargo.toml`. Expected: all pass. Then run
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` and
`cargo fmt --manifest-path src-tauri/Cargo.toml`.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/core/motion.rs src-tauri/src/core/input_engine.rs src-tauri/src/config/import_movemouse.rs
git commit -m "feat(M8): every Move Mouse direction, speed, and range picking (FR-001..FR-004)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Absolute moves that return exactly, with abort-if-I-move

**Files:**
- Modify: `src-tauri/src/platform/mod.rs` (`PathOutcome`, and the `move_path` signature)
- Modify: `src-tauri/src/platform/windows/input.rs` (absolute moves, abort, `Desk` and its tests)
- Modify: `src-tauri/src/platform/mock.rs` (`MockInjector`: abort simulation, recorded speed and abort)
- Modify: `src-tauri/src/core/input_engine.rs` (`dispatch` adapts to the new signature)

**Interfaces:**
- Consumes: `motion::path` (Task 1).
- Produces:
  - `pub struct PathOutcome { pub elapsed_ms: u32, pub aborted: bool }` in `platform`
  - `InputInjector::move_path(&self, steps: &[(i32, i32)], step_ms: u32, abortable: bool) -> Result<PathOutcome>`
  - `MockInjector` gains `user_moves_after: Arc<Mutex<Option<usize>>>`,
    `last_step_ms: Arc<Mutex<Option<u32>>>` and `last_abortable: Arc<Mutex<Option<bool>>>`

- [ ] **Step 1: Write the failing tests.** Append to `platform/windows/input.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const ONE: Desk = Desk { x: 0, y: 0, w: 1920, h: 1080 };
    /// A second monitor to the left of the primary: the virtual desktop starts at x = -1280.
    const TWO: Desk = Desk { x: -1280, y: 0, w: 3200, h: 1080 };

    #[test]
    fn normalise_maps_the_corners_to_the_ends_of_the_range() {
        assert_eq!(ONE.normalise((0, 0)), (0, 0));
        assert_eq!(ONE.normalise((1919, 1079)), (65_535, 65_535));
        assert_eq!(TWO.normalise((-1280, 0)), (0, 0));
        assert_eq!(TWO.normalise((1919, 1079)), (65_535, 65_535));
    }

    #[test]
    fn normalise_round_trips_to_the_same_pixel() {
        // Windows maps back with x = nx * (w - 1) / 65535. The rounding must not move us a pixel.
        for px in [0, 1, 17, 640, 959, 1918, 1919] {
            let (nx, _) = ONE.normalise((px, 0));
            let back = (nx as i64 * (ONE.w as i64 - 1) + 32_767) / 65_535;
            assert_eq!(back, px as i64, "pixel {px} came back as {back}");
        }
    }

    #[test]
    fn clamp_keeps_a_point_on_the_desktop() {
        assert_eq!(ONE.clamp((-5, 2000)), (0, 1079));
        assert_eq!(TWO.clamp((-2000, -1)), (-1280, 0));
        assert_eq!(ONE.clamp((100, 100)), (100, 100));
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail.**
Run `cargo test --manifest-path src-tauri/Cargo.toml windows::input`. Expected: compile error
(`Desk` not found).

- [ ] **Step 3: Implement.** In `platform/mod.rs`, add this above `trait InputInjector`:

```rust
/// How a path ended (spec 006 FR-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathOutcome {
    /// How long it took. The engine treats that whole span as its own input.
    pub elapsed_ms: u32,
    /// The user took the mouse mid-path, and the rest of the path was skipped.
    pub aborted: bool,
}
```

Then replace the `move_path` declaration in the trait with:

```rust
    /// Trace a closed path, `step_ms` apart (spec 006 FR-004). With `abortable`, stop at once if
    /// the cursor is not where the previous step put it, because the user has taken the mouse.
    fn move_path(&self, steps: &[(i32, i32)], step_ms: u32, abortable: bool) -> Result<PathOutcome>;
```

In `platform/windows/input.rs`:
- Extend the imports: `MOUSEEVENTF_ABSOLUTE` and `MOUSEEVENTF_VIRTUALDESK` from
  `KeyboardAndMouse`, `windows::Win32::Foundation::POINT`, and
  `windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetSystemMetrics, SetCursorPos, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN}`.
- Change the existing `use crate::platform::{InputInjector, PlatformError, Result};` to add
  `PathOutcome`.
- Delete the inherent `fn move_relative` (it becomes unused).
- Replace the trait's `fn move_path` with the version below, and add the helpers.

```rust
    fn move_path(&self, steps: &[(i32, i32)], step_ms: u32, abortable: bool) -> Result<PathOutcome> {
        let started = std::time::Instant::now();
        let origin = cursor_pos().ok_or_else(|| PlatformError("GetCursorPos failed".into()))?;
        let desk = Desk::current();
        let mut target = origin;
        let mut aborted = false;
        for (i, &(dx, dy)) in steps.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(step_ms as u64));
                // The cursor is not where our last step put it: the user has the mouse. Let go.
                if abortable
                    && cursor_pos().is_some_and(|(x, y)| (x - target.0).abs() > 2 || (y - target.1).abs() > 2)
                {
                    aborted = true;
                    break;
                }
            }
            target = desk.clamp((target.0 + dx, target.1 + dy));
            send(&[absolute(desk.normalise(target))])?;
        }
        // Absolute coordinates round to 1/65535 of the desktop, and a step clipped at a screen
        // edge leaves the sum short. Either way, land exactly where we began. SetCursorPos is not
        // input, so this changes nothing about the idle clock.
        if !aborted && cursor_pos() != Some(origin) {
            unsafe {
                let _ = SetCursorPos(origin.0, origin.1);
            }
        }
        Ok(PathOutcome {
            elapsed_ms: started.elapsed().as_millis() as u32,
            aborted,
        })
    }
```

Add these below the `impl InputInjector for WindowsInputInjector` block:

```rust
fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok().map(|_| (p.x, p.y))
}

/// The virtual desktop: every monitor's rectangle together, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Desk {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Desk {
    fn current() -> Self {
        unsafe {
            Desk {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
                w: GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2),
                h: GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2),
            }
        }
    }

    fn clamp(self, (px, py): (i32, i32)) -> (i32, i32) {
        (
            px.clamp(self.x, self.x + self.w - 1),
            py.clamp(self.y, self.y + self.h - 1),
        )
    }

    /// `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK` coordinates: 0..=65535 across the whole
    /// virtual desktop, rounded to the nearest so the pixel comes back unchanged.
    fn normalise(self, (px, py): (i32, i32)) -> (i32, i32) {
        let n = |v: i32, origin: i32, len: i32| {
            let span = (len - 1).max(1) as i64;
            (((v - origin) as i64 * 65_535 + span / 2) / span) as i32
        };
        (n(px, self.x, self.w), n(py, self.y, self.h))
    }
}

fn absolute((nx, ny): (i32, i32)) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: nx,
                dy: ny,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                time: 0,
                dwExtraInfo: MAGIC_EXTRA,
            },
        },
    }
}
```

In `platform/mock.rs`:
- Add three fields to `MockInjector`, each with a doc comment:
  - `/// Pretend the user grabs the mouse after this many steps.` → `pub user_moves_after: Arc<Mutex<Option<usize>>>,`
  - `/// The step gap the last path asked for.` → `pub last_step_ms: Arc<Mutex<Option<u32>>>,`
  - `/// Whether the last path was abortable.` → `pub last_abortable: Arc<Mutex<Option<bool>>>,`
- Import `PathOutcome`.
- Replace `MockInjector::move_path` with:

```rust
    fn move_path(&self, steps: &[(i32, i32)], step_ms: u32, abortable: bool) -> Result<PathOutcome> {
        self.call()?;
        *self.last_step_ms.lock().unwrap() = Some(step_ms);
        *self.last_abortable.lock().unwrap() = Some(abortable);
        let grab = *self.user_moves_after.lock().unwrap();
        for (i, s) in steps.iter().enumerate() {
            if abortable && grab.is_some_and(|k| i > k) {
                return Ok(PathOutcome { elapsed_ms: i as u32 * step_ms, aborted: true });
            }
            self.moves.lock().unwrap().push(*s);
        }
        Ok(PathOutcome { elapsed_ms: steps.len() as u32 * step_ms, aborted: false })
    }
```

Also update `NoopInjector::move_path` to the new signature, returning
`Ok(PathOutcome { elapsed_ms: 0, aborted: false })`.

In `core/input_engine.rs`, `dispatch` becomes:

```rust
        self.injector
            .move_path(
                &motion::path(self.motion, distance, now.rotate_left(16)),
                motion::Speed::Normal.step_ms(0),
                false,
            )
            .map(|o| o.elapsed_ms)
```

- [ ] **Step 4: Run the tests and confirm they pass.**
Run `cargo test --manifest-path src-tauri/Cargo.toml`, then clippy `-D warnings`, then `cargo fmt`.
Expected: everything is clean, including the 3 new `windows::input` tests.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/platform src-tauri/src/core/input_engine.rs
git commit -m "feat(M8): absolute moves return exactly; abort when the user takes the mouse (FR-005)

Relative moves went through pointer acceleration and clipped at screen edges;
absolute moves on the virtual desktop land where asked, so a closed path
really closes and 'the user moved it' can be told apart from our own steps.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Input settings: random interval and distance, speed, abort

**Files:**
- Modify: `src-tauri/src/core/input_engine.rs` (`InputSettings`, the engine's stored settings, `dispatch`, `move_now`, tests)
- Modify: `src-tauri/src/core/motion.rs` (delete `vary` and its four `variation_*` tests)
- Modify: `src-tauri/src/config/import_movemouse.rs` (the `InputSettings` it builds, and one test literal)
- Modify: `src-tauri/src/config/migrate.rs` (one test literal)

The last two still name `vary_pct`. Rewrite them with struct update syntax, so they keep
compiling as fields are added:
- In `import()`, the importer builds `InputSettings { interval_secs, key: 0, motion, distance_px, vary_pct: 0 }`.
  Change it to `InputSettings { interval_secs, motion, distance_px, ..InputSettings::default() }`.
- Its test `the_cursor_action_becomes_homes_movement` expects a literal with `vary_pct: 0`.
  Change it to
  `InputSettings { interval_secs: 200, motion: Motion::Square, distance_px: 10, ..InputSettings::default() }`.
- In `migrate.rs`, `v2_with_input_enabled_keeps_its_settings` expects
  `InputSettings { interval_secs: 200, key: 0, motion: Motion::RightAndLeft, distance_px: 25, vary_pct: 10 }`.
  Change it to
  `InputSettings { interval_secs: 200, motion: Motion::RightAndLeft, distance_px: 25, ..InputSettings::default() }`.
  The v2 JSON's `vary_pct` is ignored, and the new fields take their defaults.

**Interfaces:**
- Consumes: `Speed`, `pick` and `path` (Task 1); `move_path(.., abortable)`, `PathOutcome` and the
  mock fields (Task 2).
- Produces:
  - `InputSettings { interval_secs, interval_random: bool, interval_max_secs, key, motion, distance_px, distance_random: bool, distance_max_px, speed: Speed, custom_step_ms: u8, abortable: bool }`
  - Defaults: 60 s, not random, max 120; key 0; Square; 10 px, not random, max 20; Normal;
    custom step 10 ms; abortable on.
  - `vary_pct` is gone.

- [ ] **Step 1: Write the failing tests.** In `core/input_engine.rs` tests:
- Replace `defaults_are_a_visible_square_after_a_minute`,
  `variation_is_drawn_once_per_cycle_so_the_countdown_is_steady` and
  `settings_round_trip_and_clamp` with the versions below.
- Add the remaining tests.
- Leave the other existing tests unchanged; their `every(..)` helper still compiles.

```rust
    #[test]
    fn defaults_are_a_visible_square_after_a_minute_that_lets_go_when_touched() {
        let d = InputSettings::default();
        assert_eq!((d.interval_secs, d.interval_random, d.interval_max_secs), (60, false, 120));
        assert_eq!((d.key, d.motion), (0, Motion::Square));
        assert_eq!((d.distance_px, d.distance_random, d.distance_max_px), (10, false, 20));
        assert_eq!((d.speed, d.custom_step_ms, d.abortable), (Speed::Normal, 10, true));
    }

    #[test]
    fn a_random_interval_is_drawn_once_per_cycle_so_the_countdown_is_steady() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            interval_random: true,
            interval_max_secs: 90,
            ..every(30, Motion::Virtual)
        });
        e.set_enabled(true);
        e.tick(0, 30_000); // the first cycle is exactly the low end
        assert_eq!(calls(&m), 1);
        e.tick(30_000, 31_000);
        let a = e.next_move_in_secs().unwrap();
        e.tick(30_000, 32_000);
        let b = e.next_move_in_secs().unwrap();
        assert_eq!(a - b, 1, "the countdown jumped: {a} then {b}");
    }

    #[test]
    fn a_random_interval_stays_in_range_and_varies() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            interval_random: true,
            interval_max_secs: 120,
            ..every(60, Motion::Virtual)
        });
        let cycles: std::collections::HashSet<u32> = (0..40u32)
            .map(|i| {
                e.move_now(i.wrapping_mul(7_919));
                e.cycle_ms()
            })
            .collect();
        assert!(cycles.iter().all(|c| (60_000..=120_000).contains(c)), "{cycles:?}");
        assert!(cycles.len() > 3, "only {} distinct waits", cycles.len());
    }

    #[test]
    fn a_fixed_interval_ignores_the_max_and_a_swapped_range_still_works() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings { interval_max_secs: 9, ..every(60, Motion::Virtual) });
        e.move_now(5);
        assert_eq!(e.cycle_ms(), 60_000);
        e.set_settings(InputSettings {
            interval_random: true,
            interval_max_secs: 30,
            ..every(90, Motion::Virtual)
        });
        for i in 0..20u32 {
            e.move_now(i * 31);
            assert!((30_000..=90_000).contains(&e.cycle_ms()));
        }
    }

    #[test]
    fn a_random_distance_stays_in_range() {
        for seed in 0..30u32 {
            let m = MockInjector::default();
            let mut e = engine(&m);
            e.set_settings(InputSettings {
                distance_random: true,
                distance_px: 10,
                distance_max_px: 20,
                ..every(5, Motion::Square)
            });
            e.move_now(seed.wrapping_mul(104_729));
            // A square's only rightward leg is its first, so the rightward total is the distance.
            let d: i32 = m.moves.lock().unwrap().iter().filter(|s| s.0 > 0).map(|s| s.0).sum();
            assert!((10..=20).contains(&d), "seed {seed}: {d}px");
        }
    }

    #[test]
    fn speed_and_abort_reach_the_injector() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings { speed: Speed::Slow, abortable: false, ..every(5, Motion::Square) });
        e.move_now(1);
        assert_eq!(*m.last_step_ms.lock().unwrap(), Some(20));
        assert_eq!(*m.last_abortable.lock().unwrap(), Some(false));
        e.set_settings(InputSettings { speed: Speed::Custom, custom_step_ms: 0, ..every(5, Motion::Square) });
        e.move_now(2);
        assert_eq!(*m.last_step_ms.lock().unwrap(), Some(1), "custom is held to 1..=50");
    }

    /// Spec 006 edge case: an aborted path still counts as a move, so the countdown restarts.
    #[test]
    fn an_aborted_path_still_counts_as_a_move() {
        let m = MockInjector::default();
        *m.user_moves_after.lock().unwrap() = Some(2);
        let mut e = engine(&m);
        e.set_settings(every(5, Motion::Square));
        e.set_enabled(true);
        e.tick(0, 5_000);
        assert_eq!(calls(&m), 1);
        assert_eq!(m.moves.lock().unwrap().len(), 3, "it stopped after the user grabbed the mouse");
        e.tick(5_030, 6_000);
        assert_eq!(e.next_move_in_secs(), Some(5));
    }

    #[test]
    fn settings_round_trip_and_clamp() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            interval_secs: 1,
            interval_random: true,
            interval_max_secs: 99_999,
            key: 0x7E,
            motion: Motion::Circle,
            distance_px: 0,
            distance_random: true,
            distance_max_px: 9_999,
            speed: Speed::Custom,
            custom_step_ms: 0,
            abortable: false,
        });
        assert_eq!(
            e.settings(),
            InputSettings {
                interval_secs: 5,
                interval_random: true,
                interval_max_secs: 3_600,
                key: 0x7E,
                motion: Motion::Circle,
                distance_px: 1,
                distance_random: true,
                distance_max_px: 500,
                speed: Speed::Custom,
                custom_step_ms: 1,
                abortable: false,
            }
        );
    }
```

Change the test module's import line to `use crate::core::motion::Speed;` plus the existing imports.

- [ ] **Step 2: Run the tests and confirm they fail.**
Run `cargo test --manifest-path src-tauri/Cargo.toml input_engine`. Expected: compile errors (the new
fields, and `cycle_ms`).

- [ ] **Step 3: Implement.**

Replace `InputSettings`, its `Default` and `default_distance` with:

```rust
/// The user-settable input-engine knobs (spec 006 Movement tab). Clamped in
/// `InputEngine::set_settings`. Settings saved before M8 still load: missing fields take their
/// defaults, and the old `vary_pct` is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSettings {
    /// Move once the PC has had no input for this many seconds (the low end when random).
    pub interval_secs: u32,
    /// Draw the wait afresh each cycle between `interval_secs` and `interval_max_secs` (FR-006).
    #[serde(default)]
    pub interval_random: bool,
    #[serde(default = "default_interval_max")]
    pub interval_max_secs: u32,
    /// 0 = move the mouse; otherwise press this virtual-key code instead.
    pub key: u16,
    /// What the cursor does (FR-001). `Virtual` moves nothing visible.
    #[serde(default)]
    pub motion: Motion,
    /// Pixels per leg (the low end when random). Ignored by `Motion::Virtual`.
    #[serde(default = "default_distance")]
    pub distance_px: u16,
    /// Draw the distance afresh each move between `distance_px` and `distance_max_px` (FR-003).
    #[serde(default)]
    pub distance_random: bool,
    #[serde(default = "default_distance_max")]
    pub distance_max_px: u16,
    #[serde(default)]
    pub speed: Speed,
    /// Milliseconds between steps when `speed` is `Custom`.
    #[serde(default = "default_custom_step")]
    pub custom_step_ms: u8,
    /// Let go at once if the user touches the mouse mid-path (FR-005).
    #[serde(default = "default_true")]
    pub abortable: bool,
}

fn default_interval_max() -> u32 {
    120
}
fn default_distance() -> u16 {
    10
}
fn default_distance_max() -> u16 {
    20
}
fn default_custom_step() -> u8 {
    10
}
fn default_true() -> bool {
    true
}

impl Default for InputSettings {
    /// A visible 10 px square after a minute with no input, letting go if touched: what someone
    /// arriving from Move Mouse expects to see.
    fn default() -> Self {
        Self {
            interval_secs: 60,
            interval_random: false,
            interval_max_secs: default_interval_max(),
            key: 0,
            motion: Motion::Square,
            distance_px: default_distance(),
            distance_random: false,
            distance_max_px: default_distance_max(),
            speed: Speed::Normal,
            custom_step_ms: default_custom_step(),
            abortable: true,
        }
    }
}
```

In `struct InputEngine`, replace the five fields `interval_ms`, `key`, `motion`, `distance_px`
and `vary_pct` with a single field:

```rust
    /// The settings, already clamped.
    s: InputSettings,
```

In `InputEngine::new`, replace those five field initialisers with `s: InputSettings::default(),`.
Then replace `set_settings`, `settings`, `dispatch` and `move_now` with:

```rust
    /// Clamped so a bad config or a typo cannot produce a runaway injector: interval 5 s–1 h,
    /// distance 1–500 px, custom speed 1–50 ms per step. A swapped min and max is fine: the draw
    /// orders them.
    pub fn set_settings(&mut self, s: InputSettings) {
        self.s = InputSettings {
            interval_secs: s.interval_secs.clamp(5, 3_600),
            interval_max_secs: s.interval_max_secs.clamp(5, 3_600),
            distance_px: s.distance_px.clamp(1, 500),
            distance_max_px: s.distance_max_px.clamp(1, 500),
            custom_step_ms: s.custom_step_ms.clamp(1, 50),
            ..s
        };
        self.cycle_ms = self.s.interval_secs * 1000;
    }

    pub fn settings(&self) -> InputSettings {
        self.s
    }

    /// This cycle's wait: fixed, or drawn between the two ends (FR-006).
    fn draw_interval_ms(&self, seed: u32) -> u32 {
        let (a, b) = (self.s.interval_secs, self.s.interval_max_secs);
        let secs = if self.s.interval_random { motion::pick(a.min(b), a.max(b), seed) } else { a };
        secs * 1000
    }

    /// This move's distance: fixed, or drawn between the two ends (FR-003).
    fn draw_distance(&self, seed: u32) -> i32 {
        let (a, b) = (self.s.distance_px as u32, self.s.distance_max_px as u32);
        let d = if self.s.distance_random { motion::pick(a.min(b), a.max(b), seed) } else { a };
        d as i32
    }

    #[cfg(test)]
    fn cycle_ms(&self) -> u32 {
        self.cycle_ms
    }

    /// What actually gets synthesized: a key if one is set, else the motion. Returns how long it
    /// took, so the whole span can be recognised as ours.
    fn dispatch(&self, now: u32) -> crate::platform::Result<u32> {
        if self.s.key != 0 {
            return self.injector.key(self.s.key).map(|()| 0);
        }
        if self.s.motion == Motion::Virtual {
            return self.injector.virtual_jiggle().map(|()| 0);
        }
        let steps = motion::path(self.s.motion, self.draw_distance(now), now.rotate_left(16));
        let outcome = self.injector.move_path(
            &steps,
            self.s.speed.step_ms(self.s.custom_step_ms),
            self.s.abortable,
        )?;
        if outcome.aborted {
            tracing::info!("move aborted: the user took the mouse");
        }
        Ok(outcome.elapsed_ms)
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
        // Drawn once per cycle, so the countdown is steady. Seeded from the tick, no RNG state.
        self.cycle_ms = self.draw_interval_ms(now ^ 0x5BD1_E995);
    }
```

In `core/motion.rs`, delete `pub fn vary` and its doc comment, plus the four `variation_*`
tests.

- [ ] **Step 4: Run the tests and confirm they pass.**
Run `cargo test --manifest-path src-tauri/Cargo.toml`, clippy `-D warnings`, and `cargo fmt`.
Expected: all clean. The config tests still pass, because the new fields have serde defaults.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/core
git commit -m "feat(M8): random interval and distance ranges, speed, abort-if-I-move (FR-003..FR-006)

Replaces vary-by-percent with explicit min and max, like Move Mouse.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `core::autopilot`: schedules, Run for, and pauses

**Files:**
- Create: `src-tauri/src/core/autopilot.rs`
- Modify: `src-tauri/src/core/mod.rs` (`pub mod autopilot;`)
- Modify: `src-tauri/src/core/running.rs` (`RunSettings` gains three pause switches)

**Interfaces:**
- Consumes: `Snapshot` (`weekday`, `minutes`, `epoch_secs`, `on_ac`, `session_locked`,
  `notification_state`), `NotifState`, `RunSettings`.
- Produces:
  - `ScheduleAction { Start, Stop }`
  - `Schedule { days: [bool; 7], at: u16, action, enabled }`
  - `Blackout { days, from, to, enabled }` with `active_until(weekday, minute) -> Option<u16>`
  - `Timetable { schedules, blackouts }` with `sanitised()`
  - `PauseReason { Battery, Locked, Presenting, Blackout { until } }`, serialised as
    `{"reason": "...", "until"?: n}`, with `holds_power()`
  - `pause_reason(&RunSettings, &[Blackout], &Snapshot) -> Option<PauseReason>`
  - `Cause { Schedule, RunFor }` and `Command { Start(Cause), Stop(Cause) }`
  - `Decision { command, pause }`
  - `RunFor { Forever, Minutes { minutes }, Until { at } }` (serde tag `kind`, snake_case)
  - `deadline_for(RunFor, epoch, minute_now) -> Option<u64>`
  - `Autopilot` with `set_deadline`, `deadline` and `tick(&RunSettings, &Timetable, &Snapshot, running) -> Decision`
  - `RunSettings` gains `pause_on_battery: bool` (false), `pause_when_locked: bool` (true) and
    `pause_when_presenting: bool` (false)

- [ ] **Step 1: Add the pause switches to `RunSettings`** in `core/running.rs`, after
`start_on_launch` in the struct:

```rust
    /// Pause moves and keep-awake while on battery, so the PC can sleep (spec 006 FR-008).
    pub pause_on_battery: bool,
    /// Pause moves while the screen is locked; keep-awake continues (FR-009).
    pub pause_when_locked: bool,
    /// Pause moves while presenting or a full-screen app or game is up; keep-awake continues (FR-027).
    pub pause_when_presenting: bool,
```

In `Default`, set them to `pause_on_battery: false, pause_when_locked: true, pause_when_presenting: false`.

- [ ] **Step 2: Write the module, starting with the failing tests.** Create
`src-tauri/src/core/autopilot.rs` containing only the module doc, the `use` lines and the tests
below. Add `pub mod autopilot;` to `core/mod.rs`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rule::NotifState;

    const EVERY_DAY: [bool; 7] = [true; 7];
    const MONDAY_ONLY: [bool; 7] = [true, false, false, false, false, false, false];

    fn snap(weekday: u8, minutes: u16) -> Snapshot {
        Snapshot { weekday, minutes, epoch_secs: 1_000_050, ..Default::default() }
    }
    fn at(minute: u16, action: ScheduleAction) -> Schedule {
        Schedule { days: EVERY_DAY, at: minute, action, enabled: true }
    }
    fn table(schedules: Vec<Schedule>) -> Timetable {
        Timetable { schedules, blackouts: Vec::new() }
    }
    fn run() -> RunSettings {
        RunSettings::default()
    }
    use ScheduleAction::{Start, Stop};

    #[test]
    fn the_first_tick_fires_nothing() {
        let mut a = Autopilot::default();
        let d = a.tick(&run(), &table(vec![at(540, Start)]), &snap(0, 540), false);
        assert_eq!(d.command, None);
    }

    #[test]
    fn crossing_a_start_time_starts() {
        let mut a = Autopilot::default();
        let t = table(vec![at(540, Start)]);
        a.tick(&run(), &t, &snap(0, 539), false);
        assert_eq!(a.tick(&run(), &t, &snap(0, 540), false).command, Some(Command::Start(Cause::Schedule)));
        assert_eq!(a.tick(&run(), &t, &snap(0, 540), true).command, None, "fires once");
    }

    #[test]
    fn a_start_while_running_and_a_stop_while_stopped_do_nothing() {
        let mut a = Autopilot::default();
        let t = table(vec![at(540, Start), at(600, Stop)]);
        a.tick(&run(), &t, &snap(0, 539), true);
        assert_eq!(a.tick(&run(), &t, &snap(0, 540), true).command, None);
        a.tick(&run(), &t, &snap(0, 599), false);
        assert_eq!(a.tick(&run(), &t, &snap(0, 600), false).command, None);
    }

    /// Review Focus 1: asleep across both edges, the later one wins.
    #[test]
    fn a_sleep_gap_fires_the_latest_edge_crossed() {
        let t = table(vec![at(540, Start), at(560, Stop)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 500), true);
        assert_eq!(a.tick(&run(), &t, &snap(0, 600), true).command, Some(Command::Stop(Cause::Schedule)));

        let t = table(vec![at(520, Stop), at(540, Start)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 500), false);
        assert_eq!(a.tick(&run(), &t, &snap(0, 600), false).command, Some(Command::Start(Cause::Schedule)));
    }

    #[test]
    fn a_start_and_a_stop_at_the_same_minute_stop_wins() {
        let t = table(vec![at(540, Start), at(540, Stop)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 539), true);
        assert_eq!(a.tick(&run(), &t, &snap(0, 540), true).command, Some(Command::Stop(Cause::Schedule)));
    }

    #[test]
    fn edges_from_a_previous_day_never_fire() {
        let t = table(vec![at(1430, Start)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 1420), false); // Monday 23:40
        assert_eq!(a.tick(&run(), &t, &snap(1, 5), false).command, None, "Tuesday 00:05");
    }

    #[test]
    fn crossing_midnight_fires_a_midnight_edge() {
        let t = table(vec![at(0, Start)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 1439), false);
        assert_eq!(a.tick(&run(), &t, &snap(1, 0), false).command, Some(Command::Start(Cause::Schedule)));
    }

    #[test]
    fn days_and_the_enabled_switch_are_respected() {
        let mut only_monday = at(540, Start);
        only_monday.days = MONDAY_ONLY;
        let mut off = at(540, Start);
        off.enabled = false;
        for t in [table(vec![only_monday]), table(vec![off])] {
            let mut a = Autopilot::default();
            a.tick(&run(), &t, &snap(1, 539), false); // Tuesday
            assert_eq!(a.tick(&run(), &t, &snap(1, 540), false).command, None);
        }
    }

    #[test]
    fn the_clock_going_backwards_fires_nothing() {
        let t = table(vec![at(540, Start)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 600), false);
        assert_eq!(a.tick(&run(), &t, &snap(0, 530), false).command, None);
    }

    #[test]
    fn the_run_for_deadline_stops_and_clears() {
        let mut a = Autopilot::default();
        a.set_deadline(Some(1_000_100));
        let mut s = snap(0, 600);
        s.epoch_secs = 1_000_099;
        assert_eq!(a.tick(&run(), &table(vec![]), &s, true).command, None);
        s.epoch_secs = 1_000_100;
        assert_eq!(a.tick(&run(), &table(vec![]), &s, true).command, Some(Command::Stop(Cause::RunFor)));
        assert_eq!(a.deadline(), None);
    }

    #[test]
    fn a_scheduled_stop_clears_the_deadline() {
        let t = table(vec![at(540, Stop)]);
        let mut a = Autopilot::default();
        a.set_deadline(Some(9_999_999));
        a.tick(&run(), &t, &snap(0, 539), true);
        a.tick(&run(), &t, &snap(0, 540), true);
        assert_eq!(a.deadline(), None);
    }

    #[test]
    fn deadline_for_counts_minutes_and_finds_the_next_clock_time() {
        // epoch 1_000_050 is 30 s into a minute; "until" lands on the minute.
        assert_eq!(deadline_for(RunFor::Forever, 1_000_050, 540), None);
        assert_eq!(deadline_for(RunFor::Minutes { minutes: 30 }, 1_000_050, 540), Some(1_001_850));
        assert_eq!(deadline_for(RunFor::Until { at: 600 }, 1_000_050, 540), Some(1_003_620));
        assert_eq!(deadline_for(RunFor::Until { at: 540 }, 1_000_050, 600), Some(1_082_820), "tomorrow");
        assert_eq!(deadline_for(RunFor::Until { at: 600 }, 1_000_050, 600), Some(1_086_420), "same minute: tomorrow");
    }

    #[test]
    fn pauses_follow_their_switches_and_their_priority() {
        let mut s = snap(0, 600);
        s.on_ac = false;
        s.session_locked = true;
        s.notification_state = NotifState::Presentation;
        let all = RunSettings { pause_on_battery: true, pause_when_locked: true, pause_when_presenting: true, ..run() };
        assert_eq!(pause_reason(&all, &[], &s), Some(PauseReason::Battery));
        let no_battery = RunSettings { pause_on_battery: false, ..all };
        assert_eq!(pause_reason(&no_battery, &[], &s), Some(PauseReason::Locked));
        let only_presenting = RunSettings { pause_when_locked: false, ..no_battery };
        assert_eq!(pause_reason(&only_presenting, &[], &s), Some(PauseReason::Presenting));
        let none = RunSettings { pause_when_presenting: false, ..only_presenting };
        assert_eq!(pause_reason(&none, &[], &s), None);
    }

    #[test]
    fn the_default_pauses_are_locked_only() {
        let mut s = snap(0, 600);
        s.on_ac = false;
        s.notification_state = NotifState::Game;
        assert_eq!(pause_reason(&run(), &[], &s), None);
        s.session_locked = true;
        assert_eq!(pause_reason(&run(), &[], &s), Some(PauseReason::Locked));
    }

    #[test]
    fn presenting_means_presentation_full_screen_or_game() {
        let p = RunSettings { pause_when_presenting: true, ..run() };
        for (state, paused) in [
            (NotifState::Presentation, true),
            (NotifState::Busy, true),
            (NotifState::Game, true),
            (NotifState::QuietTime, false),
            (NotifState::Normal, false),
        ] {
            let mut s = snap(0, 600);
            s.notification_state = state;
            assert_eq!(pause_reason(&p, &[], &s).is_some(), paused, "{state:?}");
        }
    }

    #[test]
    fn a_blackout_pauses_inside_its_window_and_says_until_when() {
        let lunch = Blackout { days: EVERY_DAY, from: 750, to: 810, enabled: true };
        assert_eq!(pause_reason(&run(), &[lunch.clone()], &snap(2, 760)), Some(PauseReason::Blackout { until: 810 }));
        assert_eq!(pause_reason(&run(), &[lunch.clone()], &snap(2, 810)), None);
        assert_eq!(pause_reason(&run(), &[lunch], &snap(2, 749)), None);
    }

    #[test]
    fn a_blackout_across_midnight_uses_its_start_day() {
        let night = Blackout { days: MONDAY_ONLY, from: 1320, to: 360, enabled: true }; // Mon 22:00 to 06:00
        assert!(night.active_until(0, 1380).is_some(), "Monday 23:00");
        assert!(night.active_until(1, 300).is_some(), "Tuesday 05:00, Monday's window");
        assert!(night.active_until(1, 360).is_none(), "Tuesday 06:00");
        assert!(night.active_until(1, 1380).is_none(), "Tuesday 23:00, not a Tuesday window");
        assert!(night.active_until(0, 300).is_none(), "Monday 05:00, Sunday's window is off");
    }

    /// Review Focus 3.
    #[test]
    fn an_empty_or_disabled_blackout_never_applies_and_bad_times_are_clamped() {
        let empty = Blackout { days: EVERY_DAY, from: 600, to: 600, enabled: true };
        let off = Blackout { days: EVERY_DAY, from: 0, to: 1439, enabled: false };
        assert!(empty.active_until(0, 600).is_none());
        assert!(off.active_until(0, 600).is_none());
        let t = Timetable {
            schedules: vec![at(5_000, Start)],
            blackouts: vec![Blackout { days: EVERY_DAY, from: 2_000, to: 3_000, enabled: true }],
        }
        .sanitised();
        assert_eq!(t.schedules[0].at, 1439);
        assert_eq!((t.blackouts[0].from, t.blackouts[0].to), (1439, 1439));
    }

    #[test]
    fn no_pause_is_reported_while_stopped_and_battery_alone_releases_power() {
        let mut s = snap(0, 600);
        s.session_locked = true;
        let mut a = Autopilot::default();
        assert_eq!(a.tick(&run(), &table(vec![]), &s, false).pause, None);
        assert_eq!(a.tick(&run(), &table(vec![]), &s, true).pause, Some(PauseReason::Locked));
        assert!(!PauseReason::Battery.holds_power());
        assert!(PauseReason::Locked.holds_power());
        assert!(PauseReason::Presenting.holds_power());
        assert!(PauseReason::Blackout { until: 0 }.holds_power());
    }

    #[test]
    fn pause_reasons_serialise_for_home() {
        assert_eq!(serde_json::to_string(&PauseReason::Battery).unwrap(), r#"{"reason":"battery"}"#);
        assert_eq!(
            serde_json::to_string(&PauseReason::Blackout { until: 810 }).unwrap(),
            r#"{"reason":"blackout","until":810}"#
        );
        let r: RunFor = serde_json::from_str(r#"{"kind":"until","at":1080}"#).unwrap();
        assert_eq!(r, RunFor::Until { at: 1080 });
    }
}
```

- [ ] **Step 3: Run the tests and confirm they fail.**
Run `cargo test --manifest-path src-tauri/Cargo.toml autopilot`. Expected: compile errors (the types
are not defined yet).

- [ ] **Step 4: Implement.** Put this above the tests:

```rust
//! When Start runs and when it pauses (spec 006 FR-007 to FR-013, FR-027). Pure: it is fed the
//! settings, the Run-for deadline, the previous tick's local time and a `Snapshot`. It decides,
//! and the shell acts. No OS code, so every rule here has a unit test.

use serde::{Deserialize, Serialize};

use crate::core::rule::NotifState;
use crate::core::running::RunSettings;
use crate::core::snapshot::Snapshot;

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleAction {
    Start,
    Stop,
}

/// "These days, at this time, Start (or Stop)" (FR-011). `days[0]` is Monday, and `at` is a local
/// minute of the day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub days: [bool; 7],
    pub at: u16,
    pub action: ScheduleAction,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// A quiet window with no mouse moves (FR-010). `from > to` crosses midnight, and `days` name the
/// day a window starts on. `from == to` is empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blackout {
    pub days: [bool; 7],
    pub from: u16,
    pub to: u16,
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl Blackout {
    /// `Some(end)` while this window is active at `weekday` (0 = Monday), `minute`.
    pub fn active_until(&self, weekday: u8, minute: u16) -> Option<u16> {
        if !self.enabled || self.from == self.to {
            return None;
        }
        let today = self.days[weekday as usize % 7];
        let yesterday = self.days[(weekday as usize + 6) % 7];
        let on = if self.from < self.to {
            today && minute >= self.from && minute < self.to
        } else {
            (today && minute >= self.from) || (yesterday && minute < self.to)
        };
        on.then_some(self.to)
    }
}

/// Everything time-based the user set up: schedules and blackouts. Saved as `timetable`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timetable {
    #[serde(default)]
    pub schedules: Vec<Schedule>,
    #[serde(default)]
    pub blackouts: Vec<Blackout>,
}

impl Timetable {
    /// Times past the end of a day are clamped to 23:59, so a hand-edited value is visible and
    /// fixable rather than silently never firing (Review Focus 3).
    pub fn sanitised(mut self) -> Self {
        for s in &mut self.schedules {
            s.at = s.at.min(1439);
        }
        for b in &mut self.blackouts {
            b.from = b.from.min(1439);
            b.to = b.to.min(1439);
        }
        self
    }
}

/// Why moves (and, on battery, keep-awake) are paused. Home shows it, tagged by `reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum PauseReason {
    Battery,
    Locked,
    Presenting,
    Blackout { until: u16 },
}

impl PauseReason {
    /// Only a battery pause lets go of the power request. The others pause moves and keep the PC
    /// awake, so whatever is running can continue and the moves can resume afterwards.
    pub fn holds_power(self) -> bool {
        !matches!(self, PauseReason::Battery)
    }
}

/// The first pause that applies, in priority order: battery, locked, presenting, blackout.
pub fn pause_reason(s: &RunSettings, blackouts: &[Blackout], snap: &Snapshot) -> Option<PauseReason> {
    if s.pause_on_battery && !snap.on_ac {
        return Some(PauseReason::Battery);
    }
    if s.pause_when_locked && snap.session_locked {
        return Some(PauseReason::Locked);
    }
    if s.pause_when_presenting
        && matches!(
            snap.notification_state,
            NotifState::Presentation | NotifState::Busy | NotifState::Game
        )
    {
        return Some(PauseReason::Presenting);
    }
    blackouts
        .iter()
        .find_map(|b| b.active_until(snap.weekday, snap.minutes))
        .map(|until| PauseReason::Blackout { until })
}

/// What made the autopilot act, for the notification text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Schedule,
    RunFor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Start(Cause),
    Stop(Cause),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Decision {
    pub command: Option<Command>,
    pub pause: Option<PauseReason>,
}

/// How long Start runs (FR-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunFor {
    Forever,
    Minutes { minutes: u32 },
    Until { at: u16 },
}

/// The epoch second a Run-for choice ends. "Until 09:00", set at 10:00, means tomorrow. An
/// "until" lands on the start of its minute.
pub fn deadline_for(run_for: RunFor, epoch: u64, minute_now: u16) -> Option<u64> {
    match run_for {
        RunFor::Forever => None,
        RunFor::Minutes { minutes } => Some(epoch + minutes.max(1) as u64 * 60),
        RunFor::Until { at } => {
            let ahead = (at.min(1439) as i64 - minute_now as i64).rem_euclid(1440);
            let ahead = if ahead == 0 { 1440 } else { ahead } as u64;
            Some(epoch - epoch % 60 + ahead * 60)
        }
    }
}

/// Schedule edges and the Run-for deadline, tick by tick.
#[derive(Debug, Default)]
pub struct Autopilot {
    /// The previous tick's local (weekday, minute). `None` before the first tick, so launching
    /// never fires an edge.
    last: Option<(u8, u16)>,
    deadline: Option<u64>,
}

impl Autopilot {
    pub fn set_deadline(&mut self, at: Option<u64>) {
        self.deadline = at;
    }

    pub fn deadline(&self) -> Option<u64> {
        self.deadline
    }

    /// One tick: the command to carry out, if any, and the pause that applies afterwards. A
    /// deadline is checked before schedules; any Stop clears the deadline.
    pub fn tick(&mut self, s: &RunSettings, t: &Timetable, snap: &Snapshot, running: bool) -> Decision {
        let now = (snap.weekday, snap.minutes);
        let mut command = None;
        if running && self.deadline.is_some_and(|d| snap.epoch_secs >= d) {
            command = Some(Command::Stop(Cause::RunFor));
        } else if let Some(prev) = self.last {
            command = match (crossed(&t.schedules, prev, now), running) {
                (Some(ScheduleAction::Start), false) => Some(Command::Start(Cause::Schedule)),
                (Some(ScheduleAction::Stop), true) => Some(Command::Stop(Cause::Schedule)),
                _ => None,
            };
        }
        self.last = Some(now);
        let running_after = match command {
            Some(Command::Start(_)) => true,
            Some(Command::Stop(_)) => {
                self.deadline = None;
                false
            }
            None => running,
        };
        let pause = if running_after { pause_reason(s, &t.blackouts, snap) } else { None };
        Decision { command, pause }
    }
}

/// The schedule edge crossed since the previous tick, today only. The latest time wins, and
/// Stop wins a tie (Review Focus 1). A clock going backwards within the day fires nothing.
fn crossed(schedules: &[Schedule], (pd, pm): (u8, u16), (nd, nm): (u8, u16)) -> Option<ScheduleAction> {
    let lower: i32 = if pd == nd {
        if nm < pm {
            return None;
        }
        pm as i32
    } else {
        -1
    };
    schedules
        .iter()
        .filter(|s| s.enabled && s.days[nd as usize % 7] && s.at as i32 > lower && s.at <= nm)
        .max_by_key(|s| (s.at, s.action == ScheduleAction::Stop))
        .map(|s| s.action)
}
```

Until Task 7 wires it in, nothing outside the tests uses this module. If clippy reports dead
code, add `#![allow(dead_code)] // wired in Task 7` directly after the `//!` block. Task 7 removes
it.

- [ ] **Step 5: Run the tests and confirm they pass.**
Run `cargo test --manifest-path src-tauri/Cargo.toml`, clippy `-D warnings`, and `cargo fmt`.
Expected: all 20 autopilot tests pass, and so does everything else.

- [ ] **Step 6: Commit.**

```bash
git add src-tauri/src/core
git commit -m "feat(M8): core::autopilot: schedules, Run for, and pauses, all pure (FR-007..FR-013, FR-027)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The pause overlay, `core::apps`, and the Paused status

**Files:**
- Modify: `src-tauri/src/core/engine.rs` (manual suspension)
- Modify: `src-tauri/src/core/running.rs` (`apply` takes a pause; `StatusKind::Paused`; `status_kind` takes `paused`)
- Create: `src-tauri/src/core/apps.rs`
- Modify: `src-tauri/src/core/mod.rs` (`pub mod apps;`)
- Modify the call sites:
  - `src-tauri/src/lib.rs`: `running::apply` ×2, `status_kind` ×1
  - `src-tauri/src/ipc/mod.rs`: `status_kind`
  - `src-tauri/src/tray.rs`: a `Paused` match arm

**Interfaces:**
- Consumes: `PauseReason` (Task 4).
- Produces:
  - `Engine::set_manual_suspended(bool)`
  - `running::apply(engine, input, settings, mode, pause: Option<PauseReason>)`
  - `StatusKind::Paused`
  - `running::status_kind(running, move_mouse, blocked, effective, paused: bool)`
  - in `core::apps`: `APPS_RULE_ID = "apps"`, `apps(&Profile) -> Vec<String>`,
    `set_apps(&mut Profile, Vec<String>) -> Vec<String>`,
    `first_running(&Profile, &[String]) -> Option<String>`, and
    `process_only(&Rule) -> Option<&[String]>`

- [ ] **Step 1: Write the failing tests.**

Add to the tests in `core/running.rs`, extending the imports with `use crate::core::autopilot::PauseReason;`:

```rust
    fn engines_with_guard() -> (Engine, InputEngine, MockPowerGuard) {
        let guard = MockPowerGuard::default();
        (
            Engine::new(Arc::new(guard.clone())),
            InputEngine::new(Arc::new(MockInjector::default()), 0),
            guard,
        )
    }

    /// Review Focus 2: a battery pause keeps Start on, but lets go of power and moves.
    #[test]
    fn a_battery_pause_keeps_running_but_releases_power_and_moves() {
        let (mut e, mut i, guard) = engines_with_guard();
        let s = RunSettings::default();
        apply(&mut e, &mut i, &s, s.start_mode(), Some(PauseReason::Battery));
        e.tick(&Default::default());
        assert!(is_running(&e));
        assert!(!i.enabled());
        assert_eq!(guard.held(), None);
    }

    #[test]
    fn a_locked_pause_stops_moves_and_keeps_power() {
        let (mut e, mut i, guard) = engines_with_guard();
        let s = RunSettings::default();
        apply(&mut e, &mut i, &s, s.start_mode(), Some(PauseReason::Locked));
        e.tick(&Default::default());
        assert!(!i.enabled());
        assert_eq!(guard.held(), Some(true), "keep presenting is still held");
    }

    #[test]
    fn resuming_restores_moves_and_power() {
        let (mut e, mut i, guard) = engines_with_guard();
        let s = RunSettings::default();
        apply(&mut e, &mut i, &s, s.start_mode(), Some(PauseReason::Battery));
        e.tick(&Default::default());
        apply(&mut e, &mut i, &s, s.start_mode(), None);
        e.tick(&Default::default());
        assert!(i.enabled());
        assert_eq!(guard.held(), Some(true));
    }

    #[test]
    fn a_pause_shows_as_paused_only_while_running() {
        use StatusKind::*;
        use WakeMode::*;
        assert_eq!(status_kind(true, true, false, KeepPresenting, true), Paused);
        assert_eq!(status_kind(true, false, false, KeepRunning, true), Paused);
        assert_eq!(status_kind(false, true, false, Off, true), Stopped);
    }
```

In the existing `status_says_what_is_true` test, add a fifth argument, `false`, to every
`status_kind` call. In the existing tests that call `apply(...)` with four arguments, add `None`.

Create `src-tauri/src/core/apps.rs` containing only the doc comment, the `use` lines and these
tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn set_apps_trims_dedupes_and_keeps_the_users_order() {
        let mut p = Profile::new("default", "Default");
        let list = set_apps(&mut p, names(&[" msbuild.exe ", "", "chrome.exe", "MSBuild.exe"]));
        assert_eq!(list, names(&["msbuild.exe", "chrome.exe"]));
        assert_eq!(apps(&p), list);
        let rule = p.rules.iter().find(|r| r.id == APPS_RULE_ID).unwrap();
        assert!(rule.enabled);
        assert_eq!(rule.mode, WakeMode::KeepRunning);
        assert_eq!(rule.conditions, vec![Condition::ProcessRunning(list)]);
    }

    #[test]
    fn an_empty_list_removes_the_rule() {
        let mut p = Profile::new("default", "Default");
        set_apps(&mut p, names(&["a.exe"]));
        set_apps(&mut p, Vec::new());
        assert!(p.rules.is_empty());
        assert!(apps(&p).is_empty());
    }

    #[test]
    fn first_running_names_the_first_listed_app_that_is_running() {
        let mut p = Profile::new("default", "Default");
        set_apps(&mut p, names(&["msbuild.exe", "chrome.exe"]));
        assert_eq!(first_running(&p, &names(&["Chrome.exe", "MSBUILD.EXE"])), Some("msbuild.exe".into()));
        assert_eq!(first_running(&p, &names(&["notepad.exe"])), None);
    }

    #[test]
    fn process_only_finds_old_style_app_rules_but_not_the_apps_rule() {
        let r = |id: &str, c: Vec<Condition>| Rule { id: id.into(), name: id.into(), enabled: true, conditions: c, mode: WakeMode::KeepRunning };
        let old = r("x", vec![Condition::ProcessRunning(names(&["a.exe"]))]);
        let mixed = r("y", vec![Condition::ProcessRunning(names(&["a.exe"])), Condition::OnACPower]);
        let ours = r(APPS_RULE_ID, vec![Condition::ProcessRunning(names(&["a.exe"]))]);
        assert_eq!(process_only(&old), Some(&names(&["a.exe"])[..]));
        assert_eq!(process_only(&mixed), None);
        assert_eq!(process_only(&ours), None);
    }
}
```

Add `pub mod apps;` to `core/mod.rs`.

- [ ] **Step 2: Run the tests and confirm they fail.**
Run `cargo test --manifest-path src-tauri/Cargo.toml --lib core::`. Expected: compile errors.

- [ ] **Step 3: Implement.**

In `core/engine.rs`:
- Add the field `manual_suspended: bool,` (initialised `false`).
- Add the method:

```rust
    /// A battery pause (spec 006 FR-012): Start stays on, but its power request lets go. Rules,
    /// such as "keep awake while these apps run", still hold.
    pub fn set_manual_suspended(&mut self, on: bool) {
        self.manual_suspended = on;
    }
```

- In `tick`, replace `let desired = self.manual.max(desired_mode(&self.profile, snap));` with:

```rust
        let manual = if self.manual_suspended { WakeMode::Off } else { self.manual };
        let desired = manual.max(desired_mode(&self.profile, snap));
```

In `core/running.rs`:
- Add `use crate::core::autopilot::PauseReason;`.
- Add a `Paused` variant (doc comment: `/// Running, with moves paused (battery, locked, presenting or a blackout).`) to `StatusKind`.
- Replace `apply` and `status_kind` with:

```rust
/// Set the manual mode and bring both engines into line (spec 005 FR-003, spec 006 FR-012):
/// - input on ⇔ running ∧ Move the mouse ∧ not paused
/// - Start's power request held ⇔ running ∧ not battery-paused
/// A pause never changes whether it is running.
pub fn apply(
    engine: &mut Engine,
    input: &mut InputEngine,
    settings: &RunSettings,
    mode: WakeMode,
    pause: Option<PauseReason>,
) {
    engine.set_manual(mode);
    engine.set_manual_suspended(pause.is_some_and(|p| !p.holds_power()));
    input.set_enabled(mode != WakeMode::Off && settings.move_mouse && pause.is_none());
}

pub fn status_kind(
    running: bool,
    move_mouse: bool,
    blocked: bool,
    effective: WakeMode,
    paused: bool,
) -> StatusKind {
    match (running, paused, move_mouse, blocked) {
        (false, ..) if effective != WakeMode::Off => StatusKind::StoppedButRuleHolds,
        (false, ..) => StatusKind::Stopped,
        (true, true, ..) => StatusKind::Paused,
        (true, false, false, _) => StatusKind::RunningPowerOnly,
        (true, false, true, true) => StatusKind::RunningBlocked,
        (true, false, true, false) => StatusKind::Running,
    }
}
```

- The test-only `set_running` passes `None` as the pause.

Above the tests in `core/apps.rs`, put:

```rust
//! "Keep awake while these apps run" (spec 006 FR-026): the plain-words face of one process rule.

use crate::core::modes::WakeMode;
use crate::core::rule::{Condition, Profile, Rule};

pub const APPS_RULE_ID: &str = "apps";

/// The names in the apps rule, in the order the user added them.
pub fn apps(profile: &Profile) -> Vec<String> {
    profile
        .rules
        .iter()
        .find(|r| r.id == APPS_RULE_ID)
        .and_then(|r| match r.conditions.first() {
            Some(Condition::ProcessRunning(n)) => Some(n.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Replace the list: names trimmed, empty ones dropped, and duplicates in any case removed. The
/// rule exists only while the list is not empty. Returns the list as stored.
pub fn set_apps(profile: &mut Profile, names: Vec<String>) -> Vec<String> {
    let mut list: Vec<String> = Vec::new();
    for n in names {
        let n = n.trim().to_string();
        if !n.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
            list.push(n);
        }
    }
    profile.rules.retain(|r| r.id != APPS_RULE_ID);
    if !list.is_empty() {
        profile.rules.push(Rule {
            id: APPS_RULE_ID.into(),
            name: "Keep awake while these apps run".into(),
            enabled: true,
            conditions: vec![Condition::ProcessRunning(list.clone())],
            mode: WakeMode::KeepRunning,
        });
    }
    list
}

/// The first listed app that is running now, for Home's "Stopped, but msbuild.exe is keeping the
/// PC awake".
pub fn first_running(profile: &Profile, running: &[String]) -> Option<String> {
    apps(profile)
        .into_iter()
        .find(|a| running.iter().any(|r| r.eq_ignore_ascii_case(a)))
}

/// A rule whose only condition is a process list: what config v4 folds into the apps list
/// (FR-031). The apps rule itself is not one.
pub fn process_only(rule: &Rule) -> Option<&[String]> {
    match rule.conditions.as_slice() {
        [Condition::ProcessRunning(n)] if rule.id != APPS_RULE_ID => Some(n),
        _ => None,
    }
}
```

Update the call sites, so that every shell call keeps today's behaviour until Task 7:
- `lib.rs` `run()`: `running::apply(&mut engine, &mut input_engine, &run_settings, initial_mode, None);`
- `lib.rs` `apply_mode`: `running::apply(&mut e, &mut ie, &settings, mode, None);`
- `lib.rs` scheduler: `running::status_kind(on, move_mouse, blocked, effective, false)`
- `ipc/mod.rs` `get_status`: add `false` as the last argument to `status_kind`.
- `tray.rs` `tooltip`: add the arm `StatusKind::Paused => "Paused".to_string(),`

If clippy reports `apps` as dead code, add `#![allow(dead_code)] // wired in Tasks 6 and 9` after
the module doc. Task 9 removes it.

- [ ] **Step 4: Run the tests and confirm they pass.**
Run `cargo test --manifest-path src-tauri/Cargo.toml`, clippy `-D warnings`, and `cargo fmt`.
Expected: all pass.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src
git commit -m "feat(M8): pause overlay, Paused status, and core::apps (FR-012, FR-026)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Config v4

**Files:**
- Modify: `src-tauri/src/config/model.rs` (`Appearance`, `timetable`, `CURRENT_SCHEMA_VERSION = 4`)
- Modify: `src-tauri/src/config/migrate.rs` (v3→v4: timer rule removed, process rules folded, tests)
- Modify: `src-tauri/src/lib.rs` (load, then manage `SharedTimetable` and `SharedAppearance`, and persist them)

**Interfaces:**
- Consumes: `Timetable` (Task 4); `core::apps::{set_apps, apps, process_only}` (Task 5).
- Produces:
  - `config::model::Appearance { always_on_top: bool, taskbar_dot: bool, notifications: bool }`, default `false, true, true`
  - `Config.timetable: Timetable` and `Config.appearance: Appearance`
  - in lib.rs: `pub(crate) type SharedTimetable = Arc<Mutex<Timetable>>` and
    `pub(crate) type SharedAppearance = Arc<Mutex<Appearance>>`, both managed and both saved by
    `persist_current`

- [ ] **Step 1: Write the failing tests.** In `config/migrate.rs` tests:
- Change `loads_current_version` to use `"schema_version": 4`.
- Rename the `migrates_*` assertions that check `schema_version == 3` to expect `4`.
- Add:

```rust
    use crate::core::apps::{apps, APPS_RULE_ID};
    use crate::core::rule::{Condition, Rule};
    use crate::core::modes::WakeMode;

    fn v3_with_rules() -> Value {
        json!({
            "schema_version": 3,
            "input": { "interval_secs": 200, "key": 0, "motion": "Line", "distance_px": 10, "vary_pct": 25 },
            "profiles": [{ "id": "default", "name": "Default", "rules": [
                { "id": "timer", "name": "Keep running for 30 minutes", "enabled": true,
                  "conditions": [{ "ExpiryAt": 1 }], "mode": "KeepRunning" },
                { "id": "b", "name": "while build", "enabled": true,
                  "conditions": [{ "ProcessRunning": ["msbuild.exe"] }], "mode": "KeepRunning" },
                { "id": "c", "name": "off one", "enabled": false,
                  "conditions": [{ "ProcessRunning": ["off.exe"] }], "mode": "KeepRunning" },
                { "id": "d", "name": "on AC", "enabled": true,
                  "conditions": ["OnACPower"], "mode": "KeepPresenting" }
            ]}],
            "active_profile": "default"
        })
    }

    /// SC-006 and SC-011.
    #[test]
    fn v3_to_v4_drops_the_timer_and_folds_process_rules_into_the_apps_list() {
        let cfg = migrate(v3_with_rules()).unwrap();
        assert_eq!(cfg.schema_version, 4);
        let p = cfg.active().unwrap();
        let ids: Vec<&str> = p.rules.iter().map(|r| r.id.as_str()).collect();
        assert!(!ids.contains(&"timer"), "{ids:?}");
        assert!(!ids.contains(&"b"), "the process rule was folded");
        assert!(ids.contains(&"c"), "a disabled rule is kept as it is");
        assert!(ids.contains(&"d"), "a non-process rule is kept as it is");
        assert!(ids.contains(&APPS_RULE_ID));
        assert_eq!(apps(p), vec!["msbuild.exe".to_string()]);
    }

    #[test]
    fn v3_input_keeps_its_values_and_gains_the_new_defaults() {
        let cfg = migrate(v3_with_rules()).unwrap();
        assert_eq!(cfg.input.interval_secs, 200);
        assert_eq!(cfg.input.motion, crate::core::motion::Motion::RightAndLeft);
        assert!(!cfg.input.interval_random && cfg.input.abortable);
        assert_eq!(cfg.appearance, crate::config::model::Appearance::default());
        assert!(cfg.timetable.schedules.is_empty() && cfg.timetable.blackouts.is_empty());
    }

    #[test]
    fn a_v4_timetable_and_appearance_round_trip() {
        let v = json!({
            "schema_version": 4,
            "timetable": {
                "schedules": [{ "days": [true,true,true,true,true,false,false], "at": 540, "action": "Start" }],
                "blackouts": [{ "days": [true,true,true,true,true,true,true], "from": 750, "to": 810, "enabled": false }]
            },
            "appearance": { "always_on_top": true }
        });
        let cfg = migrate(v).unwrap();
        assert_eq!(cfg.timetable.schedules[0].at, 540);
        assert!(cfg.timetable.schedules[0].enabled, "enabled defaults to on");
        assert!(!cfg.timetable.blackouts[0].enabled);
        assert!(cfg.appearance.always_on_top && cfg.appearance.taskbar_dot && cfg.appearance.notifications);
    }

    #[test]
    fn a_v4_file_is_not_re_folded() {
        let mut v = v3_with_rules();
        v["schema_version"] = json!(4);
        let cfg = migrate(v).unwrap();
        let ids: Vec<&str> = cfg.active().unwrap().rules.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"timer") && ids.contains(&"b"), "v4 files are taken as they are");
    }
```

Remove any unused imports, such as `Rule`, `Condition` or `WakeMode`, if clippy complains.

- [ ] **Step 2: Run the tests and confirm they fail.**
Run `cargo test --manifest-path src-tauri/Cargo.toml migrate`. Expected: compile errors
(`appearance`, `timetable`).

- [ ] **Step 3: Implement.**

In `config/model.rs`:
- Set `CURRENT_SCHEMA_VERSION = 4`.
- Add `use crate::core::autopilot::Timetable;`.
- Add this struct:

```rust
/// Window and tray extras (spec 006 FR-015 to FR-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub always_on_top: bool,
    /// The green or yellow dot on the taskbar button while the window is open.
    pub taskbar_dot: bool,
    /// Toasts for things the user did not do by hand.
    pub notifications: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self { always_on_top: false, taskbar_dot: true, notifications: true }
    }
}
```

- Add to `Config`, after `run`:

```rust
    /// Schedules and blackouts (spec 006).
    #[serde(default)]
    pub timetable: Timetable,
    #[serde(default)]
    pub appearance: Appearance,
```

- Add `timetable: Timetable::default(), appearance: Appearance::default(),` to `Config::default()`.
- Update the module doc to say "v4 (spec 006) adds the timetable and appearance and folds old
  process rules into the apps list."

In `config/migrate.rs`, restructure the `match`:

```rust
    match version {
        0..=3 => {
            if version <= 2 {
                // (the existing v0-2 body up to, not including, the final from_value:
                //  the input reset, and the removal of mode/input_enabled)
            }
            value["schema_version"] = json!(CURRENT_SCHEMA_VERSION);
            let mut cfg: Config = serde_json::from_value(value)
                .map_err(|e| format!("migrate v{version}→v{CURRENT_SCHEMA_VERSION}: {e}"))?;
            upgrade_rules(&mut cfg.profiles);
            Ok(cfg)
        }
        4 => serde_json::from_value(value).map_err(|e| format!("parse v4: {e}")),
        v => Err(format!(
            "config schema v{v} is newer than supported v{CURRENT_SCHEMA_VERSION}; refusing to load"
        )),
    }
```

Add this helper:

```rust
/// v3 → v4 (spec 006 FR-019, FR-031): drop the Advanced timer rule, and fold every enabled
/// process-only rule into the plain apps list. Every other rule stays exactly as it was
/// (constitution VI); About ▸ Troubleshooting lists them.
fn upgrade_rules(profiles: &mut [Profile]) {
    for p in profiles {
        p.rules.retain(|r| r.id != "timer");
        let mut names = apps::apps(p);
        let mut folded: Vec<String> = Vec::new();
        for r in &p.rules {
            if let Some(n) = apps::process_only(r).filter(|_| r.enabled) {
                names.extend(n.iter().cloned());
                folded.push(r.id.clone());
            }
        }
        if !folded.is_empty() {
            p.rules.retain(|r| !folded.contains(&r.id));
            apps::set_apps(p, names);
        }
    }
}
```

Add the imports `use crate::core::apps;` and `use crate::core::rule::Profile;`. Update the
module doc's version list.

In `lib.rs`:
- Add the imports `use crate::config::model::Appearance;` and
  `use crate::core::autopilot::Timetable;`.
- Add the aliases:

```rust
/// Schedules and blackouts (spec 006).
pub(crate) type SharedTimetable = Arc<Mutex<Timetable>>;
/// Always on top, the taskbar dot, notifications.
pub(crate) type SharedAppearance = Arc<Mutex<Appearance>>;
```

- In `run()`, extend the load tuple with two locals, `timetable_value` (from
  `c.timetable.sanitised()`) and `appearance_value` (from `c.appearance`). On error, use
  `Timetable::default()` and `Appearance::default()`.
- After the engine and input engine are built, create
  `let timetable: SharedTimetable = Arc::new(Mutex::new(timetable_value.clone()));` and
  `let appearance: SharedAppearance = Arc::new(Mutex::new(appearance_value));`. Task 7 reads
  `timetable_value` once more, before it is wrapped.
- Add `.manage(timetable.clone())` and `.manage(appearance.clone())`.
- In `persist_current`, read `let timetable = app.state::<SharedTimetable>().lock().unwrap().clone();`
  and `let appearance = *app.state::<SharedAppearance>().lock().unwrap();`, and put both into the
  `Config { … }`.

- [ ] **Step 4: Run the tests and confirm they pass.**
Run `cargo test --manifest-path src-tauri/Cargo.toml`, clippy `-D warnings`, and `cargo fmt`.
Expected: all pass, store tests included.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src
git commit -m "feat(M8): config v4: timetable, appearance; old app rules fold into the apps list (FR-019, FR-031)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Shell: the autopilot drives Start, Stop and pauses; Run for; timetable IPC

**Files:**
- Modify: `src-tauri/src/lib.rs` (`SharedAutopilot`, `apply_mode` with pauses, `start_for`, `set_run_for`, `set_timetable`, the scheduler loop)
- Modify: `src-tauri/src/ipc/mod.rs` (`start(run_for)`, `set_run_for`, `get_timetable`, `set_timetable`, `Status` gains `pause`, `stops_at` and `idle_secs`)
- Modify: `src-tauri/src/tray.rs` (`tooltip` takes the pause; paused text)
- Modify: `src-tauri/src/core/autopilot.rs` (remove the temporary dead-code allow, if Task 4 added one)

**Interfaces:**
- Consumes: `Autopilot`, `pause_reason`, `deadline_for`, `RunFor`, `Command`, `Timetable` and
  `PauseReason` (Task 4); `running::apply(.., pause)` and `status_kind(.., paused)` (Task 5);
  `SharedTimetable` (Task 6).
- Produces:
  - `pub(crate) type SharedAutopilot = Arc<Mutex<Autopilot>>`, managed
  - in lib.rs: `pub(crate) fn start_for(&AppHandle, RunFor)`,
    `pub(crate) fn set_run_for(&AppHandle, RunFor)` and
    `pub(crate) fn set_timetable(&AppHandle, Timetable) -> Timetable`
  - IPC `start({ runFor? })`, `set_run_for({ runFor })`, `get_timetable` and
    `set_timetable({ timetable })`
  - `Status { kind, running, next_move_in_secs, keep_screen_on, pause: Option<PauseReason>, stops_at: Option<u64>, idle_secs: u32 }`
  - `tray::tooltip(kind, next, pause: Option<PauseReason>, rule_left, update)`

The scheduler and IPC changes are Tauri glue over the pure, already-tested autopilot. The test
here is the tray tooltip's new paused text. The rest is checked in Task 15.

- [ ] **Step 1: Write the failing test.** In `tray.rs` tests:
- Add a third argument, `None`, to every existing `tooltip(...)` call.
- Add:

```rust
    #[test]
    fn a_paused_tooltip_says_why() {
        use crate::core::autopilot::PauseReason;
        assert_eq!(
            tooltip(StatusKind::Paused, None, Some(PauseReason::Battery), None, None),
            "project-mouse: Paused · on battery"
        );
        assert_eq!(
            tooltip(StatusKind::Paused, None, Some(PauseReason::Blackout { until: 810 }), None, None),
            "project-mouse: Paused · blackout until 13:30"
        );
    }
```

- Also extend `every_tooltip_fits` with
  `tooltip(StatusKind::Paused, None, Some(PauseReason::Blackout { until: 1439 }), Some(86_399), Some("10.10.10"))`.

- [ ] **Step 2: Run the test and confirm it fails.**
Run `cargo test --manifest-path src-tauri/Cargo.toml tray`. Expected: compile error (argument count).

- [ ] **Step 3: Implement.**

In `tray.rs`:
- Add `use crate::core::autopilot::PauseReason;`.
- Add this helper:

```rust
/// `HH:MM` for a minute of the day.
fn hhmm(m: u16) -> String {
    format!("{:02}:{:02}", m / 60 % 24, m % 60)
}
```

- Change the `tooltip` signature to
  `pub fn tooltip(kind: StatusKind, next_move_secs: Option<u32>, pause: Option<PauseReason>, rule_left_secs: Option<u64>, update: Option<&str>) -> String`.
- Replace the `Paused` arm with:

```rust
        StatusKind::Paused => match pause {
            Some(PauseReason::Battery) => "Paused · on battery".to_string(),
            Some(PauseReason::Locked) => "Paused · screen locked".to_string(),
            Some(PauseReason::Presenting) => "Paused · presenting".to_string(),
            Some(PauseReason::Blackout { until }) => format!("Paused · blackout until {}", hhmm(until)),
            None => "Paused".to_string(),
        },
```

In `lib.rs`:
- Add the imports `use crate::core::autopilot::{self, Autopilot, Command, RunFor};`.
- Add the alias:

```rust
/// Schedule edges and the Run-for deadline (spec 006).
pub(crate) type SharedAutopilot = Arc<Mutex<Autopilot>>;
```

- Create `let auto: SharedAutopilot = Arc::new(Mutex::new(Autopilot::default()));` in `run()`,
  `.manage(auto.clone())`, and in `setup` clone it as `sched_auto` (plus
  `let sched_tt = timetable.clone();`).
- Replace `apply_mode` with:

```rust
/// Every start and stop goes through here (spec 005 FR-003). Set the manual mode, overlay
/// whatever pause applies right now (spec 006 FR-012), and reconcile power at once rather than on
/// the next tick. Stopping clears a Run-for deadline. Then tell the tray and any open window.
/// Lock order: run → timetable → autopilot → engine → input, never two at once except the last
/// pair.
pub(crate) fn apply_mode(app: &tauri::AppHandle, mode: WakeMode) {
    let settings = *app.state::<SharedRun>().lock().unwrap();
    let snap = app.state::<Arc<Sampler>>().last();
    let pause = if mode == WakeMode::Off {
        app.state::<SharedAutopilot>().lock().unwrap().set_deadline(None);
        None
    } else {
        let tt = app.state::<SharedTimetable>();
        let tt = tt.lock().unwrap();
        autopilot::pause_reason(&settings, &tt.blackouts, &snap)
    };
    {
        let engine = app.state::<SharedEngine>();
        let input = app.state::<SharedInput>();
        let mut e = engine.lock().unwrap();
        let mut ie = input.lock().unwrap();
        running::apply(&mut e, &mut ie, &settings, mode, pause);
        e.tick(&snap);
    }
    after_change(app);
}

/// Start, and end by itself after `run_for` (spec 006 FR-007).
pub(crate) fn start_for(app: &tauri::AppHandle, run_for: RunFor) {
    set_running(app, true);
    set_run_for(app, run_for);
}

/// Change when a running Start ends. Nothing happens while stopped.
pub(crate) fn set_run_for(app: &tauri::AppHandle, run_for: RunFor) {
    if !is_running(app) {
        return;
    }
    let snap = app.state::<Arc<Sampler>>().last();
    let deadline = autopilot::deadline_for(run_for, snap.epoch_secs, snap.minutes);
    app.state::<SharedAutopilot>().lock().unwrap().set_deadline(deadline);
    after_change(app);
}

/// New schedules and blackouts take effect at once and are saved. Returns what was stored.
pub(crate) fn set_timetable(app: &tauri::AppHandle, t: Timetable) -> Timetable {
    let t = t.sanitised();
    *app.state::<SharedTimetable>().lock().unwrap() = t.clone();
    let mode = app.state::<SharedEngine>().lock().unwrap().manual();
    apply_mode(app, mode);
    persist_current(app);
    t
}
```

- In `run()`, a launch that starts running (`--keep`, or Start on launch) must honour a pause
  that already applies. Move the `let sampler = Arc::new(Sampler::new(...));` statement up so
  that it comes before `running::apply`. Then replace that `running::apply(...)` call with:

```rust
    let startup_pause = if initial_mode == WakeMode::Off {
        None
    } else {
        autopilot::pause_reason(&run_settings, &timetable_value.blackouts, &sampler.snapshot())
    };
    running::apply(&mut engine, &mut input_engine, &run_settings, initial_mode, startup_pause);
```

  `timetable_value` is the loaded, sanitised `Timetable` local from Task 6.
- Replace the scheduler closure body from `let snap = …` down to (not including) `true`:

```rust
                    let snap = sched_sampler.snapshot();
                    // Phase 0 (spec 006): schedules and Run for may start or stop; pauses overlay.
                    let was_on = running::is_running(&sched_engine.lock().unwrap());
                    let decision = {
                        let run = *sched_run.lock().unwrap();
                        let tt = sched_tt.lock().unwrap().clone();
                        sched_auto.lock().unwrap().tick(&run, &tt, &snap, was_on)
                    };
                    match decision.command {
                        Some(Command::Start(_)) => set_running(&sched_app, true),
                        Some(Command::Stop(_)) => set_running(&sched_app, false),
                        None if decision.pause != last_pause => {
                            let mode = sched_engine.lock().unwrap().manual();
                            apply_mode(&sched_app, mode);
                        }
                        None => {}
                    }
                    last_pause = decision.pause;
                    // Phase 1: reconcile the power engine against desired state.
                    let (on, effective, remaining) = {
                        let mut e = sched_engine.lock().unwrap();
                        e.tick(&snap);
                        (
                            running::is_running(&e),
                            e.mode(),
                            soonest_expiry_secs(e.profile(), &snap),
                        )
                    };
                    // Phase 2: the input engine (off unless running with moves on and not paused).
                    let (blocked, next) = {
                        let mut ie = sched_input.lock().unwrap();
                        ie.tick(platform::last_input_tick(), platform::tick_now());
                        (ie.enabled() && ie.blocked, ie.next_move_in_secs())
                    };
                    let move_mouse = sched_run.lock().unwrap().move_mouse;
                    let pause = if on { decision.pause } else { None };
                    let kind = running::status_kind(on, move_mouse, blocked, effective, pause.is_some());
                    let tip = tray::tooltip(kind, next, pause, remaining, update_available().as_deref());
```

  Declare `let mut last_pause = None;` next to `let mut last_tip = String::new();`. The rest of the
  closure (pushing the tooltip, emitting) stays as it is.

In `ipc/mod.rs`:
- Add the imports `use crate::core::autopilot::{self, PauseReason, RunFor, Timetable};` and
  `use crate::sampler::Sampler;`.
- Add the aliases
  `type SharedTimetable = Arc<Mutex<Timetable>>; type SharedAutopilot = Arc<Mutex<crate::core::autopilot::Autopilot>>;`.
- Extend `Status`:

```rust
    /// Why moves are paused, while running and paused (spec 006 FR-014).
    pub pause: Option<PauseReason>,
    /// When a Run-for ends (epoch seconds), while running.
    pub stops_at: Option<u64>,
    /// Seconds since the last input of any kind (spec 006 FR-018).
    pub idle_secs: u32,
```

- Replace `get_status` and `start`, and add the timetable commands:

```rust
#[tauri::command]
pub fn get_status(
    engine: State<'_, SharedEngine>,
    input: State<'_, SharedInput>,
    run: State<'_, SharedRun>,
    timetable: State<'_, SharedTimetable>,
    auto: State<'_, SharedAutopilot>,
    sampler: State<'_, Arc<Sampler>>,
) -> Status {
    let settings = *run.lock().unwrap();
    let snap = sampler.last();
    let (on, effective) = {
        let e = engine.lock().unwrap();
        (running::is_running(&e), e.mode())
    };
    let pause = if on {
        autopilot::pause_reason(&settings, &timetable.lock().unwrap().blackouts, &snap)
    } else {
        None
    };
    let (blocked, next_move_in_secs, idle_ms) = {
        let ie = input.lock().unwrap();
        (ie.enabled() && ie.blocked, ie.next_move_in_secs(), ie.system_idle_ms)
    };
    let stops_at = if on { auto.lock().unwrap().deadline() } else { None };
    Status {
        kind: running::status_kind(on, settings.move_mouse, blocked, effective, pause.is_some()),
        running: on,
        next_move_in_secs,
        keep_screen_on: settings.keep_screen_on,
        pause,
        stops_at,
        idle_secs: idle_ms / 1000,
    }
}

/// Start, optionally for a while (spec 006 FR-007). JS sends `{ runFor }`.
#[tauri::command]
pub fn start(app: AppHandle, run_for: Option<RunFor>) {
    crate::start_for(&app, run_for.unwrap_or(RunFor::Forever));
}

#[tauri::command]
pub fn set_run_for(app: AppHandle, run_for: RunFor) {
    crate::set_run_for(&app, run_for);
}

#[tauri::command]
pub fn get_timetable(timetable: State<'_, SharedTimetable>) -> Timetable {
    timetable.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_timetable(app: AppHandle, timetable: Timetable) -> Timetable {
    crate::set_timetable(&app, timetable)
}
```

- Add `ipc::set_run_for`, `ipc::get_timetable` and `ipc::set_timetable` to
  `generate_handler![…]`.
- Remove Task 4's `#![allow(dead_code)]` from `core/autopilot.rs`, if it is there.

- [ ] **Step 4: Run the tests and confirm they pass.**
Run `cargo test --manifest-path src-tauri/Cargo.toml`, clippy `-D warnings`, and `cargo fmt`.
Expected: all clean.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src
git commit -m "feat(M8): the autopilot drives Start, Stop and pauses; Run for; timetable IPC (FR-007..FR-014)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Appearance: notifications, the taskbar dot, always on top

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `tauri-plugin-notification = "2"`)
- Create: `src-tauri/src/appearance.rs`
- Modify: `src-tauri/src/lib.rs`:
  - `mod appearance;` and the plugin
  - `Dots` managed
  - `after_change` and `open_window` sync the window
  - scheduler notifications
  - `set_appearance`
- Modify: `src-tauri/src/ipc/mod.rs` (`get_appearance`, `set_appearance`)

**Interfaces:**
- Consumes: `Command`/`Cause` (Task 4); `SharedAppearance` (Task 6).
- Produces:
  - `appearance::dot([u8; 3]) -> Vec<u8>`, `appearance::Dots::new()`,
    `appearance::sync_window(&AppHandle, running: bool, paused: bool)`,
    `appearance::notify(&AppHandle, &str)` and `appearance::command_text(Command) -> &'static str`
  - IPC `get_appearance` and `set_appearance({ appearance })`

- [ ] **Step 1: Write the failing tests.** Create `src-tauri/src/appearance.rs` containing only
its doc comment, its `use` lines and:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::autopilot::{Cause, Command};

    #[test]
    fn a_dot_is_an_opaque_circle_on_a_clear_square() {
        let px = dot([10, 20, 30]);
        assert_eq!(px.len(), (SIZE * SIZE * 4) as usize);
        let at = |x: u32, y: u32| &px[((y * SIZE + x) * 4) as usize..((y * SIZE + x) * 4 + 4) as usize];
        assert_eq!(at(8, 8), &[10, 20, 30, 255], "the centre is filled");
        assert_eq!(at(0, 0)[3], 0, "a corner is clear");
    }

    #[test]
    fn every_command_has_plain_words() {
        for c in [
            Command::Start(Cause::Schedule),
            Command::Stop(Cause::Schedule),
            Command::Stop(Cause::RunFor),
            Command::Start(Cause::RunFor),
        ] {
            let t = command_text(c);
            assert!(!t.is_empty() && !t.contains('\u{2014}'), "{t}");
        }
    }
}
```

Add `mod appearance;` to `lib.rs`.

- [ ] **Step 2: Run the tests and confirm they fail.**
Run `cargo test --manifest-path src-tauri/Cargo.toml appearance`. Expected: compile errors.

- [ ] **Step 3: Implement.** In `Cargo.toml` `[dependencies]`, add
`tauri-plugin-notification = "2"`. Above the tests in `appearance.rs`, put:

```rust
//! Window and tray extras (spec 006 FR-015 to FR-017): always on top, the taskbar dot and
//! notifications. Shell code: it talks to Tauri, so it lives outside `core`.

use tauri::image::Image;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::core::autopilot::{Cause, Command};
use crate::SharedAppearance;

const SIZE: u32 = 16;

/// A filled circle in `rgb` on a clear 16×16 square: the taskbar dot.
pub fn dot(rgb: [u8; 3]) -> Vec<u8> {
    let c = (SIZE as f32 - 1.0) / 2.0;
    let r = SIZE as f32 / 2.0 - 1.0;
    (0..SIZE * SIZE)
        .flat_map(|i| {
            let (x, y) = ((i % SIZE) as f32 - c, (i / SIZE) as f32 - c);
            let alpha = if x * x + y * y <= r * r { 255 } else { 0 };
            [rgb[0], rgb[1], rgb[2], alpha]
        })
        .collect()
}

/// The two dots, made once at startup.
pub struct Dots {
    running: Image<'static>,
    paused: Image<'static>,
}

impl Dots {
    pub fn new() -> Self {
        Self {
            running: Image::new_owned(dot([0x2e, 0xa0, 0x43]), SIZE, SIZE),
            paused: Image::new_owned(dot([0xd2, 0x99, 0x22]), SIZE, SIZE),
        }
    }
}

/// Bring an open window in line: always on top, and the dot for running, paused or stopped.
pub fn sync_window(app: &AppHandle, running: bool, paused: bool) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let a = *app.state::<SharedAppearance>().lock().unwrap();
    let _ = w.set_always_on_top(a.always_on_top);
    let icon = match (a.taskbar_dot, running, app.try_state::<Dots>()) {
        (true, true, Some(d)) => Some(if paused { d.paused.clone() } else { d.running.clone() }),
        _ => None,
    };
    let _ = w.set_overlay_icon(icon);
}

/// A toast, if notifications are on. Windows may still suppress it (Focus Assist), and a
/// failure is ignored: nothing else depends on it (Review Focus 5).
pub fn notify(app: &AppHandle, body: &str) {
    if !app.state::<SharedAppearance>().lock().unwrap().notifications {
        return;
    }
    let _ = app
        .notification()
        .builder()
        .title("project-mouse")
        .body(body)
        .show();
}

/// What a notification says when the autopilot acts.
pub fn command_text(cmd: Command) -> &'static str {
    match cmd {
        Command::Start(Cause::Schedule) => "Started by your schedule.",
        Command::Stop(Cause::Schedule) => "Stopped by your schedule.",
        Command::Stop(Cause::RunFor) => "Run for is over, so it stopped.",
        Command::Start(Cause::RunFor) => "Started.",
    }
}
```

In `lib.rs`:
- Register `.plugin(tauri_plugin_notification::init())` next to the other plugins.
- In `setup`, `app.manage(appearance::Dots::new());`.
- Add:

```rust
/// Whether it is running and, if so, paused: what the taskbar dot shows.
fn window_state(app: &tauri::AppHandle) -> (bool, bool) {
    let running = is_running(app);
    if !running {
        return (false, false);
    }
    let settings = *app.state::<SharedRun>().lock().unwrap();
    let snap = app.state::<Arc<Sampler>>().last();
    let tt = app.state::<SharedTimetable>();
    let paused = autopilot::pause_reason(&settings, &tt.lock().unwrap().blackouts, &snap).is_some();
    (true, paused)
}

/// New appearance settings take effect at once and are saved.
pub(crate) fn set_appearance(app: &tauri::AppHandle, a: Appearance) {
    *app.state::<SharedAppearance>().lock().unwrap() = a;
    let (running, paused) = window_state(app);
    appearance::sync_window(app, running, paused);
    persist_current(app);
}
```

- In `after_change`, after `tray::sync(app);`, add
  `let (running, paused) = window_state(app); appearance::sync_window(app, running, paused);`.
- In `open_window`, after `let _ = b.build();`, add the same two lines, so a fresh window starts
  correct.
- In the scheduler:
  - After the `match decision.command { … }` block, add
    `if let Some(cmd) = decision.command { appearance::notify(&sched_app, appearance::command_text(cmd)); }`.
  - Track the first blocked move: declare `let mut blocked_told = false;` with the other loop
    state, and after `blocked` is known:

```rust
                    if blocked && !blocked_told {
                        appearance::notify(
                            &sched_app,
                            "Windows blocked a mouse move. An app running as administrator is in front; click another window.",
                        );
                        blocked_told = true;
                    }
                    if !on {
                        blocked_told = false;
                    }
```

In `ipc/mod.rs`:
- Add `use crate::config::model::Appearance;` and `type SharedAppearance = Arc<Mutex<Appearance>>;`.
- Add:

```rust
#[tauri::command]
pub fn get_appearance(appearance: State<'_, SharedAppearance>) -> Appearance {
    *appearance.lock().unwrap()
}

#[tauri::command]
pub fn set_appearance(app: AppHandle, appearance: Appearance) {
    crate::set_appearance(&app, appearance);
}
```

- Register both in `generate_handler!`.

- [ ] **Step 4: Run the tests and confirm they pass.**
Run `cargo test --manifest-path src-tauri/Cargo.toml` (the first run downloads the plugin), then
clippy `-D warnings` and `cargo fmt`. Expected: all clean.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src
git commit -m "feat(M8): notifications, the taskbar dot, always on top (FR-015..FR-017)

Adds tauri-plugin-notification, used from Rust only. Toasts only for what the
user did not do by hand: a schedule, a Run for deadline, a blocked move.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Apps list IPC, running apps, Home's "kept awake by", and IPC cleanup

**Files:**
- Modify: `src-tauri/src/lib.rs` (`set_apps`; `set_autostart` and `toggle_autostart` emit `state:changed`)
- Modify: `src-tauri/src/ipc/mod.rs`:
  - add `get_apps`, `set_apps` and `list_running_apps`
  - add `Status.holding_app`
  - remove `list_profiles`, `set_profile`, `create_profile`, `delete_profile`, `upsert_rule` and `ProfileSummary`
- Modify: `src-tauri/src/core/apps.rs` (remove Task 5's dead-code allow, if present)

**Interfaces:**
- Consumes: `core::apps` (Task 5).
- Produces:
  - IPC `get_apps() -> Vec<String>`, `set_apps({ names }) -> Vec<String>` and
    `list_running_apps() -> Vec<String>`
  - `Status.holding_app: Option<String>`
- Kept for About ▸ Troubleshooting: `get_rules`, `set_rule_enabled`, `delete_rule`,
  `get_diagnostics`, `why_awake`, `get_logs` and `import_move_mouse`.

- [ ] **Step 1: Implement.** In `lib.rs`:

```rust
/// Replace "keep awake while these apps run" (spec 006 FR-026), take effect at once, and save.
pub(crate) fn set_apps(app: &tauri::AppHandle, names: Vec<String>) -> Vec<String> {
    let list = {
        let engine = app.state::<SharedEngine>();
        let mut e = engine.lock().unwrap();
        let mut profile = e.profile().clone();
        let list = core::apps::set_apps(&mut profile, names);
        e.set_profile(profile);
        list
    };
    persist_current(app);
    after_change(app);
    list
}
```

In `set_autostart`, after `tray::sync(app);`, add
`if app.get_webview_window("main").is_some() { let _ = app.emit("state:changed", ()); }`. That
keeps the Behaviour switch in step with the tray (M7 final review #9).

In `ipc/mod.rs`:
- Delete `ProfileSummary`, `list_profiles`, `set_profile`, `create_profile`, `delete_profile` and
  `upsert_rule`, and remove them from `generate_handler!` in lib.rs.
- Remove any `profiles` imports that become unused.
- Add `holding_app` to `Status`, with the doc
  `/// While stopped: the first listed app that is keeping the PC awake (FR-026).`
- Compute it in `get_status` inside the engine block:

```rust
    let (on, effective, holding_app) = {
        let e = engine.lock().unwrap();
        let on = running::is_running(&e);
        let holding = (!on && e.mode() != WakeMode::Off)
            .then(|| crate::core::apps::first_running(e.profile(), &snap.running_processes))
            .flatten();
        (on, e.mode(), holding)
    };
```

  Put it in the returned struct. Move `let snap = sampler.last();` above this block.
- Add:

```rust
#[tauri::command]
pub fn get_apps(engine: State<'_, SharedEngine>) -> Vec<String> {
    crate::core::apps::apps(engine.lock().unwrap().profile())
}

#[tauri::command]
pub fn set_apps(app: AppHandle, names: Vec<String>) -> Vec<String> {
    crate::set_apps(&app, names)
}

/// Running executables, for the "add an app" suggestions: sorted, one entry per name.
#[tauri::command]
pub fn list_running_apps(sampler: State<'_, Arc<Sampler>>) -> Vec<String> {
    let mut names: Vec<String> = sampler
        .last()
        .running_processes
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".exe"))
        .collect();
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}
```

- Register `get_apps`, `set_apps` and `list_running_apps` in `generate_handler!`.
- Remove Task 5's `#![allow(dead_code)]` from `core/apps.rs`, if it is there.

- [ ] **Step 2: Run the gates.**
Run `cargo test --manifest-path src-tauri/Cargo.toml`, clippy `-D warnings`, and `cargo fmt`.
Expected: all clean.

- [ ] **Step 3: Commit.**

```bash
git add src-tauri/src
git commit -m "feat(M8): apps list IPC, running apps, 'kept awake by' on Home; drop profile IPC (FR-026, FR-028)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: UI: seven-tab shell, icons, the "?" help, Movement tab, and Home

**Files:**
- Rewrite: `src/types.ts`
- Create: `src/icons.tsx`, `src/movement.tsx`
- Modify: `src/controls.tsx` (`SettingRow` gets `hint` and `help`; adds `DaysPicker` and `TimeField`)
- Rewrite: `src/home.tsx`, `src/App.tsx`
- Modify: `src/settings.tsx`:
  - delete the Distance, Vary by and What to send rows
  - rename every `note=` on `SettingRow` to `hint=`
- Modify: `src/updates.tsx` (rename `note=` to `hint=` on `SettingRow`)
- Modify: `src/styles.css`

There is no TS test runner, and none may be added. The gates are tsc, the vite build and the
greps. Behaviour is checked in Task 15.

**Interfaces:**
- Consumes these IPC commands:
  - `get_status` (with `pause`, `stops_at`, `idle_secs` and `holding_app`)
  - `start({ runFor })`, `set_run_for({ runFor })`, `stop`, `test_move`
  - `get_input_settings` / `set_input_settings({ settings })`
  - `get_run_settings`
- Produces:
  - `App` with `type Page` and `PAGES`, which later tasks extend
  - `Home` with props `{ go: (p: "movement" | "behaviour") => void }`
  - in `controls.tsx`: `SettingRow { title, hint?, help?, children }`,
    `DaysPicker { days, onChange }` and `TimeField { minutes, onChange, label }`
  - `Icon { name }`

- [ ] **Step 1: `src/types.ts`.** Replace the whole file with:

```ts
// Mirrors of the Rust IPC types: ipc/mod.rs (Status), core/running.rs (RunSettings, StatusKind),
// core/input_engine.rs (InputSettings), core/motion.rs (Motion, Speed), core/autopilot.rs
// (PauseReason, RunFor, Schedule, Blackout, Timetable), config/model.rs (Appearance).
export type StatusKind =
  | "stopped"
  | "stopped_but_rule_holds"
  | "running"
  | "running_blocked"
  | "running_power_only"
  | "paused";

export type PauseReason =
  | { reason: "battery" }
  | { reason: "locked" }
  | { reason: "presenting" }
  | { reason: "blackout"; until: number };

export type Status = {
  kind: StatusKind;
  running: boolean;
  next_move_in_secs: number | null;
  keep_screen_on: boolean;
  pause: PauseReason | null;
  stops_at: number | null;
  idle_secs: number;
  holding_app: string | null;
};

export type RunSettings = {
  move_mouse: boolean;
  keep_screen_on: boolean;
  start_on_launch: boolean;
  pause_on_battery: boolean;
  pause_when_locked: boolean;
  pause_when_presenting: boolean;
};

export type Motion =
  | "Virtual" | "Square" | "Circle"
  | "RightAndLeft" | "LeftAndRight" | "UpAndDown" | "DownAndUp"
  | "North" | "NorthEast" | "East" | "SouthEast" | "South" | "SouthWest" | "West" | "NorthWest"
  | "Random";

export type Speed = "Slow" | "Normal" | "Fast" | "Custom";

export type InputSettings = {
  interval_secs: number;
  interval_random: boolean;
  interval_max_secs: number;
  key: number;
  motion: Motion;
  distance_px: number;
  distance_random: boolean;
  distance_max_px: number;
  speed: Speed;
  custom_step_ms: number;
  abortable: boolean;
};

export type RunFor = { kind: "forever" } | { kind: "minutes"; minutes: number } | { kind: "until"; at: number };

export type Schedule = { days: boolean[]; at: number; action: "Start" | "Stop"; enabled: boolean };
export type Blackout = { days: boolean[]; from: number; to: number; enabled: boolean };
export type Timetable = { schedules: Schedule[]; blackouts: Blackout[] };
export type Appearance = { always_on_top: boolean; taskbar_dot: boolean; notifications: boolean };

/** Virtual-key codes worth offering. 0 means "move the mouse". F15 is the category's convention
 *  (Caffeine), and it is also the one that breaks in PuTTY, PowerPoint and Google Docs, which is
 *  why the choice is the user's. */
export const KEYS: [number, string][] = [
  [0, "Mouse movement"],
  [0x7e, "F15 key press"],
  [0x91, "Scroll Lock key press"],
  [0x10, "Shift key press"],
];

/** Move Mouse's full Direction list, grouped (spec 006 FR-001). */
export const DIRECTIONS: { group: string; items: [Motion, string][] }[] = [
  { group: "Shapes", items: [["Square", "Small square"], ["Circle", "Small circle"]] },
  {
    group: "Back and forth",
    items: [
      ["RightAndLeft", "Right and left"],
      ["LeftAndRight", "Left and right"],
      ["UpAndDown", "Up and down"],
      ["DownAndUp", "Down and up"],
    ],
  },
  {
    group: "One direction, out and back",
    items: [
      ["North", "North (up)"],
      ["NorthEast", "North-east"],
      ["East", "East (right)"],
      ["SouthEast", "South-east"],
      ["South", "South (down)"],
      ["SouthWest", "South-west"],
      ["West", "West (left)"],
      ["NorthWest", "North-west"],
    ],
  },
  {
    group: "Other",
    items: [
      ["Random", "Random direction"],
      ["Virtual", "Invisible (resets idle time, the cursor stays put)"],
    ],
  },
];

export const motionLabel = (m: Motion) =>
  DIRECTIONS.flatMap((g) => g.items).find(([id]) => id === m)?.[1] ?? m;
export const keyLabel = (k: number) => KEYS.find(([c]) => c === k)?.[1] ?? "a key press";

export const DAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
export const WEEKDAYS = [true, true, true, true, true, false, false];
export const EVERY_DAY = [true, true, true, true, true, true, true];

export const hhmm = (m: number) =>
  `${String(Math.floor(m / 60) % 24).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
export const toMinutes = (v: string) => {
  const [h, m] = v.split(":").map(Number);
  return ((h || 0) * 60 + (m || 0)) % 1440;
};
export const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
```

- [ ] **Step 2: `src/icons.tsx`.**

```tsx
// Small line icons for the rail (spec 006 FR-024). Inline SVG: no assets, no dependency, and
// they follow the text colour in light, dark and High Contrast.
export type IconName = "home" | "movement" | "behaviour" | "schedules" | "blackouts" | "appearance" | "about";

const PATHS: Record<IconName, string> = {
  home: "M2.5 7.5 8 3l5.5 4.5M4 6.5V13h3V9.5h2V13h3V6.5",
  movement: "M8 1.5v13M1.5 8h13M6 3.5l2-2 2 2M6 12.5l2 2 2-2M3.5 6l-2 2 2 2M12.5 6l2 2-2 2",
  behaviour: "M2 4.5h6M11 4.5h3M9.5 3v3M2 11.5h2M7 11.5h7M5.5 10v3",
  schedules: "M2.5 3.5h11v10h-11zM2.5 6.5h11M5 2v3M11 2v3",
  blackouts: "M12.5 10.5A5 5 0 0 1 5.5 3.5a5 5 0 1 0 7 7z",
  appearance: "M2 3h12v10H2zM2 5.5h12",
  about: "M8 14.5A6.5 6.5 0 1 0 8 1.5a6.5 6.5 0 0 0 0 13zM8 7v4.5M8 4.75v.5",
};

export function Icon({ name }: { name: IconName }) {
  return (
    <svg
      className="icon"
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
```

- [ ] **Step 3: `src/controls.tsx`.** Keep `Switch` and `NumberField` as they are. Replace
`SettingRow`, and add `DaysPicker` and `TimeField`. Change the React import to
`import { useId, useState, type ReactNode } from "react";` (`useEffect` stays only if still used),
and add `import { DAY_NAMES, hhmm, toMinutes } from "./types";`.

```tsx
/** One setting: a short label, at most one hint line, and the long explanation behind a "?"
 *  that opens and closes with the keyboard (spec 006 FR-025). */
export function SettingRow({
  title,
  hint,
  help,
  children,
}: {
  title: string;
  hint?: string;
  help?: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <div className="setting-wrap">
      <div className="setting">
        <div>
          <div className="setting-title">
            {title}
            {help && (
              <button
                type="button"
                className="help"
                aria-expanded={open}
                aria-controls={id}
                aria-label={`More about ${title}`}
                onClick={() => setOpen(!open)}
              >
                ?
              </button>
            )}
          </div>
          {hint && <div className="note">{hint}</div>}
        </div>
        <div className="setting-control">{children}</div>
      </div>
      {help && open && (
        <div id={id} className="help-text">
          {help}
        </div>
      )}
    </div>
  );
}

/** Seven toggle chips, Monday first. */
export function DaysPicker({ days, onChange }: { days: boolean[]; onChange: (d: boolean[]) => void }) {
  return (
    <div className="days" role="group" aria-label="Days">
      {DAY_NAMES.map((n, i) => (
        <button
          key={n}
          type="button"
          className={`chip ${days[i] ? "on" : ""}`}
          aria-pressed={days[i]}
          onClick={() => onChange(days.map((d, j) => (j === i ? !d : d)))}
        >
          {n}
        </button>
      ))}
    </div>
  );
}

/** A time of day, as minutes since midnight. An empty or partial value is ignored. */
export function TimeField({
  minutes,
  onChange,
  label,
}: {
  minutes: number;
  onChange: (m: number) => void;
  label: string;
}) {
  return (
    <input
      type="time"
      className="btn time"
      aria-label={label}
      value={hhmm(minutes)}
      onChange={(e) => e.target.value && onChange(toMinutes(e.target.value))}
    />
  );
}
```

- [ ] **Step 4: `src/movement.tsx`.**

```tsx
// Movement (spec 006 FR-001 to FR-006, FR-024): what happens, and when.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DIRECTIONS, KEYS, type InputSettings, type Motion, type Speed } from "./types";
import { NumberField, SettingRow, Switch } from "./controls";

const SPEEDS: [Speed, string][] = [["Slow", "Slow"], ["Normal", "Normal"], ["Fast", "Fast"], ["Custom", "Custom"]];

export default function Movement() {
  const [s, setS] = useState<InputSettings | null>(null);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    invoke<InputSettings>("get_input_settings").then(setS).catch(() => {});
  }, []);

  const save = (next: InputSettings) =>
    invoke<InputSettings>("set_input_settings", { settings: next }).then(setS).catch(() => {});

  const test = () => {
    invoke("test_move").catch(() => {});
    if (s && s.key === 0 && s.motion === "Virtual") {
      setNote("Sent an invisible move: the cursor stays put, but Windows saw input.");
      window.setTimeout(() => setNote(null), 2500);
    }
  };

  if (!s) return <h1>Movement</h1>;
  const mouse = s.key === 0;
  const visible = mouse && s.motion !== "Virtual";

  return (
    <>
      <h1>Movement</h1>

      <section className="section first">
        <h2>What happens</h2>
        <SettingRow
          title="Send"
          hint="Move the mouse, or press a key."
          help="A key press works where mouse movement is ignored, such as some remote desktops. F15 is a key no keyboard has, but a few apps (PuTTY, PowerPoint, Google Docs) still react to it."
        >
          <select className="btn" aria-label="Send" value={s.key} onChange={(e) => save({ ...s, key: Number(e.target.value) })}>
            {KEYS.map(([code, label]) => (
              <option key={code} value={code}>
                {label}
              </option>
            ))}
          </select>
        </SettingRow>

        {mouse && (
          <SettingRow
            title="Direction"
            hint="Every movement comes back to where it started."
            help="Invisible nudges the mouse one pixel and back in the same instant: Windows sees input and nothing moves on screen. Use it while you share your screen."
          >
            <select className="btn" aria-label="Direction" value={s.motion} onChange={(e) => save({ ...s, motion: e.target.value as Motion })}>
              {DIRECTIONS.map((g) => (
                <optgroup key={g.group} label={g.group}>
                  {g.items.map(([id, label]) => (
                    <option key={id} value={id}>
                      {label}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
          </SettingRow>
        )}

        {visible && (
          <>
            <SettingRow
              title="Distance"
              hint={s.distance_random ? "A new distance each time, between these two." : "How far each side goes."}
              help="Measured in screen pixels. The cursor always ends exactly where it started."
            >
              <NumberField label="Distance in pixels" value={s.distance_px} max={500} onCommit={(n) => save({ ...s, distance_px: n })} />
              {s.distance_random && (
                <>
                  to
                  <NumberField label="Largest distance in pixels" value={s.distance_max_px} max={500} onCommit={(n) => save({ ...s, distance_max_px: n })} />
                </>
              )}
              px
            </SettingRow>
            <SettingRow title="Random distance">
              <Switch label="Random distance" on={s.distance_random} onChange={(v) => save({ ...s, distance_random: v })} />
            </SettingRow>
            <SettingRow title="Speed" hint="How fast the cursor travels the path.">
              <select className="btn" aria-label="Speed" value={s.speed} onChange={(e) => save({ ...s, speed: e.target.value as Speed })}>
                {SPEEDS.map(([id, label]) => (
                  <option key={id} value={id}>
                    {label}
                  </option>
                ))}
              </select>
              {s.speed === "Custom" && (
                <>
                  <NumberField label="Milliseconds between steps" value={s.custom_step_ms} max={50} onCommit={(n) => save({ ...s, custom_step_ms: n })} />
                  ms
                </>
              )}
            </SettingRow>
            <SettingRow title="Stop if I move the mouse" hint="Touch the mouse while it moves and it lets go at once.">
              <Switch label="Stop if I move the mouse" on={s.abortable} onChange={(v) => save({ ...s, abortable: v })} />
            </SettingRow>
          </>
        )}
      </section>

      <section className="section">
        <h2>When</h2>
        <SettingRow
          title="Move after"
          hint={s.interval_random ? "A new wait each time, between these two." : "Seconds with no mouse or keyboard input."}
          help="The countdown restarts whenever you use the mouse or keyboard, so it never moves while you work. Teams usually marks you away after about five minutes without input."
        >
          <NumberField label="Seconds with no input" value={s.interval_secs} max={3_600} onCommit={(n) => save({ ...s, interval_secs: n })} />
          {s.interval_random && (
            <>
              to
              <NumberField label="Longest wait in seconds" value={s.interval_max_secs} max={3_600} onCommit={(n) => save({ ...s, interval_max_secs: n })} />
            </>
          )}
          s
        </SettingRow>
        <SettingRow title="Random wait" hint="So the moves don't line up with other things on a timer.">
          <Switch label="Random wait" on={s.interval_random} onChange={(v) => save({ ...s, interval_random: v })} />
        </SettingRow>
      </section>

      <section className="section">
        <SettingRow title="Try it" hint="Runs one movement now, even while stopped.">
          <button className="btn" onClick={test}>
            Test
          </button>
        </SettingRow>
        {note && (
          <p className="note" role="status">
            {note}
          </p>
        )}
      </section>
    </>
  );
}
```

- [ ] **Step 5: `src/home.tsx`.** Replace the whole file:

```tsx
// Home (spec 006 FR-024): what is true right now, one button, Run for, and a summary of the
// movement with a way to change it.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  clock, hhmm, keyLabel, motionLabel,
  type InputSettings, type PauseReason, type RunFor, type RunSettings, type Status,
} from "./types";
import { TimeField } from "./controls";
import { UpdateBanner } from "./updates";

const RUN_FOR: [string, string][] = [
  ["forever", "Until I stop"],
  ["30", "30 minutes"],
  ["60", "1 hour"],
  ["120", "2 hours"],
  ["240", "4 hours"],
  ["until", "Until a time"],
];

function toRunFor(choice: string, until: number): RunFor {
  if (choice === "forever") return { kind: "forever" };
  if (choice === "until") return { kind: "until", at: until };
  return { kind: "minutes", minutes: Number(choice) };
}

function pauseText(p: PauseReason): string {
  switch (p.reason) {
    case "battery":
      return "On battery. It carries on when you plug in.";
    case "locked":
      return "The screen is locked. It carries on when you unlock.";
    case "presenting":
      return "You are presenting, or a full-screen app is open.";
    case "blackout":
      return `Blackout until ${hhmm(p.until)}.`;
  }
}

type Tone = "on" | "off" | "warn" | "pause";

function describe(s: Status): { title: string; detail: string; tone: Tone } {
  switch (s.kind) {
    case "running":
      return { title: "Running", detail: s.next_move_in_secs == null ? "Starting." : `Next move in ${clock(s.next_move_in_secs)}`, tone: "on" };
    case "running_blocked":
      return {
        title: "Running, but Windows blocked the last move",
        detail: "An app running as administrator is in front. Click another window and the next move will land.",
        tone: "warn",
      };
    case "running_power_only":
      return { title: "Running", detail: "Keeping the PC awake. Mouse moves are off (Behaviour).", tone: "on" };
    case "paused":
      return { title: "Paused", detail: s.pause ? pauseText(s.pause) : "", tone: "pause" };
    case "stopped_but_rule_holds":
      return {
        title: "Stopped",
        detail: s.holding_app
          ? `But ${s.holding_app} is running, so the PC stays awake (Behaviour).`
          : "But a rule is keeping the PC awake (About, Troubleshooting).",
        tone: "off",
      };
    default:
      return { title: "Stopped", detail: "Your PC can sleep and lock as normal.", tone: "off" };
  }
}

/** The one honest explanation, matched to what Start will actually do (constitution I/II). */
function explanation(input: InputSettings, run: RunSettings): string {
  if (!run.move_mouse) {
    return "When running, this keeps the PC awake. It sends no input, so the screen can still lock and Teams or Slack can still show you as away.";
  }
  const what =
    input.key !== 0
      ? `presses ${keyLabel(input.key).replace(" key press", "")} once`
      : input.motion === "Virtual"
        ? "nudges the mouse invisibly"
        : "moves your mouse a few pixels and back";
  return `When running, this ${what} once your PC has had no input for a while. Windows, the screen lock and apps that watch for idle time, such as Teams and Slack, see that as activity. It also keeps the PC from sleeping. Monitoring software can detect simulated input.`;
}

function summary(input: InputSettings, run: RunSettings): string {
  if (!run.move_mouse) return "Mouse moves are off";
  const what = input.key !== 0 ? keyLabel(input.key) : motionLabel(input.motion);
  const wait = input.interval_random ? `${input.interval_secs} to ${input.interval_max_secs} s` : `${input.interval_secs} s`;
  return `${what} after ${wait} with no input`;
}

export default function Home({ go }: { go: (page: "movement" | "behaviour") => void }) {
  const [s, setS] = useState<Status | null>(null);
  const [input, setInput] = useState<InputSettings | null>(null);
  const [run, setRun] = useState<RunSettings | null>(null);
  const [choice, setChoice] = useState("forever");
  const [until, setUntil] = useState(18 * 60);
  const [testing, setTesting] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const read = useCallback(() => {
    invoke<Status>("get_status").then(setS).catch(() => {});
  }, []);
  const readSettings = useCallback(() => {
    invoke<InputSettings>("get_input_settings").then(setInput).catch(() => {});
    invoke<RunSettings>("get_run_settings").then(setRun).catch(() => {});
  }, []);

  useEffect(() => {
    read();
    readSettings();
    // Once a second, as text (UI-UX §4). The event makes changes from the tray show at once.
    const t = window.setInterval(read, 1000);
    const un = listen("state:changed", () => {
      read();
      readSettings();
    });
    return () => {
      window.clearInterval(t);
      un.then((f) => f());
    };
  }, [read, readSettings]);

  const running = s?.running ?? false;
  const d = s ? describe(s) : null;

  const startStop = () =>
    invoke(running ? "stop" : "start", running ? {} : { runFor: toRunFor(choice, until) })
      .then(read)
      .catch(() => {});

  const changeRunFor = (c: string, u: number) => {
    setChoice(c);
    setUntil(u);
    if (running) invoke("set_run_for", { runFor: toRunFor(c, u) }).then(read).catch(() => {});
  };

  const test = () => {
    setTesting(true);
    invoke("test_move").catch(() => {});
    if (input && input.key === 0 && input.motion === "Virtual") {
      setNote("Sent an invisible move: the cursor stays put, but Windows saw input.");
      window.setTimeout(() => setNote(null), 2500);
    }
    window.setTimeout(() => setTesting(false), 800); // one path at a time, not a queue of them
  };

  const stopsAt = s?.stops_at ? new Date(s.stops_at * 1000) : null;

  return (
    <>
      <UpdateBanner />
      <div className={`status ${d?.tone ?? "off"}`}>
        <div className="status-title" role="status" aria-live="polite">
          <span className="dot" aria-hidden="true" />
          {d?.title ?? " "}
        </div>
        <div className="status-detail" role="timer">
          {d?.detail ?? " "}
        </div>
        {running && s && (
          <div className="status-detail">
            {s.keep_screen_on ? "PC won't sleep · screen stays on" : "PC won't sleep · the screen may turn off"}
            {stopsAt && ` · Stops at ${hhmm(stopsAt.getHours() * 60 + stopsAt.getMinutes())}`}
          </div>
        )}
      </div>

      <button className={`btn big ${running ? "" : "primary"}`} onClick={startStop}>
        {running ? (
          <><span aria-hidden="true">■</span>  Stop</>
        ) : (
          <><span aria-hidden="true">▶</span>  Start</>
        )}
      </button>

      <div className="fields">
        <div className="field">
          <span>Run for</span>
          <span className="inline">
            <select className="btn" aria-label="Run for" value={choice} onChange={(e) => changeRunFor(e.target.value, until)}>
              {RUN_FOR.map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))}
            </select>
            {choice === "until" && <TimeField label="Stop at" minutes={until} onChange={(m) => changeRunFor("until", m)} />}
          </span>
        </div>
        {input && run && (
          <div className="field">
            <span>Movement</span>
            <span className="inline">
              <span className="summary">{summary(input, run)}</span>
              <button className="btn" onClick={() => go(run.move_mouse ? "movement" : "behaviour")}>
                Edit
              </button>
              <button className="btn" onClick={test} disabled={testing || !run.move_mouse}>
                Test
              </button>
            </span>
          </div>
        )}
        {note && (
          <p className="note" role="status">
            {note}
          </p>
        )}
      </div>

      {input && run && <p className="note">{explanation(input, run)}</p>}
      <p className="note">
        {s && <>Idle for {clock(s.idle_secs)} · </>}Closing this window keeps project-mouse running in the tray, next to the clock.
      </p>
    </>
  );
}
```

`go("behaviour")` targets a page that only exists from Task 11. Until then, `App` maps any
unknown page to Home (see Step 6), so tsc passes.

- [ ] **Step 6: `src/App.tsx`.** Replace it:

```tsx
// The window (spec 006 FR-024): tabs down the left, one concern each, as in Move Mouse.
import { useState } from "react";
import Home from "./home";
import Movement from "./movement";
import Settings from "./settings";
import Advanced from "./advanced";
import { Icon, type IconName } from "./icons";
import "./styles.css";

export type Page = "home" | "movement" | "behaviour" | "settings" | "advanced";

const PAGES: [Page, string, IconName][] = [
  ["home", "Home", "home"],
  ["movement", "Movement", "movement"],
  ["settings", "Settings", "behaviour"],
  ["advanced", "Advanced", "about"],
];

export default function App() {
  const [page, setPage] = useState<Page>("home");
  return (
    <div className="app">
      <nav className="rail" aria-label="Pages">
        <div className="brand">project-mouse</div>
        {PAGES.map(([id, label, icon]) => (
          <button
            key={id}
            className={page === id ? "active" : ""}
            aria-current={page === id ? "page" : undefined}
            onClick={() => setPage(id)}
          >
            <Icon name={icon} />
            {label}
          </button>
        ))}
      </nav>
      <main className="content">
        {page === "movement" ? (
          <Movement />
        ) : page === "settings" ? (
          <Settings />
        ) : page === "advanced" ? (
          <Advanced />
        ) : (
          <Home go={setPage} />
        )}
      </main>
    </div>
  );
}
```

- [ ] **Step 7: Fix up `settings.tsx` and `updates.tsx`.**
- `settings.tsx`:
  - Delete the "What to send", "Distance" and "Vary by" rows; Movement owns them now.
  - Delete `visible`, and any import that becomes unused (`KEYS`, `NumberField`, `type InputSettings`, and the `input` state if nothing else uses it).
  - Rename `note=` to `hint=` on every `SettingRow`.
- `updates.tsx`: rename `note=` to `hint=` on `SettingRow`.

- [ ] **Step 8: Styles.** Append to `src/styles.css`:

```css
/* M8: tabs with icons, the "?" help, chips, list entries, paused tone (spec 006 FR-024/FR-025). */
:root { --pause: #9a6700; }
@media (prefers-color-scheme: dark) { :root { --pause: #d29922; } }

.rail button { display: flex; align-items: center; gap: 8px; }
.rail .icon { flex-shrink: 0; opacity: .85; }
.rail button.active .icon { opacity: 1; }

.section.first { margin-top: 0; border-top: none; padding-top: 0; }
.setting-wrap { border-bottom: 1px solid var(--border); }
.setting-wrap:last-child { border-bottom: none; }
.setting-title { display: flex; align-items: center; gap: 6px; }
.help {
  all: unset; box-sizing: border-box; width: 18px; height: 18px; border-radius: 50%;
  border: 1px solid var(--border); color: var(--text-2); font-size: 11px; line-height: 16px;
  text-align: center; cursor: default;
}
.help:hover, .help[aria-expanded="true"] { border-color: var(--accent); color: var(--accent); }
.help:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.help-text {
  margin: 0 0 10px; padding: 8px 10px; border-radius: var(--radius);
  background: var(--surface); color: var(--text-2); font-size: 12px; line-height: 1.5;
}

.status.pause .dot { background: var(--pause); border-color: var(--pause); }
.summary { color: var(--text-2); }
.btn.time { width: 92px; }

.days { display: flex; gap: 4px; flex-wrap: wrap; }
.chip {
  all: unset; box-sizing: border-box; padding: 3px 8px; border-radius: 10px; font-size: 12px;
  border: 1px solid var(--border); color: var(--text-2); cursor: default;
}
.chip.on { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
.chip:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }

.entry { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; padding: 10px 0; border-bottom: 1px solid var(--border); font-size: 13px; }
.entry .grow { flex: 1; }
.empty { color: var(--text-2); font-size: 13px; padding: 8px 0; }
.icon-btn { padding: 4px 9px; }
```

Delete the now-unused old `.days` and `.days label` rules from the rule-builder block.

- [ ] **Step 9: Gates.** Run `npx tsc --noEmit`, `npm run build`, the honesty grep, and
`grep -rnP "\x{2014}" src` (must be empty). All must be clean.

- [ ] **Step 10: Commit.**

```bash
git add -A src
git commit -m "feat(M8): tab rail with icons, ? help, Movement tab, new Home (FR-001..FR-007, FR-021, FR-024, FR-025)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Behaviour tab

**Files:**
- Create: `src/behaviour.tsx`
- Modify: `src/App.tsx` (add the Behaviour tab and remove Settings)
- Delete: `src/settings.tsx` (everything it had moves to Behaviour, Movement or About; Updates
  moves to About in Task 13, so until then Behaviour renders `<UpdateSettings />` at its end)

**Interfaces:**
- Consumes these IPC commands:
  - `get_run_settings` / `set_run_settings({ settings })`
  - `get_autostart` / `set_autostart({ enabled })`
  - `get_apps` / `set_apps({ names })`
  - `list_running_apps`
  - the `state:changed` event

- [ ] **Step 1: Write `src/behaviour.tsx`.**

```tsx
// Behaviour (spec 006 FR-008, FR-009, FR-024, FR-026, FR-027): what Start does, when it pauses,
// what keeps the PC awake on its own, and how it starts.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RunSettings } from "./types";
import { SettingRow, Switch } from "./controls";
import { UpdateSettings } from "./updates";

export default function Behaviour() {
  const [run, setRun] = useState<RunSettings | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [autostartErr, setAutostartErr] = useState<string | null>(null);
  const [apps, setApps] = useState<string[]>([]);
  const [running, setRunning] = useState<string[]>([]);
  const [draft, setDraft] = useState("");

  const readAutostart = useCallback(() => {
    invoke<boolean>("get_autostart").then(setAutostart).catch(() => {});
  }, []);

  useEffect(() => {
    invoke<RunSettings>("get_run_settings").then(setRun).catch(() => {});
    invoke<string[]>("get_apps").then(setApps).catch(() => {});
    invoke<string[]>("list_running_apps").then(setRunning).catch(() => {});
    readAutostart();
    // The tray can flip Start with Windows too; stay in step with it.
    const un = listen("state:changed", readAutostart);
    return () => {
      un.then((f) => f());
    };
  }, [readAutostart]);

  const saveRun = (next: RunSettings) => {
    setRun(next);
    invoke("set_run_settings", { settings: next }).catch(() => {});
  };
  const saveApps = (names: string[]) =>
    invoke<string[]>("set_apps", { names }).then(setApps).catch(() => {});
  const addApp = () => {
    if (draft.trim()) saveApps([...apps, draft.trim()]).then(() => setDraft(""));
  };
  const toggleAutostart = (on: boolean) => {
    setAutostartErr(null);
    invoke<boolean>("set_autostart", { enabled: on })
      .then(setAutostart)
      .catch((e) => setAutostartErr(String(e)));
  };

  if (!run) return <h1>Behaviour</h1>;

  return (
    <>
      <h1>Behaviour</h1>

      <section className="section first">
        <h2>When running</h2>
        <SettingRow
          title="Move the mouse"
          hint="Off: Start only keeps the PC awake."
          help="With this off, nothing is sent: the screen can still lock, and Teams or Slack can still show you as away."
        >
          <Switch label="Move the mouse" on={run.move_mouse} onChange={(v) => saveRun({ ...run, move_mouse: v })} />
        </SettingRow>
        <SettingRow title="Keep the screen on" hint="Off: the PC stays awake, but the screen may turn off.">
          <Switch label="Keep the screen on" on={run.keep_screen_on} onChange={(v) => saveRun({ ...run, keep_screen_on: v })} />
        </SettingRow>
      </section>

      <section className="section">
        <h2>Pause automatically</h2>
        <SettingRow title="On battery" hint="Lets the PC sleep to save the battery. Carries on when you plug in.">
          <Switch label="Pause on battery" on={run.pause_on_battery} onChange={(v) => saveRun({ ...run, pause_on_battery: v })} />
        </SettingRow>
        <SettingRow title="When the screen is locked" hint="Moves stop and the PC stays awake. Carries on when you unlock.">
          <Switch label="Pause when the screen is locked" on={run.pause_when_locked} onChange={(v) => saveRun({ ...run, pause_when_locked: v })} />
        </SettingRow>
        <SettingRow
          title="While presenting"
          hint="Presentations, full-screen videos and games. Moves stop and the PC stays awake."
          help="Windows reports when presentation mode is on or an app is full screen. Nothing on screen twitches while people are watching."
        >
          <Switch label="Pause while presenting" on={run.pause_when_presenting} onChange={(v) => saveRun({ ...run, pause_when_presenting: v })} />
        </SettingRow>
      </section>

      <section className="section">
        <h2>Keep awake while these apps run</h2>
        <p className="note">
          Whenever one of these is running, the PC stays awake, even while Home says Stopped. No mouse moves. Good for
          builds, renders and downloads.
        </p>
        {apps.length === 0 && <p className="empty">No apps yet.</p>}
        {apps.map((a) => (
          <div className="entry" key={a}>
            <span className="grow">{a}</span>
            <button className="btn icon-btn" aria-label={`Remove ${a}`} onClick={() => saveApps(apps.filter((x) => x !== a))}>
              ✕
            </button>
          </div>
        ))}
        <div className="cond-row" style={{ marginTop: 10 }}>
          <input
            className="btn"
            style={{ flex: 1 }}
            list="running-apps"
            placeholder="App name, e.g. msbuild.exe"
            aria-label="App to add"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addApp()}
          />
          <datalist id="running-apps">
            {running.map((n) => (
              <option key={n} value={n} />
            ))}
          </datalist>
          <button className="btn primary" onClick={addApp}>
            Add
          </button>
        </div>
      </section>

      <section className="section">
        <h2>Starting</h2>
        <SettingRow title="Start automatically when project-mouse opens">
          <Switch label="Start automatically when project-mouse opens" on={run.start_on_launch} onChange={(v) => saveRun({ ...run, start_on_launch: v })} />
        </SettingRow>
        <SettingRow title="Start project-mouse with Windows" hint="It opens in the tray, without this window.">
          <Switch label="Start project-mouse with Windows" on={autostart} onChange={toggleAutostart} />
        </SettingRow>
        {autostartErr && <p className="note error">{autostartErr}</p>}
        <p className="note">Ctrl+Alt+K starts and stops it from anywhere.</p>
      </section>

      <UpdateSettings />
    </>
  );
}
```

- [ ] **Step 2: `App.tsx`.**
- Replace the `settings` entry with `["behaviour", "Behaviour", "behaviour"]`.
- Remove `"settings"` from `Page`.
- Import `Behaviour` and render it for `page === "behaviour"`.
- Delete the `Settings` import and `src/settings.tsx`.

- [ ] **Step 3: Gates.** Run tsc, the build, the honesty grep, and the em dash grep on `src`.

- [ ] **Step 4: Commit.**

```bash
git add -A src
git commit -m "feat(M8): Behaviour tab: pauses, keep awake while apps run, starting (FR-008, FR-009, FR-026, FR-027)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Schedules and Blackouts tabs

**Files:**
- Create: `src/timetable.tsx` (exports `Schedules` and `Blackouts`)
- Modify: `src/App.tsx` (two tabs)

**Interfaces:**
- Consumes these IPC commands: `get_timetable` and `set_timetable({ timetable }) -> Timetable`.

- [ ] **Step 1: Write `src/timetable.tsx`.**

```tsx
// Schedules and Blackouts (spec 006 FR-010, FR-011): Move Mouse's two time tabs. Every change is
// saved at once. A blackout whose end equals its start is refused here (Review Focus 3).
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EVERY_DAY, WEEKDAYS, type Blackout, type Schedule, type Timetable } from "./types";
import { DaysPicker, Switch, TimeField } from "./controls";

function useTimetable(): [Timetable | null, (t: Timetable) => void] {
  const [t, setT] = useState<Timetable | null>(null);
  useEffect(() => {
    invoke<Timetable>("get_timetable").then(setT).catch(() => {});
  }, []);
  const save = (next: Timetable) => {
    setT(next);
    invoke<Timetable>("set_timetable", { timetable: next }).then(setT).catch(() => {});
  };
  return [t, save];
}

export function Schedules() {
  const [t, save] = useTimetable();
  if (!t) return <h1>Schedules</h1>;
  const set = (i: number, s: Schedule) => save({ ...t, schedules: t.schedules.map((x, j) => (j === i ? s : x)) });
  const add = (...s: Schedule[]) => save({ ...t, schedules: [...t.schedules, ...s] });

  return (
    <>
      <h1>Schedules</h1>
      <p className="note">
        Start or stop at set times. If you start or stop it yourself in between, that stands until the next scheduled
        time. project-mouse has to be running in the tray, so turn on Start with Windows (Behaviour).
      </p>
      {t.schedules.length === 0 && <p className="empty">No schedules yet.</p>}
      {t.schedules.map((s, i) => (
        <div className="entry" key={i}>
          <select className="btn" aria-label="Action" value={s.action} onChange={(e) => set(i, { ...s, action: e.target.value as Schedule["action"] })}>
            <option value="Start">Start</option>
            <option value="Stop">Stop</option>
          </select>
          <span>at</span>
          <TimeField label="Time" minutes={s.at} onChange={(m) => set(i, { ...s, at: m })} />
          <DaysPicker days={s.days} onChange={(d) => set(i, { ...s, days: d })} />
          <span className="grow" />
          <Switch label="Enabled" on={s.enabled} onChange={(v) => set(i, { ...s, enabled: v })} />
          <button className="btn icon-btn" aria-label="Delete this schedule" onClick={() => save({ ...t, schedules: t.schedules.filter((_, j) => j !== i) })}>
            ✕
          </button>
        </div>
      ))}
      <div className="cond-row" style={{ marginTop: 12 }}>
        <button
          className="btn primary"
          onClick={() =>
            add(
              { days: WEEKDAYS, at: 9 * 60, action: "Start", enabled: true },
              { days: WEEKDAYS, at: 18 * 60, action: "Stop", enabled: true },
            )
          }
        >
          + Work hours (Mon to Fri, 09:00 to 18:00)
        </button>
        <button className="btn" onClick={() => add({ days: WEEKDAYS, at: 9 * 60, action: "Start", enabled: true })}>
          + Start time
        </button>
        <button className="btn" onClick={() => add({ days: WEEKDAYS, at: 18 * 60, action: "Stop", enabled: true })}>
          + Stop time
        </button>
      </div>
    </>
  );
}

export function Blackouts() {
  const [t, save] = useTimetable();
  const [err, setErr] = useState<string | null>(null);
  if (!t) return <h1>Blackouts</h1>;
  const set = (i: number, b: Blackout) => {
    if (b.from === b.to) {
      setErr("The end has to be different from the start.");
      return;
    }
    setErr(null);
    save({ ...t, blackouts: t.blackouts.map((x, j) => (j === i ? b : x)) });
  };
  const add = (b: Blackout) => save({ ...t, blackouts: [...t.blackouts, b] });

  return (
    <>
      <h1>Blackouts</h1>
      <p className="note">
        Quiet times with no mouse moves, such as lunch. The PC stays awake, so it carries on afterwards. A window can
        cross midnight; its days are the days it starts on.
      </p>
      {t.blackouts.length === 0 && <p className="empty">No blackouts yet.</p>}
      {t.blackouts.map((b, i) => (
        <div className="entry" key={i}>
          <TimeField label="From" minutes={b.from} onChange={(m) => set(i, { ...b, from: m })} />
          <span>to</span>
          <TimeField label="To" minutes={b.to} onChange={(m) => set(i, { ...b, to: m })} />
          <DaysPicker days={b.days} onChange={(d) => set(i, { ...b, days: d })} />
          <span className="grow" />
          <Switch label="Enabled" on={b.enabled} onChange={(v) => set(i, { ...b, enabled: v })} />
          <button className="btn icon-btn" aria-label="Delete this blackout" onClick={() => save({ ...t, blackouts: t.blackouts.filter((_, j) => j !== i) })}>
            ✕
          </button>
        </div>
      ))}
      {err && <p className="note error" role="alert">{err}</p>}
      <div className="cond-row" style={{ marginTop: 12 }}>
        <button className="btn primary" onClick={() => add({ days: WEEKDAYS, from: 12 * 60 + 30, to: 13 * 60 + 30, enabled: true })}>
          + Lunch (Mon to Fri, 12:30 to 13:30)
        </button>
        <button className="btn" onClick={() => add({ days: EVERY_DAY, from: 12 * 60, to: 13 * 60, enabled: true })}>
          + Blackout
        </button>
      </div>
    </>
  );
}
```

- [ ] **Step 2: `App.tsx`.** Add `"schedules" | "blackouts"` to `Page`, and insert the entries
`["schedules", "Schedules", "schedules"]` and `["blackouts", "Blackouts", "blackouts"]` after
Behaviour. Import `{ Schedules, Blackouts }` from `./timetable` and render them.

- [ ] **Step 3: Gates.** Run tsc, the build, the honesty grep, and the em dash grep.

- [ ] **Step 4: Commit.**

```bash
git add -A src
git commit -m "feat(M8): Schedules and Blackouts tabs (FR-010, FR-011)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Appearance and About tabs; retire Advanced

**Files:**
- Create: `src/appearance.tsx`, `src/about.tsx`
- Modify: `src/App.tsx` (the final seven tabs)
- Modify: `src/behaviour.tsx` (remove `<UpdateSettings />`; About owns it now)
- Delete: `src/advanced.tsx`, `src/rules.tsx`
- Modify: `src/styles.css` (the Troubleshooting `details`; remove dead rule-builder and timer CSS:
  `.builder`, `.cond`, `.rail .spacer`, `.mono-hint`)

**Interfaces:**
- Consumes these IPC commands:
  - `get_appearance` / `set_appearance({ appearance })`
  - `get_update_status`
  - `get_status` (idle time)
  - `get_diagnostics`, `why_awake`, `get_logs`
  - `get_rules`, `set_rule_enabled({ id, enabled })`, `delete_rule({ id })`
  - `import_move_mouse({ path })`

- [ ] **Step 1: Write `src/appearance.tsx`.**

```tsx
// Appearance (spec 006 FR-015 to FR-017).
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Appearance as A } from "./types";
import { SettingRow, Switch } from "./controls";

export default function Appearance() {
  const [a, setA] = useState<A | null>(null);
  useEffect(() => {
    invoke<A>("get_appearance").then(setA).catch(() => {});
  }, []);
  const save = (next: A) => {
    setA(next);
    invoke("set_appearance", { appearance: next }).catch(() => {});
  };
  if (!a) return <h1>Appearance</h1>;
  return (
    <>
      <h1>Appearance</h1>
      <section className="section first">
        <SettingRow title="Always on top" hint="Keep this window above other windows.">
          <Switch label="Always on top" on={a.always_on_top} onChange={(v) => save({ ...a, always_on_top: v })} />
        </SettingRow>
        <SettingRow title="Taskbar dot" hint="Green while running, yellow while paused, on the taskbar button while this window is open.">
          <Switch label="Taskbar dot" on={a.taskbar_dot} onChange={(v) => save({ ...a, taskbar_dot: v })} />
        </SettingRow>
        <SettingRow
          title="Notifications"
          hint="Only for things you didn't do yourself."
          help="A schedule starting or stopping it, Run for ending, or Windows blocking a move. Your own clicks never notify."
        >
          <Switch label="Notifications" on={a.notifications} onChange={(v) => save({ ...a, notifications: v })} />
        </SettingRow>
      </section>
    </>
  );
}
```

- [ ] **Step 2: Write `src/about.tsx`.** Move these from `src/advanced.tsx` verbatim, keeping
their doc comments: the types `Diagnostics` and `AwakeReport`, `fmtIdle`, `ELEVATED_CMD`,
`WhyAwake`, `Readout`, `Activity` and `ImportMoveMouse`. Do not move `ProfileSummary`, `Timer`,
`TIMER_ID`, `DURATIONS`, `ProfileSwitcher`, `ProfileManager` or `Advanced`; they are gone. Then
add:

```tsx
// About (spec 006 FR-024, FR-029, FR-030): what this is, updates, import, and Troubleshooting in
// plain words.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { clock, type Status } from "./types";
import { UpdateSettings } from "./updates";

type LegacyRule = { id: string; name: string; enabled: boolean };
type ProfileView = { id: string; name: string; rules: LegacyRule[] };

/** Rules from an earlier version (FR-029): anything that is not the apps list, shown so nothing
 *  keeps the PC awake invisibly. Hidden when there are none. */
function LegacyRules() {
  const [rules, setRules] = useState<LegacyRule[]>([]);
  const load = useCallback(() => {
    invoke<ProfileView>("get_rules")
      .then((p) => setRules(p.rules.filter((r) => r.id !== "apps")))
      .catch(() => {});
  }, []);
  useEffect(load, [load]);
  if (!rules.length) return null;
  return (
    <>
      <h2>Rules from an earlier version</h2>
      <p className="note">These keep the PC awake on their own conditions. Turn them off or delete them if you don't need them.</p>
      {rules.map((r) => (
        <div className="entry" key={r.id}>
          <span className="grow">{r.name}</span>
          <label className="note">
            <input type="checkbox" checked={r.enabled} onChange={(e) => invoke("set_rule_enabled", { id: r.id, enabled: e.target.checked }).then(load)} /> on
          </label>
          <button className="btn icon-btn" aria-label={`Delete ${r.name}`} onClick={() => invoke("delete_rule", { id: r.id }).then(load)}>
            ✕
          </button>
        </div>
      ))}
    </>
  );
}

export default function About() {
  const [s, setS] = useState<Status | null>(null);
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  useEffect(() => {
    const read = () => {
      invoke<Status>("get_status").then(setS).catch(() => {});
      invoke<Diagnostics>("get_diagnostics").then(setDiag).catch(() => {});
    };
    read();
    const t = window.setInterval(read, 1000);
    return () => window.clearInterval(t);
  }, []);

  return (
    <>
      <h1>About</h1>
      <section className="section first">
        <p className="note">Source and issues: github.com/kalanadidulanga/project-mouse</p>
        <p className="note">{s ? `System idle for ${clock(s.idle_secs)}` : " "}</p>
        <p className="note">
          It does not change your power plan, and it lets go of everything when you quit. Monitoring software can detect
          simulated input. With Move the mouse off it sends no input at all, so it cannot keep the screen from locking or
          keep a chat status active.
        </p>
      </section>

      <UpdateSettings />

      <section className="section">
        <h2>Import from Move Mouse</h2>
        <ImportMoveMouse />
      </section>

      <details className="section troubleshoot">
        <summary>Troubleshooting</summary>
        <h2>What Windows is being asked for</h2>
        <Readout diag={diag} />
        <WhyAwake />
        <h2>Recent activity</h2>
        <Activity />
        <LegacyRules />
      </details>
    </>
  );
}
```

Inside `WhyAwake`, change its heading to `<h2>Why is my PC awake?</h2>` if it is not already.
In `ImportMoveMouse`, keep the placeholder "Leave empty to find it, or paste the path to
Settings.xml".

- [ ] **Step 3: The final `App.tsx`.**

```tsx
// The window (spec 006 FR-024): seven tabs down the left, one concern each, as in Move Mouse.
import { useState } from "react";
import Home from "./home";
import Movement from "./movement";
import Behaviour from "./behaviour";
import { Blackouts, Schedules } from "./timetable";
import Appearance from "./appearance";
import About from "./about";
import { Icon, type IconName } from "./icons";
import "./styles.css";

export type Page = "home" | "movement" | "behaviour" | "schedules" | "blackouts" | "appearance" | "about";

const PAGES: [Page, string, IconName][] = [
  ["home", "Home", "home"],
  ["movement", "Movement", "movement"],
  ["behaviour", "Behaviour", "behaviour"],
  ["schedules", "Schedules", "schedules"],
  ["blackouts", "Blackouts", "blackouts"],
  ["appearance", "Appearance", "appearance"],
  ["about", "About", "about"],
];

function PageView({ page, go }: { page: Page; go: (p: Page) => void }) {
  switch (page) {
    case "movement":
      return <Movement />;
    case "behaviour":
      return <Behaviour />;
    case "schedules":
      return <Schedules />;
    case "blackouts":
      return <Blackouts />;
    case "appearance":
      return <Appearance />;
    case "about":
      return <About />;
    default:
      return <Home go={go} />;
  }
}

export default function App() {
  const [page, setPage] = useState<Page>("home");
  return (
    <div className="app">
      <nav className="rail" aria-label="Pages">
        <div className="brand">project-mouse</div>
        {PAGES.map(([id, label, icon]) => (
          <button
            key={id}
            className={page === id ? "active" : ""}
            aria-current={page === id ? "page" : undefined}
            onClick={() => setPage(id)}
          >
            <Icon name={icon} />
            {label}
          </button>
        ))}
      </nav>
      <main className="content">
        <PageView page={page} go={setPage} />
      </main>
    </div>
  );
}
```

Delete `src/advanced.tsx` and `src/rules.tsx`, and remove `<UpdateSettings />` (and its import)
from `behaviour.tsx`.

- [ ] **Step 4: Styles.** Append the following, and delete the `.builder`, `.cond`,
`.rail .spacer` and `.mono-hint` rules:

```css
details.troubleshoot > summary { cursor: default; font-size: 13px; font-weight: 600; padding: 4px 0; }
details.troubleshoot > summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
details.troubleshoot h2 { margin-top: 16px; }
```

- [ ] **Step 5: Gates.** Run tsc and the build. Run
`grep -rn "list_profiles\|set_profile\|create_profile\|delete_profile\|upsert_rule" src`, which must
be empty. Run the honesty grep and the em dash grep.

- [ ] **Step 6: Commit.**

```bash
git add -A src
git commit -m "feat(M8): Appearance and About tabs; Advanced retired into plain-words Troubleshooting (FR-015..FR-018, FR-028..FR-030)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: Docs

**Files:**
- Modify: `CHANGELOG.md` (an M8 block in `## Unreleased`)
- Modify: `README.md` (the "Using it" paragraph)
- Modify: `CLAUDE.md` (point at `specs/006-move-mouse-parity/plan.md`)

- [ ] **Step 1: CHANGELOG.** In `## Unreleased`, add a subsection `### Added (Move Mouse parity)`
above `### Changed`, with these bullets:
- Seven tabs: Home, Movement, Behaviour, Schedules, Blackouts, Appearance and About. Each setting
  has a short line, with the details behind a **?**.
- Every Move Mouse direction, random distance and wait, speed, and *Stop if I move the mouse*.
  Moves now land exactly and come back exactly, even at a screen edge.
- **Run for** 30 min to 4 h, or until a time.
- Pauses: on battery, when locked (on by default) and while presenting, plus Blackouts and
  Schedules.
- *Keep awake while these apps run* replaces the rule builder.
- Always on top, a taskbar dot, notifications for things you didn't do yourself, and live idle
  time.

Under `### Notes`, add these bullets:
- Config moves to v4. Old app rules become the apps list; any other old rule is listed under
  About, Troubleshooting.
- The Advanced tab, profiles and the "keep awake for a while" timer are gone from the window.

No em dashes.

- [ ] **Step 2: README.** Replace the "**Using it:**" paragraph with this one:

> **Using it:** open the app and press **Start**. *Next move in m:ss* counts down and restarts
> whenever you touch the mouse or keyboard. *Run for* stops it by itself. The tabs on the left
> hold everything else: how it moves, when it pauses, schedules, quiet times and appearance. Close
> the window and it keeps running in the tray, next to the clock. **Stop**, the tray menu or
> Ctrl+Alt+K turns it off.

- [ ] **Step 3: CLAUDE.md.** Change the plan pointer to `specs/006-move-mouse-parity/plan.md`, and
mention that `specs/005-start-stop/` holds M7.

- [ ] **Step 4: Check and commit.** `grep -nP "\x{2014}" CHANGELOG.md README.md CLAUDE.md` must be empty.

```bash
git add CHANGELOG.md README.md CLAUDE.md
git commit -m "docs(M8): CHANGELOG, README and CLAUDE.md for Move Mouse parity

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: Verify M7 and M8 together, for real

The controller runs steps 1–7, which need Kalana's hands for a few seconds at a time. Step 8 is the
SDD final whole-branch review. Step 9 is Kalana's.

- [ ] **Step 1: Gates.** Run `npm run build`, `cargo fmt --check`, clippy `-D warnings`,
`cargo test`, the honesty grep, the `cfg(windows)` boundary grep, and the em dash grep over all
tracked files except vendored spec-kit. Record each result.
- [ ] **Step 2: Launch** the dev build: quit any running instance, then `npm run tauri dev` in the
background. Using the scratchpad scripts from M7 (`windows.ps1`, `shot.ps1`):
  - **SC-001 (M7):** a plain launch shows the window; X gives 0 windows with the process alive; a
    second launch gives 1; `--minimized` gives 0.
  - Screenshot every tab at 760×540, in light and dark. Each must fit with at most a little
    scrolling (SC-010).
- [ ] **Step 3: Moves (M7 SC-002, M8 SC-001 and SC-003).**
  - Set the interval to 5 s and Start. Ask Kalana to keep their hands off for 15 s, and sample
    `GetCursorPos`. Do this for Square, NorthEast and Random; start and end positions must be
    equal.
  - Repeat with the cursor parked at the right edge of the screen; it must still end where it
    started.
  - Ask Kalana to touch the mouse during a move with *Stop if I move* on. The path stops, and the
    cursor stays where they put it.
- [ ] **Step 4: Pauses (SC-005, SC-008).**
  - Win+L, then unlock: the status reads Paused (locked), then resumes. Check that the power
    request stays held, using About ▸ Troubleshooting ("Why is my PC awake?").
  - If Kalana's laptop is available: unplug with *On battery* on. The status reads Paused and the
    request is released; plugging back in resumes.
  - Add a blackout covering now: Paused (blackout until HH:MM).
- [ ] **Step 5: Schedules, Run for and notifications (SC-008).**
  - Add a Start schedule one minute ahead while stopped. It starts, and a toast appears.
  - Run for "until" two minutes ahead. It stops, and a toast appears.
  - Taskbar dot: green while running, yellow while paused, none while stopped.
- [ ] **Step 6: Apps list.** Add `notepad.exe` and start Notepad while Stopped. Home reads
"Stopped, but notepad.exe is running…" and Troubleshooting shows sleep blocked. Close Notepad and
the line clears.
- [ ] **Step 7: Memory (SC-009).** Close the window, wait 30 s, and record the private working set.
- [ ] **Step 8: Final review.** Run the SDD final whole-branch review on the most capable model.
Give it a code-only diff since the M7 merge base, and point it at the ledger's deferred minors. Fix
the findings in one wave, then run one scoped re-review.
- [ ] **Step 9: Hand over to Kalana.**
  - With a 200 s interval, Teams stays Available through at least 10 minutes idle.
  - Install the NSIS build and check that launching from the Start menu opens the window.
  - If they choose, decide the merge to `main` and a release.

Report every result with the actual output, including anything that failed.
