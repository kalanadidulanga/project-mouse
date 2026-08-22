//! The entire power surface of the product: three modes. FEATURES A1.

use serde::{Deserialize, Serialize};

/// Declaration order is the strength order — `Off < KeepRunning < KeepPresenting` —
/// so `#[derive(Ord)]` gives combine-by-maximum for free (ARCHITECTURE §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum WakeMode {
    #[default]
    Off,
    KeepRunning,
    KeepPresenting,
}

impl WakeMode {
    /// Whether the display must be kept on. Both awake modes hold system+execution;
    /// only KeepPresenting additionally holds the display.
    pub fn keeps_display(self) -> bool {
        matches!(self, WakeMode::KeepPresenting)
    }

    /// Human-readable `powercfg /requests` reason string (FEATURES A3).
    pub fn reason(self) -> &'static str {
        match self {
            WakeMode::Off => "project-mouse: not holding",
            WakeMode::KeepRunning => "project-mouse: Keep running (system awake, display may sleep)",
            WakeMode::KeepPresenting => "project-mouse: Keep presenting (system awake, display on)",
        }
    }

    /// Combine contributions by maximum. A weaker contribution can never lower a stronger
    /// one (ARCHITECTURE §5). Empty → Off.
    #[allow(dead_code)] // exercised by tests now; the reconcile-from-many-rules caller lands in M2
    pub fn combine(contributions: impl IntoIterator<Item = WakeMode>) -> WakeMode {
        contributions.into_iter().max().unwrap_or(WakeMode::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::WakeMode::*;
    use super::*;

    #[test]
    fn strength_ordering() {
        assert!(Off < KeepRunning);
        assert!(KeepRunning < KeepPresenting);
    }

    #[test]
    fn combine_takes_the_maximum() {
        assert_eq!(WakeMode::combine([Off, KeepRunning, KeepPresenting]), KeepPresenting);
        assert_eq!(WakeMode::combine([Off, KeepRunning]), KeepRunning);
        assert_eq!(WakeMode::combine([Off, Off]), Off);
        assert_eq!(WakeMode::combine([]), Off);
    }

    #[test]
    fn only_presenting_keeps_display() {
        assert!(!Off.keeps_display());
        assert!(!KeepRunning.keeps_display());
        assert!(KeepPresenting.keeps_display());
    }
}
