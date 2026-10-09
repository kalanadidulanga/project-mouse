//! Persisted config. Versioned, with `#[serde(default)]` on every field so a config written by an
//! older build still deserializes (FEATURES D8). v2 added profiles; v3 (spec 005) replaced the
//! saved mode and the input switch with Start/Stop and its settings.

use serde::{Deserialize, Serialize};

use crate::core::input_engine::InputSettings;
use crate::core::rule::Profile;
use crate::core::running::RunSettings;

pub const CURRENT_SCHEMA_VERSION: u32 = 3;

fn default_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub schema_version: u32,
    /// User-defined profiles (rule sets). Empty on a fresh install.
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Id of the active profile, or empty for none.
    #[serde(default)]
    pub active_profile: String,
    /// How the mouse moves while running.
    #[serde(default)]
    pub input: InputSettings,
    /// What Start means.
    #[serde(default)]
    pub run: RunSettings,
    /// Check for updates in the background (UPDATES.md §6). Default on, and a switch, because
    /// some people run this on machines where outbound requests get noticed.
    #[serde(default = "default_true")]
    pub auto_update: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            profiles: Vec::new(),
            active_profile: String::new(),
            input: InputSettings::default(),
            run: RunSettings::default(),
            auto_update: true,
        }
    }
}

impl Config {
    /// The active profile, if one is set and exists.
    pub fn active(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == self.active_profile)
    }
}
