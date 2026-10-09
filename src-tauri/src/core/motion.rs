//! Cursor movement (FEATURES C2, spec 006 FR-001 to FR-004) and the honest kind of randomness.
//!
//! Pure: the actual `SendInput` lives behind `platform::InputInjector`. Every path here is a
//! **closed** one. Its steps sum to zero, so the cursor ends where the user left it.
//!
//! Randomness here exists so moves do not line up with other timers and the cursor does not land
//! on the same pixel every time. It is not here to look human, and no string in this file or the
//! UI may say that it is (PRODUCT section 5, constitution II).

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
    #[allow(dead_code)]
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
    /// Warning: Relative distances pass through pointer acceleration when sent as relative moves. The
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
    /// Milliseconds between path steps. `custom` applies to `Custom` only and is held to 1 to 50.
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

/// Deterministic, tiny, and good enough to stop a value being identical every time. Not for
/// anything that matters cryptographically, and it is not pretending to be.
fn xorshift(seed: u32) -> u32 {
    let mut x = seed | 1;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

/// A value in `[lo, hi]`, drawn from `seed` (spec 006 FR-003, FR-006). `lo >= hi` returns `lo`.
#[allow(dead_code)]
pub fn pick(lo: u32, hi: u32, seed: u32) -> u32 {
    if hi <= lo {
        return lo;
    }
    lo + xorshift(seed) % (hi - lo + 1)
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
                    assert_eq!(
                        sum(&path(m, d, seed)),
                        (0, 0),
                        "{m:?} at {d}px, seed {seed}"
                    );
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
        assert_eq!(
            path(Motion::North, 10, 0)[0],
            (0, -2),
            "north is up the screen"
        );
        let ne = path(Motion::NorthEast, 10, 0)[0];
        assert!(ne.0 > 0 && ne.1 < 0, "north-east goes right and up: {ne:?}");
        let sw = path(Motion::SouthWest, 10, 0)[0];
        assert!(
            sw.0 < 0 && sw.1 > 0,
            "south-west goes left and down: {sw:?}"
        );
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
        let compass: Vec<(i32, i32)> = [
            North, NorthEast, East, SouthEast, South, SouthWest, West, NorthWest,
        ]
        .iter()
        .map(|&m| path(m, 10, 0)[0])
        .collect();
        let starts: HashSet<(i32, i32)> = (0..64).map(|s| path(Random, 10, s)[0]).collect();
        assert!(
            starts.len() >= 4,
            "only {} directions in 64 seeds",
            starts.len()
        );
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
}
