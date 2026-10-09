//! Thin, **synchronous** Tauri commands (keeping tokio dormant, TAURI-V2 §0.2). Each is a wrapper
//! over `core`; the React UI holds only a projection of state, never the state itself.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::core::awake::{self, AwakeReport};
use crate::core::engine::Engine;
use crate::core::input_engine::{InputEngine, InputSettings};
use crate::core::modes::WakeMode;
use crate::core::profiles;
use crate::core::rule::{Profile, Rule};
use crate::core::running::{self, RunSettings, StatusKind};
use crate::platform::PowerInspector;
use crate::{logging, platform};

type SharedEngine = Arc<Mutex<Engine>>;
type SharedInput = Arc<Mutex<InputEngine>>;
type SharedProfiles = Arc<Mutex<Vec<Profile>>>;
type SharedRun = Arc<Mutex<RunSettings>>;
type SharedInspector = Arc<dyn PowerInspector>;

fn mode_str(m: WakeMode) -> &'static str {
    match m {
        WakeMode::Off => "off",
        WakeMode::KeepRunning => "keep_running",
        WakeMode::KeepPresenting => "keep_presenting",
    }
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

/// What Home shows (spec 005 FR-008/FR-010).
#[derive(Serialize)]
pub struct Status {
    pub kind: StatusKind,
    pub running: bool,
    /// Seconds to the next move; `None` when stopped or when Move the mouse is off.
    pub next_move_in_secs: Option<u32>,
    pub keep_screen_on: bool,
}

#[tauri::command]
pub fn get_status(
    engine: State<'_, SharedEngine>,
    input: State<'_, SharedInput>,
    run: State<'_, SharedRun>,
) -> Status {
    let settings = *run.lock().unwrap();
    let (on, effective) = {
        let e = engine.lock().unwrap();
        (running::is_running(&e), e.mode())
    };
    let (blocked, next_move_in_secs) = {
        let ie = input.lock().unwrap();
        (ie.enabled() && ie.blocked, ie.next_move_in_secs())
    };
    Status {
        kind: running::status_kind(on, settings.move_mouse, blocked, effective),
        running: on,
        next_move_in_secs,
        keep_screen_on: settings.keep_screen_on,
    }
}

#[tauri::command]
pub fn start(app: AppHandle) {
    crate::set_running(&app, true);
}

#[tauri::command]
pub fn stop(app: AppHandle) {
    crate::set_running(&app, false);
}

/// Test (spec 005 FR-007). On its own thread: a path takes a few hundred milliseconds, and a
/// synchronous command runs on the main thread, which would freeze the window that long.
#[tauri::command]
pub fn test_move(input: State<'_, SharedInput>) {
    let input = input.inner().clone();
    std::thread::spawn(move || input.lock().unwrap().move_now(platform::tick_now()));
}

#[tauri::command]
pub fn get_run_settings(run: State<'_, SharedRun>) -> RunSettings {
    *run.lock().unwrap()
}

#[tauri::command]
pub fn set_run_settings(app: AppHandle, settings: RunSettings) {
    crate::set_run_settings(&app, settings);
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    crate::set_autostart(&app, enabled)
}

/// "Why is my PC awake?" (E1). Never `Err`; a refused read comes back as `readable: false`, so
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
    // The engine's copy is authoritative for the active profile, it may hold unsaved edits.
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
    crate::tray::sync(&app);
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
    crate::tray::sync(&app);
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
            return Err("that is the last profile; the app must always hold one".into());
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
    crate::tray::sync(&app);
    Ok(())
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
    let applied = ie.settings(); // clamped, the UI shows what actually took effect
    drop(ie);
    crate::persist_current(&app);
    applied
}

/// Import a Move Mouse `Settings.xml` → the active profile and movement. Returns the report.
#[tauri::command]
pub fn import_move_mouse(
    app: AppHandle,
    engine: State<'_, SharedEngine>,
    input: State<'_, SharedInput>,
    run: State<'_, SharedRun>,
    path: String,
) -> Result<Vec<String>, String> {
    use crate::config::import_movemouse as mm;
    // Empty → look where Move Mouse keeps it.
    let path = match mm::clean_path(&path) {
        Some(p) => p,
        None => mm::find_settings_xml().ok_or(
            "Move Mouse's Settings.xml is not in either usual place. Paste its full path.",
        )?,
    };
    let xml = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let imported = crate::config::import_movemouse::import(&xml)?;
    engine.lock().unwrap().set_profile(imported.profile);
    if let Some(s) = imported.input {
        input.lock().unwrap().set_settings(s);
        let settings = RunSettings {
            move_mouse: true,
            ..*run.lock().unwrap()
        };
        crate::set_run_settings(&app, settings); // re-applies and saves
    } else {
        crate::persist_current(&app);
    }
    Ok(imported.report)
}

#[derive(Serialize)]
pub struct UpdateStatus {
    pub current: String,
    /// The version a check found, or `None`. Not a promise that one does not exist, only that
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

/// Check now, without installing, the manual counterpart to the background check (UPDATES.md §6).
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
