//! Thin, **synchronous** Tauri commands (keeping tokio dormant — TAURI-V2 §0.2). Each is a wrapper
//! over `core`; the React UI holds only a projection of state, never the state itself.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::core::awake::{self, AwakeReport};
use crate::core::engine::Engine;
use crate::core::input_engine::{InputEngine, InputSettings};
use crate::core::modes::WakeMode;
use crate::core::profiles;
use crate::core::rule::{Condition, Profile, Rule};
use crate::platform::PowerInspector;
use crate::{logging, platform};

type SharedEngine = Arc<Mutex<Engine>>;
type SharedInput = Arc<Mutex<InputEngine>>;
type SharedProfiles = Arc<Mutex<Vec<Profile>>>;
type SharedInspector = Arc<dyn PowerInspector>;

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
    pub remote_session: bool,
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
    sampler: State<'_, Arc<crate::sampler::Sampler>>,
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
        remote_session: sampler.last().remote_session,
    }
}

/// "Why is my PC awake?" (E1). Never `Err` — a refused read comes back as `readable: false`, so
/// the panel can say so rather than render a confident "nothing is held".
#[tauri::command]
pub fn why_awake(
    engine: State<'_, SharedEngine>,
    inspector: State<'_, SharedInspector>,
) -> AwakeReport {
    let ours = engine.lock().unwrap().mode();
    awake::report(inspector.execution_state(), ours)
}

#[derive(Serialize)]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub rule_count: usize,
}

#[tauri::command]
pub fn list_profiles(
    engine: State<'_, SharedEngine>,
    stored: State<'_, SharedProfiles>,
) -> Vec<ProfileSummary> {
    let live = engine.lock().unwrap().profile().clone();
    let mut list = stored.lock().unwrap().clone();
    // The engine's copy is authoritative for the active profile — it may hold unsaved edits.
    profiles::upsert(&mut list, live.clone());
    list.into_iter()
        .map(|p| ProfileSummary {
            active: p.id == live.id,
            rule_count: p.rules.len(),
            id: p.id,
            name: p.name,
        })
        .collect()
}

#[tauri::command]
pub fn set_profile(
    app: AppHandle,
    engine: State<'_, SharedEngine>,
    stored: State<'_, SharedProfiles>,
    id: String,
) {
    {
        let mut e = engine.lock().unwrap();
        let mut list = stored.lock().unwrap();
        // Write the live profile back BEFORE loading the other one, or unsaved rule edits die
        // with the switch.
        profiles::upsert(&mut list, e.profile().clone());
        match profiles::find(&list, &id) {
            Some(p) => e.set_profile(p.clone()),
            None => {
                tracing::warn!(%id, "set_profile: no such profile");
                return;
            }
        }
    }
    crate::persist_current(&app);
    crate::rebuild_tray_menu(&app);
}

#[tauri::command]
pub fn create_profile(app: AppHandle, stored: State<'_, SharedProfiles>, name: String) -> String {
    let id = format!(
        "p{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let name = if name.trim().is_empty() {
        "New profile".to_string()
    } else {
        name
    };
    profiles::upsert(&mut stored.lock().unwrap(), Profile::new(&id, name));
    crate::persist_current(&app);
    crate::rebuild_tray_menu(&app);
    id
}

#[tauri::command]
pub fn delete_profile(
    app: AppHandle,
    engine: State<'_, SharedEngine>,
    stored: State<'_, SharedProfiles>,
    id: String,
) -> Result<(), String> {
    {
        let mut list = stored.lock().unwrap();
        if !profiles::delete(&mut list, &id) {
            return Err("that is the last profile — the app must always hold one".into());
        }
        // Deleting the active one means loading whatever is left.
        let mut e = engine.lock().unwrap();
        if e.profile().id == id {
            if let Some(p) = list.first().cloned() {
                e.set_profile(p);
            }
        }
    }
    crate::persist_current(&app);
    crate::rebuild_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn is_first_run() -> bool {
    crate::is_first_run()
}

/// The first-run answer → one working profile. Never touches `input_enabled` (SC-007).
#[tauri::command]
pub fn complete_first_run(
    app: AppHandle,
    engine: State<'_, SharedEngine>,
    stored: State<'_, SharedProfiles>,
    choice: String,
) {
    let mut p = match choice.as_str() {
        "long_job" => {
            let mut p = Profile::new("long-job", "Long job");
            p.rules.push(Rule {
                id: "long-job-process".into(),
                name: "Keep running while a process runs".into(),
                // Disabled and unnamed: the shape is there for the user to fill in, and it holds
                // nothing until they do.
                enabled: false,
                conditions: vec![Condition::ProcessRunning(Vec::new())],
                mode: WakeMode::KeepRunning,
            });
            p
        }
        "keep_screen" => {
            let mut p = Profile::new("keep-screen", "Keep a screen up");
            p.rules.push(Rule {
                id: "keep-screen-always".into(),
                name: "Keep presenting".into(),
                enabled: true, // they asked for exactly this
                conditions: Vec::new(),
                mode: WakeMode::KeepPresenting,
            });
            p
        }
        _ => Profile::new("default", "Default"),
    };
    p.rules.shrink_to_fit();
    {
        let mut list = stored.lock().unwrap();
        profiles::upsert(&mut list, p.clone());
        engine.lock().unwrap().set_profile(p);
    }
    crate::clear_first_run();
    crate::persist_current(&app);
    crate::rebuild_tray_menu(&app);
}

#[tauri::command]
pub fn set_input_enabled(app: AppHandle, input: State<'_, SharedInput>, enabled: bool) {
    input.lock().unwrap().set_enabled(enabled);
    crate::persist_current(&app);
}

#[tauri::command]
pub fn get_input_settings(input: State<'_, SharedInput>) -> InputSettings {
    input.lock().unwrap().settings()
}

#[tauri::command]
pub fn set_input_settings(
    app: AppHandle,
    input: State<'_, SharedInput>,
    settings: InputSettings,
) -> InputSettings {
    let mut ie = input.lock().unwrap();
    ie.set_settings(settings);
    let applied = ie.settings(); // clamped — the UI shows what actually took effect
    drop(ie);
    crate::persist_current(&app);
    applied
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

#[derive(Serialize)]
pub struct UpdateStatus {
    pub current: String,
    /// The version a check found, or `None`. Not a promise that one does not exist — only that
    /// no check has found one yet.
    pub available: Option<String>,
    pub auto_check: bool,
}

#[tauri::command]
pub fn get_update_status() -> UpdateStatus {
    UpdateStatus {
        current: env!("CARGO_PKG_VERSION").to_string(),
        available: crate::update_available(),
        auto_check: crate::auto_update_enabled(),
    }
}

#[tauri::command]
pub fn set_auto_update(app: AppHandle, enabled: bool) {
    crate::set_auto_update(&app, enabled);
}

/// Check now, without installing — the manual counterpart to the background check (UPDATES.md §6).
#[tauri::command]
pub fn check_for_update(app: AppHandle) {
    tauri::async_runtime::spawn(crate::check_and_install(app, true));
}

/// Download and install. Windows force-exits during install; `on_before_exit` releases the power
/// request first, so nothing is left holding the machine awake.
#[tauri::command]
pub fn install_update(app: AppHandle) {
    tauri::async_runtime::spawn(crate::check_and_install(app, false));
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
