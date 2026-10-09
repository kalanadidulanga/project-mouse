//! The input engine (FEATURES Part C). Enabled only while running with *Move the mouse* on
//! (spec 005 FR-003). It moves once the PC has had no input for the interval, traces a whole
//! closed path each time (FR-005), never mistakes its own path for the user (FR-006), and
//! reports when Windows silently discards the input (C7).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::idle::{self, IdleTracker};
use crate::core::motion::{self, Motion, Speed};
use crate::platform::InputInjector;

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

pub struct InputEngine {
    injector: Arc<dyn InputInjector>,
    tracker: IdleTracker,
    enabled: bool,
    pub blocked: bool,
    pub system_idle_ms: u32,
    pub human_idle_ms: u32,
    /// The settings, already clamped.
    s: InputSettings,
    /// This cycle's interval: the interval drawn once, when the cycle starts, so the countdown
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
            s: InputSettings::default(),
            cycle_ms: 0,
            last_move: None,
            pending_verify: None,
            next_move_in_ms: None,
        };
        e.set_settings(InputSettings::default());
        e
    }

    pub fn set_enabled(&mut self, on: bool) {
        // A warning from Test (or any earlier run) must not outlive the switch.
        if !on || !self.enabled {
            self.blocked = false;
        }
        self.enabled = on;
        // Pressing Start is itself input, so a whole cycle is what is left.
        self.next_move_in_ms = on.then_some(self.cycle_ms);
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Clamped so a bad config or a typo cannot produce a runaway injector: interval 5 s-1 h,
    /// distance 1-500 px, custom speed 1-50 ms per step. A swapped min and max is fine: the draw
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
        let secs = if self.s.interval_random {
            motion::pick(a.min(b), a.max(b), seed)
        } else {
            a
        };
        secs * 1000
    }

    /// This move's distance: fixed, or drawn between the two ends (FR-003).
    fn draw_distance(&self, seed: u32) -> i32 {
        let (a, b) = (self.s.distance_px as u32, self.s.distance_max_px as u32);
        let d = if self.s.distance_random {
            motion::pick(a.min(b), a.max(b), seed)
        } else {
            a
        };
        d as i32
    }

    #[cfg(test)]
    fn cycle_ms(&self) -> u32 {
        self.cycle_ms
    }

    /// Seconds until the next move, rounded up. `None` while disabled.
    pub fn next_move_in_secs(&self) -> Option<u32> {
        self.next_move_in_ms.map(|ms| ms.div_ceil(1000))
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

    /// One tick. `last_input_tick` = `GetLastInputInfo.dwTime`, `now` = `GetTickCount` (same domain).
    pub fn tick(&mut self, last_input_tick: u32, now: u32) {
        // C7: did the last move actually reset the idle clock? If Windows' last-input tick is not
        // inside our span, the input was silently discarded (UIPI).
        if let Some((start, _end)) = self.pending_verify.take() {
            // Only "still older than the span" means discarded: real input after it is the user.
            self.blocked = idle::before_span(last_input_tick, start);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::motion::Speed;
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
    fn defaults_are_a_visible_square_after_a_minute_that_lets_go_when_touched() {
        let d = InputSettings::default();
        assert_eq!(
            (d.interval_secs, d.interval_random, d.interval_max_secs),
            (60, false, 120)
        );
        assert_eq!((d.key, d.motion), (0, Motion::Square));
        assert_eq!(
            (d.distance_px, d.distance_random, d.distance_max_px),
            (10, false, 20)
        );
        assert_eq!(
            (d.speed, d.custom_step_ms, d.abortable),
            (Speed::Normal, 10, true)
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
        assert_eq!(
            m.moves.lock().unwrap().len(),
            20,
            "one trigger, one whole square"
        );
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
        assert!(
            cycles.iter().all(|c| (60_000..=120_000).contains(c)),
            "{cycles:?}"
        );
        assert!(cycles.len() > 3, "only {} distinct waits", cycles.len());
    }

    #[test]
    fn a_fixed_interval_ignores_the_max_and_a_swapped_range_still_works() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            interval_max_secs: 9,
            ..every(60, Motion::Virtual)
        });
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
            let d: i32 = m
                .moves
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.0 > 0)
                .map(|s| s.0)
                .sum();
            assert!((10..=20).contains(&d), "seed {seed}: {d}px");
        }
    }

    #[test]
    fn speed_and_abort_reach_the_injector() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            speed: Speed::Slow,
            abortable: false,
            ..every(5, Motion::Square)
        });
        e.move_now(1);
        assert_eq!(*m.last_step_ms.lock().unwrap(), Some(20));
        assert_eq!(*m.last_abortable.lock().unwrap(), Some(false));
        e.set_settings(InputSettings {
            speed: Speed::Custom,
            custom_step_ms: 0,
            ..every(5, Motion::Square)
        });
        e.move_now(2);
        assert_eq!(
            *m.last_step_ms.lock().unwrap(),
            Some(1),
            "custom is held to 1..=50"
        );
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
        assert_eq!(
            m.moves.lock().unwrap().len(),
            3,
            "it stopped after the user grabbed the mouse"
        );
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
        assert!(
            m.moves.lock().unwrap().is_empty(),
            "Invisible moved the cursor"
        );
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

    #[test]
    fn real_input_after_the_span_is_not_a_blocked_move() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.move_now(1_000);
        e.tick(2_000, 2_100);
        assert!(!e.blocked);
    }

    #[test]
    fn starting_clears_a_warning_left_by_a_failed_test() {
        let m = MockInjector::default();
        m.fail.store(true, Ordering::SeqCst);
        let mut e = engine(&m);
        e.move_now(1_000);
        assert!(e.blocked);
        e.set_enabled(true);
        assert!(!e.blocked);
    }
}
