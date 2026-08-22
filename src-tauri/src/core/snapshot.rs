//! The sampled state a tick evaluates rules against. Plain data — the OS-specific sampling that
//! fills it lives behind `platform/` (Phase 3), which is what keeps the evaluator Win32-free and
//! unit-testable: a test just builds a `Snapshot` by hand.

use crate::core::rule::NotifState;

#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Unix seconds — for expiry comparisons.
    pub epoch_secs: u64,
    /// Local weekday, 0 = Monday .. 6 = Sunday.
    pub weekday: u8,
    /// Local minutes-of-day, 0..1440.
    pub minutes: u16,
    /// Executable names currently running (cadence-limited sample).
    pub running_processes: Vec<String>,
    /// Foreground window's executable name, if known.
    pub foreground_exe: Option<String>,
    pub session_locked: bool,
    pub notification_state: NotifState,
    pub on_ac: bool,
    pub battery_pct: u8,
    // Sampled for conditions that arrive later: remote-session (B6), CPU load (B4), idle gating (M4).
    #[allow(dead_code)]
    pub remote_session: bool,
    #[allow(dead_code)]
    pub cpu_pct: u8,
    #[allow(dead_code)]
    pub idle_ms: u64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            epoch_secs: 0,
            weekday: 0,
            minutes: 0,
            running_processes: Vec::new(),
            foreground_exe: None,
            session_locked: false,
            remote_session: false,
            notification_state: NotifState::Normal,
            on_ac: true,
            battery_pct: 100,
            cpu_pct: 0,
            idle_ms: 0,
        }
    }
}
