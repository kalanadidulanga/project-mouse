//! Thin, **synchronous** Tauri commands (keeping tokio dormant — TAURI-V2 §0.2). Each is a wrapper
//! over `core`; the React UI holds only a projection of state, never the state itself.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::core::engine::Engine;
use crate::core::input_engine::InputEngine;
use crate::core::modes::WakeMode;
use crate::core::rule::{Profile, Rule};
use crate::{logging, platform};

type SharedEngine = Arc<Mutex<Engine>>;
type SharedInput = Arc<Mutex<InputEngine>>;

fn mode_str(m: WakeMode) -> &'static str {
    match m {
        WakeMode::Off => "off",
        WakeMode::KeepRunning => "keep_running",
        WakeMode::KeepPresenting => "keep_presenting",
    }
}

fn mode_from_str(s: &str) -> WakeMode {
    match s {
        "keep_running" => WakeMode::KeepRunning,
        "keep_presenting" => WakeMode::KeepPresenting,
        _ => WakeMode::Off,
    }
}

#[derive(Serialize)]
pub struct StateView {
    pub effective_mode: String,
    pub manual_mode: String,
    pub paused: bool,
    pub profile: String,
}

#[derive(Serialize)]
pub struct Diagnostics {
    pub effective_mode: String,
    pub system_sleep_blocked: bool,
    pub display_blocked: bool,
    pub lock_blocked: bool,
    pub reason: String,
    pub memory_mb: f64,
    pub system_idle_secs: u64,
    pub human_idle_secs: u64,
    pub input_enabled: bool,
    pub input_blocked: bool,
}

#[tauri::command]
pub fn get_state(engine: State<'_, SharedEngine>) -> StateView {
    let e = engine.lock().unwrap();
    StateView {
        effective_mode: mode_str(e.mode()).into(),
        manual_mode: mode_str(e.manual()).into(),
        paused: e.paused(),
        profile: e.profile_name().into(),
    }
}

#[tauri::command]
pub fn set_mode(app: AppHandle, mode: String) {
    crate::set_manual(&app, mode_from_str(&mode));
}

#[tauri::command]
pub fn pause_all(engine: State<'_, SharedEngine>) {
    engine.lock().unwrap().set_paused(true);
}

#[tauri::command]
pub fn resume_all(engine: State<'_, SharedEngine>) {
    engine.lock().unwrap().set_paused(false);
}

#[tauri::command]
pub fn get_diagnostics(
    engine: State<'_, SharedEngine>,
    input: State<'_, SharedInput>,
) -> Diagnostics {
    let m = engine.lock().unwrap().mode();
    let ie = input.lock().unwrap();
    let reason = match m {
        WakeMode::Off => "Not holding anything.".to_string(),
        WakeMode::KeepRunning => {
            "Keeping the system awake; the screen may still sleep.".to_string()
        }
        WakeMode::KeepPresenting => "Keeping the system awake and the display on.".to_string(),
    };
    Diagnostics {
        effective_mode: mode_str(m).into(),
        system_sleep_blocked: m != WakeMode::Off,
        display_blocked: m == WakeMode::KeepPresenting,
        lock_blocked: m == WakeMode::KeepPresenting,
        reason,
        memory_mb: platform::working_set_bytes() as f64 / 1_000_000.0,
        system_idle_secs: (ie.system_idle_ms / 1000) as u64,
        human_idle_secs: (ie.human_idle_ms / 1000) as u64,
        input_enabled: ie.enabled(),
        input_blocked: ie.enabled() && ie.blocked,
    }
}

#[tauri::command]
pub fn set_input_enabled(app: AppHandle, input: State<'_, SharedInput>, enabled: bool) {
    input.lock().unwrap().set_enabled(enabled);
    crate::persist_current(&app);
}

/// Import a Move Mouse `Settings.xml` → the active profile (power-only default). Returns the report.
#[tauri::command]
pub fn import_move_mouse(
    app: AppHandle,
    engine: State<'_, SharedEngine>,
    input: State<'_, SharedInput>,
    path: String,
) -> Result<Vec<String>, String> {
    let xml = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let imported = crate::config::import_movemouse::import(&xml)?;
    engine.lock().unwrap().set_profile(imported.profile);
    input.lock().unwrap().set_enabled(imported.input_enabled);
    crate::persist_current(&app);
    Ok(imported.report)
}

#[tauri::command]
pub fn get_logs(limit: usize) -> Vec<String> {
    logging::tail(limit.clamp(1, 500))
}

#[tauri::command]
pub fn get_rules(engine: State<'_, SharedEngine>) -> Profile {
    engine.lock().unwrap().profile().clone()
}

#[tauri::command]
pub fn upsert_rule(app: AppHandle, engine: State<'_, SharedEngine>, rule: Rule) {
    engine.lock().unwrap().upsert_rule(rule);
    crate::persist_current(&app);
}

#[tauri::command]
pub fn delete_rule(app: AppHandle, engine: State<'_, SharedEngine>, id: String) {
    engine.lock().unwrap().delete_rule(&id);
    crate::persist_current(&app);
}

#[tauri::command]
pub fn set_rule_enabled(
    app: AppHandle,
    engine: State<'_, SharedEngine>,
    id: String,
    enabled: bool,
) {
    engine.lock().unwrap().set_rule_enabled(&id, enabled);
    crate::persist_current(&app);
}
