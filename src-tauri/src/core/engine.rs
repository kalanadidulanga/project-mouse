//! The single owner of the power state. In M1 there are no conditions, so the mode is set
//! directly and reconciled on change — no tick loop needed yet (that arrives with conditions in
//! M2). ponytail: reconcile-on-change keeps idle CPU at zero until M2 needs periodic evaluation.

use std::sync::Arc;

use crate::core::modes::WakeMode;
use crate::platform::PowerGuard;
use crate::power::PowerReconciler;

pub struct Engine {
    reconciler: PowerReconciler,
    mode: WakeMode,
}

impl Engine {
    pub fn new(power: Arc<dyn PowerGuard>) -> Self {
        Self { reconciler: PowerReconciler::new(power), mode: WakeMode::Off }
    }

    pub fn mode(&self) -> WakeMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: WakeMode) {
        match self.reconciler.reconcile(mode) {
            Ok(()) => {
                self.mode = mode;
                tracing::info!(?mode, "wake mode set");
            }
            Err(e) => tracing::error!("failed to set wake mode {mode:?}: {e}"),
        }
    }

    pub fn release(&mut self) {
        if let Err(e) = self.reconciler.release() {
            tracing::error!("failed to release power request: {e}");
        }
        self.mode = WakeMode::Off;
    }
}
