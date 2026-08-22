//! The single owner of the power state. The desired mode is the maximum of the **manual** override
//! (set from the tray) and whatever the **active profile's** rules contribute for the current
//! `Snapshot`. Reconciled each tick; the reconciler makes repeated identical ticks a no-op.

use std::sync::Arc;

use crate::core::evaluator::desired_mode;
use crate::core::modes::WakeMode;
use crate::core::rule::{Profile, Rule};
use crate::core::snapshot::Snapshot;
use crate::platform::PowerGuard;
use crate::power::PowerReconciler;

pub struct Engine {
    reconciler: PowerReconciler,
    manual: WakeMode,
    profile: Profile,
    paused: bool,
    last: WakeMode,
}

impl Engine {
    pub fn new(power: Arc<dyn PowerGuard>) -> Self {
        Self {
            reconciler: PowerReconciler::new(power),
            manual: WakeMode::Off,
            profile: Profile::new("default", "Default"),
            paused: false,
            last: WakeMode::Off,
        }
    }

    pub fn set_manual(&mut self, mode: WakeMode) {
        self.manual = mode;
    }

    pub fn manual(&self) -> WakeMode {
        self.manual
    }

    pub fn set_profile(&mut self, profile: Profile) {
        self.profile = profile;
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn profile_name(&self) -> &str {
        &self.profile.name
    }

    pub fn upsert_rule(&mut self, rule: Rule) {
        match self.profile.rules.iter_mut().find(|r| r.id == rule.id) {
            Some(existing) => *existing = rule,
            None => self.profile.rules.push(rule),
        }
    }

    pub fn delete_rule(&mut self, id: &str) {
        self.profile.rules.retain(|r| r.id != id);
    }

    pub fn set_rule_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(r) = self.profile.rules.iter_mut().find(|r| r.id == id) {
            r.enabled = enabled;
        }
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// The effective mode currently held (after the last tick).
    pub fn mode(&self) -> WakeMode {
        self.last
    }

    /// Recompute desired = max(manual, rules) and reconcile. Idempotent across identical ticks.
    /// While paused, everything is suppressed (desired = Off).
    pub fn tick(&mut self, snap: &Snapshot) {
        let desired = if self.paused {
            WakeMode::Off
        } else {
            self.manual.max(desired_mode(&self.profile, snap))
        };
        if desired != self.last {
            tracing::info!(?desired, "reconciling wake mode");
            self.last = desired;
        }
        if let Err(e) = self.reconciler.reconcile(desired) {
            tracing::error!("reconcile failed: {e}");
        }
    }

    pub fn release(&mut self) {
        if let Err(e) = self.reconciler.release() {
            tracing::error!("failed to release power request: {e}");
        }
        self.manual = WakeMode::Off;
        self.last = WakeMode::Off;
    }
}
