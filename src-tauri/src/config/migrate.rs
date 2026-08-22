//! Versioned migration chain (FEATURES D8). A parse failure here is surfaced, never silently
//! reset. Only v1 exists today; the shape is ready for v2+ step migrations.

use super::model::{Config, CURRENT_SCHEMA_VERSION};

/// Migrate a parsed JSON value up to the current `Config`.
/// - version 0 / missing → legacy pre-versioned file: stamp v1, defaults fill any missing fields.
/// - version 1 → deserialize directly.
/// - version > current → error (a newer build wrote it; do not silently downgrade/lose data).
pub fn migrate(mut value: serde_json::Value) -> Result<Config, String> {
    let version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;

    match version {
        // v0 (legacy/unversioned) and v1 (bare `mode`) both upgrade to the current schema by
        // stamping the version; `#[serde(default)]` fills the fields added since (profiles, active).
        0 | 1 => {
            value["schema_version"] = serde_json::json!(CURRENT_SCHEMA_VERSION);
            serde_json::from_value(value).map_err(|e| format!("migrate v{version}→v{CURRENT_SCHEMA_VERSION}: {e}"))
        }
        2 => serde_json::from_value(value).map_err(|e| format!("parse v2: {e}")),
        v => Err(format!(
            "config schema v{v} is newer than supported v{CURRENT_SCHEMA_VERSION}; refusing to load"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::modes::WakeMode;

    #[test]
    fn migrates_legacy_unversioned() {
        let v = serde_json::json!({ "mode": "KeepRunning" });
        let cfg = migrate(v).unwrap();
        assert_eq!(cfg.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(cfg.mode, WakeMode::KeepRunning);
    }

    #[test]
    fn migrates_v1_to_v2() {
        // A v1 file has a bare `mode` and no profiles.
        let v = serde_json::json!({ "schema_version": 1, "mode": "KeepPresenting" });
        let cfg = migrate(v).unwrap();
        assert_eq!(cfg.schema_version, 2);
        assert_eq!(cfg.mode, WakeMode::KeepPresenting);
        assert!(cfg.profiles.is_empty());
    }

    #[test]
    fn loads_current_version() {
        let v = serde_json::json!({ "schema_version": 2, "mode": "KeepPresenting", "profiles": [], "active_profile": "" });
        assert_eq!(migrate(v).unwrap().mode, WakeMode::KeepPresenting);
    }

    #[test]
    fn rejects_newer_version() {
        let v = serde_json::json!({ "schema_version": 999, "mode": "Off" });
        assert!(migrate(v).is_err());
    }
}
