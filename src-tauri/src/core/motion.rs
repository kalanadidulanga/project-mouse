//! Visible cursor movement (FEATURES C2) and the honest kind of randomisation (C5).
//!
//! Pure, the actual `SendInput` lives behind `platform::InputInjector`. Every path here is a
//! **closed** one: the steps of a full cycle sum to zero, so the cursor is back where the user
//! left it. C2 calls this "return to origin", and it is what stops a jiggler from walking the
//! pointer across the desk over an afternoon.
//!
//! On C5, read `docs/FEATURES.md` before touching this: variation ships as *"vary the movement so
//! it is less intrusive"* and nothing else. It is not here to look human, and no string in this
//! file or the UI may say that it is.

use serde::{Deserialize, Serialize};

/// What the injector does on each trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Motion {
    /// C1 virtual jiggle: resets the idle timer, cursor does not move. The default, and the only
    /// one that cannot embarrass someone mid-presentation.
    #[default]
    Virtual,
    /// Back and forth along one axis. Two steps.
    Line,
    /// Around the four sides of a square. Four steps.
    Square,
    /// Eight steps around a rough circle.
    Circle,
}

impl Motion {
    /// Steps in one full cycle. `Virtual` never moves, so its cycle is a single no-op step.
    pub fn steps(self) -> u32 {
        match self {
            Motion::Virtual => 1,
            Motion::Line => 2,
            Motion::Square => 4,
            Motion::Circle => 8,
        }
    }

    /// The relative move for step `index` of the cycle, in pixels.
    ///
    /// ⚠️ Relative moves pass through pointer acceleration, so `distance` is a request, not a
    /// promise (FEATURES C2). That is fine here, nothing depends on landing exactly, and the
    /// cycle closing is guaranteed by construction rather than by arithmetic on the way back.
    pub fn step(self, index: u32, distance: i32) -> (i32, i32) {
        let i = index % self.steps();
        match self {
            Motion::Virtual => (0, 0),
            Motion::Line => {
                if i == 0 {
                    (distance, 0)
                } else {
                    (-distance, 0)
                }
            }
            Motion::Square => match i {
                0 => (distance, 0),
                1 => (0, distance),
                2 => (-distance, 0),
                _ => (0, -distance),
            },
            Motion::Circle => {
                // Second half is the negation of the first, so the cycle closes exactly no matter
                // how the octant is rounded.
                const OCTANT: [(i32, i32); 4] = [(7, 3), (3, 7), (-3, 7), (-7, 3)];
                let (nx, ny) = OCTANT[(i % 4) as usize];
                let (x, y) = (nx * distance / 10, ny * distance / 10);
                if i < 4 {
                    (x, y)
                } else {
                    (-x, -y)
                }
            }
        }
    }
}

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

/// Deterministic, tiny, and good enough to stop a value being identical every time. Not for
/// anything that matters cryptographically, and it is not pretending to be.
fn xorshift(seed: u32) -> u32 {
    let mut x = seed | 1;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

/// Vary `value` by up to ±`pct` percent (C5). `pct` 0 leaves it alone.
///
/// Its purpose is that a fixed interval synchronises badly with other periodic events, and a
/// cursor that always lands on the same pixel eventually lands somewhere it should not.
pub fn vary(value: u32, pct: u32, seed: u32) -> u32 {
    if pct == 0 || value == 0 {
        return value;
    }
    let pct = pct.min(100);
    let span = (value as u64 * pct as u64 / 100).max(1);
    let offset = (xorshift(seed) as u64) % (span * 2 + 1);
    let varied = value as i64 + offset as i64 - span as i64;
    varied.max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cycle_sum(m: Motion, distance: i32) -> (i32, i32) {
        (0..m.steps()).fold((0, 0), |(x, y), i| {
            let (dx, dy) = m.step(i, distance);
            (x + dx, y + dy)
        })
    }

    /// The property the whole module exists for: a full cycle puts the cursor back. Without it a
    /// jiggler walks the pointer off the screen over a long idle afternoon.
    #[test]
    fn every_pattern_returns_to_its_origin() {
        for m in [
            Motion::Virtual,
            Motion::Line,
            Motion::Square,
            Motion::Circle,
        ] {
            assert_eq!(cycle_sum(m, 40), (0, 0), "{m:?} did not close its cycle");
        }
    }

    #[test]
    fn the_default_motion_never_moves_the_cursor() {
        assert_eq!(Motion::default(), Motion::Virtual);
        for i in 0..10 {
            assert_eq!(Motion::Virtual.step(i, 500), (0, 0));
        }
    }

    #[test]
    fn a_square_walks_its_four_sides_in_order() {
        let m = Motion::Square;
        assert_eq!(m.step(0, 10), (10, 0));
        assert_eq!(m.step(1, 10), (0, 10));
        assert_eq!(m.step(2, 10), (-10, 0));
        assert_eq!(m.step(3, 10), (0, -10));
    }

    #[test]
    fn the_step_index_wraps_so_a_long_idle_keeps_cycling() {
        let m = Motion::Square;
        assert_eq!(m.step(4, 10), m.step(0, 10));
        assert_eq!(m.step(4001, 10), m.step(1, 10));
    }

    #[test]
    fn zero_distance_moves_nothing() {
        for m in [Motion::Line, Motion::Square, Motion::Circle] {
            for i in 0..m.steps() {
                assert_eq!(m.step(i, 0), (0, 0), "{m:?} moved with distance 0");
            }
        }
    }

    #[test]
    fn variation_of_zero_percent_is_the_identity() {
        for seed in [0, 1, 7, 999_999] {
            assert_eq!(vary(60, 0, seed), 60);
        }
    }

    #[test]
    fn variation_stays_within_the_requested_band() {
        // ±25% of 60 is [45, 75].
        for seed in 0..500 {
            let v = vary(60, 25, seed);
            assert!(
                (45..=75).contains(&v),
                "seed {seed} produced {v}, outside +/-25%"
            );
        }
    }

    #[test]
    fn variation_actually_varies() {
        let values: std::collections::HashSet<u32> = (0..50).map(|s| vary(60, 25, s)).collect();
        assert!(values.len() > 5, "only {} distinct values", values.len());
    }

    /// An interval of zero would be a busy loop, so the floor is load-bearing, not cosmetic.
    #[test]
    fn variation_never_returns_zero() {
        for seed in 0..500 {
            assert!(vary(1, 100, seed) >= 1);
        }
    }

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
            assert!(
                p.len() > m.steps() as usize,
                "{m:?} jumps instead of gliding: {p:?}"
            );
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
}
