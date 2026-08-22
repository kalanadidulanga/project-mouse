//! The rule model (ARCHITECTURE §5). M2 is power-only: a rule contributes a `WakeMode` when its
//! conditions hold. Triggers/actions for the input engine arrive in M4.

use serde::{Deserialize, Serialize};

use crate::core::modes::WakeMode;

/// Mirrors `SHQueryUserNotificationState` (WINDOWS-API): one cheap call covers presentation,
/// fullscreen, game, and locked/screensaver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NotifState {
    /// Locked, screensaver, or inactive fast-user-switching session.
    NotPresent,
    /// A fullscreen application (e.g. video).
    Busy,
    /// Fullscreen exclusive Direct3D — a game.
    Game,
    /// Presentation mode.
    Presentation,
    /// Normal.
    #[default]
    Normal,
    /// Quiet hours.
    QuietTime,
    /// A Store app running full screen.
    App,
}

/// A guard evaluated against a `Snapshot`. Side-effect free.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    /// Any of these executable names is running (case-insensitive).
    ProcessRunning(Vec<String>),
    /// Local time window. `from`/`to` are minutes-of-day [0,1440); `from > to` crosses midnight.
    TimeWindow {
        days: [bool; 7],
        from: u16,
        to: u16,
    },
    /// Holds while `now < deadline` (unix seconds). Releases at the deadline.
    ExpiryAt(u64),
    OnACPower,
    /// Battery percentage at or above this value.
    BatteryAbove(u8),
    SessionUnlocked,
    /// Current notification state is one of these.
    NotificationStateIn(Vec<NotifState>),
    ForegroundAppIn(Vec<String>),
    ForegroundAppNotIn(Vec<String>),
    Not(Box<Condition>),
    AnyOf(Vec<Condition>),
    AllOf(Vec<Condition>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub conditions: Vec<Condition>,
    pub mode: WakeMode,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub rules: Vec<Rule>,
}

impl Profile {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            rules: Vec::new(),
        }
    }
}
