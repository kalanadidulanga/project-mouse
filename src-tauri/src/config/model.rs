//! Persisted config. Versioned from v1 with `#[serde(default)]` on every field so a config written
//! by an older build still deserializes (FEATURES D8). v2 adds profiles + the active profile.

use serde::{Deserialize, Serialize};

use crate::core::modes::WakeMode;
use crate::core::rule::Profile;

pub const CURRENT_SCHEMA_VERSION: u32 = 2;

fn default_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub schema_version: u32,
    /// The manual override set from the tray/UI.
    #[serde(default)]
    pub mode: WakeMode,
    /// User-defined profiles (rule sets). Empty on a fresh install.
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Id of the active profile, or empty for none.
    #[serde(default)]
    pub active_profile: String,
    /// The opt-in input engine. **Off by default** (FEATURES Part C).
    #[serde(default)]
    pub input_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            mode: WakeMode::Off,
            profiles: Vec::new(),
            active_profile: String::new(),
            input_enabled: false,
        }
    }
}

impl Config {
    #[allow(dead_code)] // convenience ctor used by tests; kept as a small public API
    pub fn with_mode(mode: WakeMode) -> Self {
        Self {
            mode,
            ..Default::default()
        }
    }

    /// The active profile, if one is set and exists.
    pub fn active(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == self.active_profile)
    }
}
