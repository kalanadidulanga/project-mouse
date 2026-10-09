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
    /// A visible 10 px square after a minute with no input — what someone arriving from Move
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
        // C5: vary once per cycle, so the countdown is steady. Seeded from the tick — no RNG state.
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
        e.tick(301_000, 301_500); // 800 ms after the path ended — that is a person
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
}
