//! Thin, **synchronous** Tauri commands (keeping tokio dormant — TAURI-V2 §0.2). Each is a wrapper
//! over `core`; the React UI holds only a projection of state, never the state itself.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::core::engine::Engine;
use crate::core::modes::WakeMode;
use crate::{logging, platform};

type SharedEngine = Arc<Mutex<Engine>>;

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
pub fn get_diagnostics(engine: State<'_, SharedEngine>) -> Diagnostics {
    let m = engine.lock().unwrap().mode();
    let reason = match m {
        WakeMode::Off => "Not holding anything.".to_string(),
        WakeMode::KeepRunning => "Keeping the system awake; the screen may still sleep.".to_string(),
        WakeMode::KeepPresenting => "Keeping the system awake and the display on.".to_string(),
    };
    Diagnostics {
        effective_mode: mode_str(m).into(),
        system_sleep_blocked: m != WakeMode::Off,
        display_blocked: m == WakeMode::KeepPresenting,
        lock_blocked: m == WakeMode::KeepPresenting,
        reason,
        memory_mb: platform::working_set_bytes() as f64 / 1_000_000.0,
        system_idle_secs: platform::system_idle_ms() / 1000,
    }
}

#[tauri::command]
pub fn get_logs(limit: usize) -> Vec<String> {
    logging::tail(limit.clamp(1, 500))
}
