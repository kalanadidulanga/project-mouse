//! Persisted config. Versioned from v1 with `#[serde(default)]` on every field so a config
//! written by an older build still deserializes (FEATURES D8). Autostart is NOT here — the
//! `HKCU\Run` key is its own persistence.

use serde::{Deserialize, Serialize};

use crate::core::modes::WakeMode;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

fn default_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub mode: WakeMode,
}

impl Default for Config {
    fn default() -> Self {
        Self { schema_version: CURRENT_SCHEMA_VERSION, mode: WakeMode::Off }
    }
}

impl Config {
    pub fn with_mode(mode: WakeMode) -> Self {
        Self { schema_version: CURRENT_SCHEMA_VERSION, mode }
    }
}
