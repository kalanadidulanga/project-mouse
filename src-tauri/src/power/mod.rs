//! The power engine: desired-vs-actual reconciliation, single owner, idempotent.
//! Holds a *state*, never fires events (ARCHITECTURE §0, §5). The scheduler thread owns one
//! `PowerReconciler` for the process lifetime.

use std::sync::Arc;

use crate::core::modes::WakeMode;
use crate::platform::{PowerGuard, Result};

pub struct PowerReconciler {
    guard: Arc<dyn PowerGuard>,
    current: WakeMode,
}

impl PowerReconciler {
    pub fn new(guard: Arc<dyn PowerGuard>) -> Self {
        Self {
            guard,
            current: WakeMode::Off,
        }
    }

    #[allow(dead_code)] // read by tests now; by the IPC/diagnostics surface in M3
    pub fn current(&self) -> WakeMode {
        self.current
    }

    /// Drive the held request to `desired`. Idempotent: calling with the mode already held is a
    /// no-op (the request is acquired once, not re-acquired every tick).
    pub fn reconcile(&mut self, desired: WakeMode) -> Result<()> {
        if desired == self.current {
            return Ok(());
        }
        match desired {
            WakeMode::Off => self.guard.clear()?,
            other => self.guard.set(other.keeps_display(), other.reason())?,
        }
        self.current = desired;
        Ok(())
    }

    /// Release everything and return to Off — for shutdown and the panic hook.
    pub fn release(&mut self) -> Result<()> {
        self.guard.clear()?;
        self.current = WakeMode::Off;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::modes::WakeMode::*;
    use crate::platform::mock::MockPowerGuard;

    #[test]
    fn acquires_once_and_is_idempotent() {
        let mock = MockPowerGuard::default();
        let mut rec = PowerReconciler::new(Arc::new(mock.clone()));

        rec.reconcile(KeepRunning).unwrap();
        rec.reconcile(KeepRunning).unwrap();
        rec.reconcile(KeepRunning).unwrap();

        // set called exactly once despite three ticks
        assert_eq!(mock.calls().len(), 1);
        assert_eq!(mock.held(), Some(false)); // KeepRunning: system+execution, no display
        assert_eq!(rec.current(), KeepRunning);
    }

    #[test]
    fn presenting_holds_display() {
        let mock = MockPowerGuard::default();
        let mut rec = PowerReconciler::new(Arc::new(mock.clone()));
        rec.reconcile(KeepPresenting).unwrap();
        assert_eq!(mock.held(), Some(true));
    }

    #[test]
    fn off_clears_the_request() {
        let mock = MockPowerGuard::default();
        let mut rec = PowerReconciler::new(Arc::new(mock.clone()));
        rec.reconcile(KeepRunning).unwrap();
        rec.reconcile(Off).unwrap();
        assert_eq!(mock.held(), None);
        assert_eq!(rec.current(), Off);
        assert_eq!(mock.calls(), vec!["set(display=false, reason=project-mouse: Keep running (system awake, display may sleep))".to_string(), "clear".to_string()]);
    }

    #[test]
    fn switching_modes_re_applies() {
        let mock = MockPowerGuard::default();
        let mut rec = PowerReconciler::new(Arc::new(mock.clone()));
        rec.reconcile(KeepRunning).unwrap();
        rec.reconcile(KeepPresenting).unwrap();
        assert_eq!(mock.held(), Some(true));
        // set, set — two distinct applications
        assert_eq!(mock.calls().len(), 2);
    }

    #[test]
    fn release_returns_to_off() {
        let mock = MockPowerGuard::default();
        let mut rec = PowerReconciler::new(Arc::new(mock.clone()));
        rec.reconcile(KeepPresenting).unwrap();
        rec.release().unwrap();
        assert_eq!(rec.current(), Off);
        assert_eq!(mock.held(), None);
    }
}
