//! Versioned migration chain (FEATURES D8). A parse failure here is surfaced, never silently
//! reset.

use serde_json::{json, Value};

use super::model::{Config, CURRENT_SCHEMA_VERSION};
use crate::core::input_engine::InputSettings;

/// Migrate a parsed JSON value up to the current `Config`.
/// - v0 (unversioned), v1 (bare `mode`), v2 (profiles + the input switch) → v3.
/// - v3 → deserialize directly.
/// - newer than current → error (a newer build wrote it; do not silently downgrade or lose data).
pub fn migrate(mut value: Value) -> Result<Config, String> {
    // Indexing anything else panics, and a release build aborts on panic (Review Focus 1).
    if !value.is_object() {
        return Err("config is not a JSON object".into());
    }
    let version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;

    match version {
        0..=2 => {
            // v3 stopped saving whether it was running (`mode`) and replaced the input switch
            // with Start. If that switch was never on, nobody chose the input settings, they
            // are v2's invisible defaults, so v3's visible ones replace them. Someone who did
            // turn it on keeps what they set (spec 005 FR-014).
            let input_was_on = value
                .get("input_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !input_was_on {
                value["input"] =
                    serde_json::to_value(InputSettings::default()).map_err(|e| e.to_string())?;
            }
            if let Some(obj) = value.as_object_mut() {
                obj.remove("mode");
                obj.remove("input_enabled");
            }
            value["schema_version"] = json!(CURRENT_SCHEMA_VERSION);
            serde_json::from_value(value)
                .map_err(|e| format!("migrate v{version}→v{CURRENT_SCHEMA_VERSION}: {e}"))
        }
        3 => serde_json::from_value(value).map_err(|e| format!("parse v3: {e}")),
        v => Err(format!(
            "config schema v{v} is newer than supported v{CURRENT_SCHEMA_VERSION}; refusing to load"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::motion::Motion;
    use crate::core::running::RunSettings;

    fn v2_input(enabled: bool) -> Value {
        json!({
            "schema_version": 2,
            "mode": "KeepRunning",
            "input_enabled": enabled,
            "input": {
                "interval_secs": 200, "idle_threshold_secs": 30, "key": 0,
                "motion": "Line", "distance_px": 25, "vary_pct": 10
            },
            "profiles": [{ "id": "a", "name": "A", "rules": [] }],
            "active_profile": "a",
            "auto_update": false
        })
    }

    #[test]
    fn migrates_legacy_unversioned() {
        let cfg = migrate(json!({ "mode": "KeepRunning" })).unwrap();
        assert_eq!(cfg.schema_version, 3);
        assert_eq!(cfg.input, InputSettings::default());
        assert_eq!(cfg.run, RunSettings::default());
    }

    #[test]
    fn migrates_v1() {
        let cfg = migrate(json!({ "schema_version": 1, "mode": "KeepPresenting" })).unwrap();
        assert_eq!(cfg.schema_version, 3);
        assert!(cfg.profiles.is_empty());
    }

    /// SC-007: input never switched on → nobody chose those settings; take v3's visible defaults.
    #[test]
    fn v2_with_input_never_enabled_gets_the_v3_defaults() {
        let cfg = migrate(v2_input(false)).unwrap();
        assert_eq!(cfg.input, InputSettings::default());
        assert!(cfg.run.move_mouse);
    }

    /// SC-007: someone who turned it on keeps what they set.
    #[test]
    fn v2_with_input_enabled_keeps_its_settings() {
        let cfg = migrate(v2_input(true)).unwrap();
        assert_eq!(
            cfg.input,
            InputSettings {
                interval_secs: 200,
                key: 0,
                motion: Motion::Line,
                distance_px: 25,
                vary_pct: 10
            }
        );
    }

    #[test]
    fn v2_keeps_profiles_and_the_update_switch() {
        let cfg = migrate(v2_input(false)).unwrap();
        assert_eq!(cfg.active().map(|p| p.name.as_str()), Some("A"));
        assert!(!cfg.auto_update);
    }

    #[test]
    fn loads_current_version() {
        let v = json!({
            "schema_version": 3,
            "run": { "move_mouse": false, "keep_screen_on": false, "start_on_launch": true }
        });
        let cfg = migrate(v).unwrap();
        assert_eq!(
            cfg.run,
            RunSettings {
                move_mouse: false,
                keep_screen_on: false,
                start_on_launch: true
            }
        );
    }

    #[test]
    fn a_partial_run_block_fills_in_defaults() {
        let cfg =
            migrate(json!({ "schema_version": 3, "run": { "start_on_launch": true } })).unwrap();
        assert!(cfg.run.start_on_launch && cfg.run.move_mouse && cfg.run.keep_screen_on);
    }

    #[test]
    fn rejects_newer_version() {
        assert!(migrate(json!({ "schema_version": 999 })).is_err());
    }

    /// Review Focus 1: indexing a non-object panics, and the release build aborts on panic.
    #[test]
    fn rejects_a_config_that_is_not_an_object() {
        for v in [json!([1]), json!(5), json!("x"), Value::Null] {
            assert!(migrate(v.clone()).is_err(), "{v} was accepted");
        }
    }
}
