//! Versioned migration chain (FEATURES D8). A parse failure here is surfaced, never silently
//! reset.

use serde_json::{json, Value};

use super::model::{Config, CURRENT_SCHEMA_VERSION};
use crate::core::apps;
use crate::core::input_engine::InputSettings;
use crate::core::modes::WakeMode;
use crate::core::rule::Profile;

/// Migrate a parsed JSON value up to the current `Config`.
/// - v0 (unversioned), v1 (bare `mode`), v2 (profiles + the input switch) → v3 → v4.
/// - v3 → v4: the timer rule is dropped and process rules fold into the apps list.
/// - v4 → deserialize directly.
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
        0..=3 => {
            if version <= 2 {
                // v3 stopped saving whether it was running (`mode`) and replaced the input switch
                // with Start. If that switch was never on, nobody chose the input settings, they
                // are v2's invisible defaults, so v3's visible ones replace them. Someone who did
                // turn it on keeps what they set (spec 005 FR-014).
                let input_was_on = value
                    .get("input_enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !input_was_on {
                    value["input"] = serde_json::to_value(InputSettings::default())
                        .map_err(|e| e.to_string())?;
                }
                if let Some(obj) = value.as_object_mut() {
                    obj.remove("mode");
                    obj.remove("input_enabled");
                }
            }
            value["schema_version"] = json!(CURRENT_SCHEMA_VERSION);
            let mut cfg: Config = serde_json::from_value(value)
                .map_err(|e| format!("migrate v{version}→v{CURRENT_SCHEMA_VERSION}: {e}"))?;
            upgrade_rules(&mut cfg.profiles);
            Ok(cfg)
        }
        4 => serde_json::from_value(value).map_err(|e| format!("parse v4: {e}")),
        v => Err(format!(
            "config schema v{v} is newer than supported v{CURRENT_SCHEMA_VERSION}; refusing to load"
        )),
    }
}

/// v3 → v4 (spec 006 FR-019, FR-031): drop the Advanced timer rule, and fold every enabled
/// KeepRunning process-only rule into the plain apps list. Every other rule stays exactly as it was
/// (constitution VI); About ▸ Troubleshooting lists them.
fn upgrade_rules(profiles: &mut [Profile]) {
    for p in profiles {
        p.rules.retain(|r| r.id != "timer");
        let mut names = apps::apps(p);
        let mut folded: Vec<String> = Vec::new();
        for r in &p.rules {
            if let Some(n) =
                apps::process_only(r).filter(|_| r.enabled && r.mode == WakeMode::KeepRunning)
            {
                names.extend(n.iter().cloned());
                folded.push(r.id.clone());
            }
        }
        if !folded.is_empty() {
            p.rules.retain(|r| !folded.contains(&r.id));
            apps::set_apps(p, names);
        }
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
        assert_eq!(cfg.schema_version, 4);
        assert_eq!(cfg.input, InputSettings::default());
        assert_eq!(cfg.run, RunSettings::default());
    }

    #[test]
    fn migrates_v1() {
        let cfg = migrate(json!({ "schema_version": 1, "mode": "KeepPresenting" })).unwrap();
        assert_eq!(cfg.schema_version, 4);
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
                motion: Motion::RightAndLeft,
                distance_px: 25,
                ..InputSettings::default()
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
            "schema_version": 4,
            "run": { "move_mouse": false, "keep_screen_on": false, "start_on_launch": true }
        });
        let cfg = migrate(v).unwrap();
        assert_eq!(
            cfg.run,
            RunSettings {
                move_mouse: false,
                keep_screen_on: false,
                start_on_launch: true,
                ..Default::default()
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

    use crate::core::apps::{apps, APPS_RULE_ID};

    fn v3_with_rules() -> Value {
        json!({
            "schema_version": 3,
            "input": { "interval_secs": 200, "key": 0, "motion": "Line", "distance_px": 10, "vary_pct": 25 },
            "profiles": [{ "id": "default", "name": "Default", "rules": [
                { "id": "timer", "name": "Keep running for 30 minutes", "enabled": true,
                  "conditions": [{ "ExpiryAt": 1 }], "mode": "KeepRunning" },
                { "id": "b", "name": "while build", "enabled": true,
                  "conditions": [{ "ProcessRunning": ["msbuild.exe"] }], "mode": "KeepRunning" },
                { "id": "c", "name": "off one", "enabled": false,
                  "conditions": [{ "ProcessRunning": ["off.exe"] }], "mode": "KeepRunning" },
                { "id": "d", "name": "on AC", "enabled": true,
                  "conditions": ["OnACPower"], "mode": "KeepPresenting" },
                { "id": "e", "name": "screen on for video", "enabled": true,
                  "conditions": [{ "ProcessRunning": ["vlc.exe"] }], "mode": "KeepPresenting" }
            ]}],
            "active_profile": "default"
        })
    }

    /// SC-006 and SC-011.
    #[test]
    fn v3_to_v4_drops_the_timer_and_folds_process_rules_into_the_apps_list() {
        let cfg = migrate(v3_with_rules()).unwrap();
        assert_eq!(cfg.schema_version, 4);
        let p = cfg.active().unwrap();
        let ids: Vec<&str> = p.rules.iter().map(|r| r.id.as_str()).collect();
        assert!(!ids.contains(&"timer"), "{ids:?}");
        assert!(!ids.contains(&"b"), "the process rule was folded");
        assert!(ids.contains(&"c"), "a disabled rule is kept as it is");
        assert!(ids.contains(&"d"), "a non-process rule is kept as it is");
        assert!(ids.contains(&"e"), "only KeepRunning process rules fold");
        assert!(ids.contains(&APPS_RULE_ID));
        assert_eq!(apps(p), vec!["msbuild.exe".to_string()]);
    }

    #[test]
    fn v3_input_keeps_its_values_and_gains_the_new_defaults() {
        let cfg = migrate(v3_with_rules()).unwrap();
        assert_eq!(cfg.input.interval_secs, 200);
        assert_eq!(cfg.input.motion, crate::core::motion::Motion::RightAndLeft);
        assert!(!cfg.input.interval_random && cfg.input.abortable);
        assert_eq!(cfg.appearance, crate::config::model::Appearance::default());
        assert!(cfg.timetable.schedules.is_empty() && cfg.timetable.blackouts.is_empty());
    }

    #[test]
    fn a_v4_timetable_and_appearance_round_trip() {
        let v = json!({
            "schema_version": 4,
            "timetable": {
                "schedules": [{ "days": [true,true,true,true,true,false,false], "at": 540, "action": "Start" }],
                "blackouts": [{ "days": [true,true,true,true,true,true,true], "from": 750, "to": 810, "enabled": false }]
            },
            "appearance": { "always_on_top": true }
        });
        let cfg = migrate(v).unwrap();
        assert_eq!(cfg.timetable.schedules[0].at, 540);
        assert!(cfg.timetable.schedules[0].enabled, "enabled defaults to on");
        assert!(!cfg.timetable.blackouts[0].enabled);
        assert!(
            cfg.appearance.always_on_top
                && cfg.appearance.taskbar_dot
                && cfg.appearance.notifications
        );
    }

    #[test]
    fn a_v4_file_is_not_re_folded() {
        let mut v = v3_with_rules();
        v["schema_version"] = json!(4);
        let cfg = migrate(v).unwrap();
        let ids: Vec<&str> = cfg
            .active()
            .unwrap()
            .rules
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert!(
            ids.contains(&"timer") && ids.contains(&"b"),
            "v4 files are taken as they are"
        );
    }
}
