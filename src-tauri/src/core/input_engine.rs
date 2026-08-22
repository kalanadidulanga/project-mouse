//! The opt-in input engine (FEATURES Part C). **Off by default.** Holds the self-injection filter
//! and, when enabled, jiggles while the human is idle — standing down the instant they return (C6)
//! and reporting when injection is silently discarded (C7). Trait-based, so `core/` stays OS-free.

use std::sync::Arc;

use crate::core::idle::IdleTracker;
use crate::platform::InputInjector;

const TOLERANCE_MS: u32 = 250;

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
        let due = self.last_jiggle.map_or(true, |t| now.wrapping_sub(t) >= self.interval_ms);
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
    fn stands_down_when_human_active() {
        let m = MockInjector::default();
        let mut e = engine(&m);
        e.set_enabled(true);
        e.tick(299_000, 300_000); // human idle only 1s (< stand-down)
        assert_eq!(*m.jiggles.lock().unwrap(), 0);
    }
}
