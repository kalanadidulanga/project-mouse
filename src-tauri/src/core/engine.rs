//! The single owner of the power state. The desired mode is the maximum of the **manual** override
//! (set from the tray) and whatever the **active profile's** rules contribute for the current
//! `Snapshot`. Reconciled each tick; the reconciler makes repeated identical ticks a no-op.

use std::sync::Arc;

use crate::core::evaluator::desired_mode;
use crate::core::modes::WakeMode;
use crate::core::rule::Profile;
use crate::core::snapshot::Snapshot;
use crate::platform::PowerGuard;
use crate::power::PowerReconciler;

pub struct Engine {
    reconciler: PowerReconciler,
    manual: WakeMode,
    manual_suspended: bool,
    profile: Profile,
    last: WakeMode,
}

impl Engine {
    pub fn new(power: Arc<dyn PowerGuard>) -> Self {
        Self {
            reconciler: PowerReconciler::new(power),
            manual: WakeMode::Off,
            manual_suspended: false,
            profile: Profile::new("default", "Default"),
            last: WakeMode::Off,
        }
    }

    pub fn set_manual(&mut self, mode: WakeMode) {
        self.manual = mode;
    }

    /// A battery pause (spec 006 FR-012): Start stays on, but its power request lets go. Rules,
    /// such as "keep awake while these apps run", still hold.
    pub fn set_manual_suspended(&mut self, on: bool) {
        self.manual_suspended = on;
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

    pub fn delete_rule(&mut self, id: &str) {
        self.profile.rules.retain(|r| r.id != id);
    }

    pub fn set_rule_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(r) = self.profile.rules.iter_mut().find(|r| r.id == id) {
            r.enabled = enabled;
        }
    }

    /// The effective mode currently held (after the last tick).
    pub fn mode(&self) -> WakeMode {
        self.last
    }

    /// Recompute desired = max(manual, rules) and reconcile. Idempotent across identical ticks.
    pub fn tick(&mut self, snap: &Snapshot) {
        let manual = if self.manual_suspended {
            WakeMode::Off
        } else {
            self.manual
        };
        let desired = manual.max(desired_mode(&self.profile, snap));
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
