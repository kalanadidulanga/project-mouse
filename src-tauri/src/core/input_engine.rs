//! The opt-in input engine (FEATURES Part C). **Off by default.** Holds the self-injection filter
//! and, when enabled, jiggles while the human is idle — standing down the instant they return (C6)
//! and reporting when injection is silently discarded (C7). Trait-based, so `core/` stays OS-free.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::idle::IdleTracker;
use crate::platform::InputInjector;

const TOLERANCE_MS: u32 = 250;

/// The user-settable input-engine knobs (M4). Clamped in `InputEngine::set_settings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSettings {
    /// Seconds between jiggles.
    pub interval_secs: u32,
    /// Only jiggle once the human has been idle this long.
    pub idle_threshold_secs: u32,
    /// 0 = virtual jiggle (the honest default, PRODUCT §5); otherwise a virtual-key code.
    pub key: u16,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            idle_threshold_secs: 60,
            key: 0,
        }
    }
}

fn abs_diff(a: u32, b: u32) -> u32 {
    a.wrapping_sub(b).min(b.wrapping_sub(a))
}

pub struct InputEngine {
    injector: Arc<dyn InputInjector>,
    tracker: IdleTracker,
    enabled: bool,
    pub blocked: bool,
    pub system_idle_ms: u32,
    pub human_idle_ms: u32,
    interval_ms: u32,
    idle_threshold_ms: u32,
    stand_down_ms: u32,
    /// 0 = virtual jiggle; otherwise a keystroke of this virtual-key code.
    key: u16,
    last_jiggle: Option<u32>,
    pending_verify: Option<u32>,
}

impl InputEngine {
    pub fn new(injector: Arc<dyn InputInjector>, now: u32) -> Self {
        Self {
            injector,
            tracker: IdleTracker::new(now),
            enabled: false,
            blocked: false,
            system_idle_ms: 0,
            human_idle_ms: 0,
            interval_ms: 60_000,
            idle_threshold_ms: 60_000,
            stand_down_ms: 2_000,
            key: 0,
            last_jiggle: None,
            pending_verify: None,
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.blocked = false;
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The user-settable knobs. Clamped here so a bad config or a typo in the UI cannot produce a
    /// runaway injector: interval 5 s–1 h, idle threshold 0–24 h.
    pub fn set_settings(&mut self, s: InputSettings) {
        self.interval_ms = s.interval_secs.clamp(5, 3_600) * 1000;
        self.idle_threshold_ms = s.idle_threshold_secs.min(86_400) * 1000;
        self.key = s.key;
    }

    pub fn settings(&self) -> InputSettings {
        InputSettings {
            interval_secs: self.interval_ms / 1000,
            idle_threshold_secs: self.idle_threshold_ms / 1000,
            key: self.key,
        }
    }

    /// One tick. `last_input_tick` = `GetLastInputInfo.dwTime`, `now` = `GetTickCount` (same domain).
    pub fn tick(&mut self, last_input_tick: u32, now: u32) {
        // C7: verify the previous injection actually reset the idle timer. If the system's
        // last-input tick did not move to ~our injection, it was silently discarded (UIPI).
        if let Some(inj) = self.pending_verify.take() {
            self.blocked = abs_diff(last_input_tick, inj) >= TOLERANCE_MS;
        }
        self.tracker.observe(last_input_tick);
        self.system_idle_ms = self.tracker.system_idle_ms(last_input_tick, now);
        self.human_idle_ms = self.tracker.human_idle_ms(now);

        if !self.enabled {
            return;
        }
        // C6 stand-down: the human is active → do nothing.
        if self.human_idle_ms < self.stand_down_ms {
            return;
        }
        let due = self
            .last_jiggle
            .is_none_or(|t| now.wrapping_sub(t) >= self.interval_ms);
        if self.human_idle_ms >= self.idle_threshold_ms && due {
            let res = if self.key != 0 {
                self.injector.key(self.key)
            } else {
                self.injector.virtual_jiggle()
            };
            match res {
                Ok(()) => {
                    self.tracker.note_injection(now);
                    self.last_jiggle = Some(now);
                    self.pending_verify = Some(now);
                }
                Err(e) => {
                    self.blocked = true;
                    tracing::warn!("injection failed: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::mock::MockInjector;

    fn engine(m: &MockInjector) -> InputEngine {
        InputEngine::new(Arc::new(m.clone()), 0)
    }

    #[test]
    fn off_by_default_never_injects() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.tick(0, 300_000); // human idle 5 min, but disabled
        assert_eq!(*m.jiggles.lock().unwrap(), 0);
    }

    #[test]
    fn jiggles_when_enabled_and_idle() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_enabled(true);
        e.tick(0, 300_000);
        assert_eq!(*m.jiggles.lock().unwrap(), 1);
    }

    #[test]
    fn settings_round_trip_and_clamp() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_settings(InputSettings {
            interval_secs: 1, // below the floor
            idle_threshold_secs: 300,
            key: 0x7E, // VK_F15
        });
        let got = e.settings();
        assert_eq!(got.interval_secs, 5, "interval clamped to the 5 s floor");
        assert_eq!(got.idle_threshold_secs, 300);
        assert_eq!(got.key, 0x7E);
    }

    #[test]
    fn honours_the_idle_threshold() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_enabled(true);
        e.set_settings(InputSettings {
            interval_secs: 60,
            idle_threshold_secs: 600,
            key: 0,
        });
        e.tick(0, 300_000); // human idle 5 min < the 10 min threshold
        assert_eq!(*m.jiggles.lock().unwrap(), 0);
        e.tick(0, 700_000); // now past it
        assert_eq!(*m.jiggles.lock().unwrap(), 1);
    }

    #[test]
    fn stands_down_when_human_active() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_enabled(true);
        e.tick(299_000, 300_000); // human idle only 1s (< stand-down)
        assert_eq!(*m.jiggles.lock().unwrap(), 0);
    }
}
