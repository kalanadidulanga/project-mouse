//! Thin, **synchronous** Tauri commands (keeping tokio dormant, TAURI-V2 §0.2). Each is a wrapper
//! over `core`; the React UI holds only a projection of state, never the state itself.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::config::model::Appearance;
use crate::core::autopilot::{self, PauseReason, RunFor, Timetable};
use crate::core::awake::{self, AwakeReport};
use crate::core::engine::Engine;
use crate::core::input_engine::{InputEngine, InputSettings};
use crate::core::modes::WakeMode;
use crate::core::rule::Profile;
use crate::core::running::{self, RunSettings, StatusKind};
use crate::platform::PowerInspector;
use crate::sampler::Sampler;
use crate::{logging, platform};

type SharedEngine = Arc<Mutex<Engine>>;
type SharedInput = Arc<Mutex<InputEngine>>;
type SharedRun = Arc<Mutex<RunSettings>>;
type SharedTimetable = Arc<Mutex<Timetable>>;
type SharedAppearance = Arc<Mutex<Appearance>>;
type SharedAutopilot = Arc<Mutex<crate::core::autopilot::Autopilot>>;
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
    /// Why moves are paused, while running and paused (spec 006 FR-014).
    pub pause: Option<PauseReason>,
    /// When a Run-for ends (epoch seconds), while running.
    pub stops_at: Option<u64>,
    /// Seconds since the last input of any kind (spec 006 FR-018).
    pub idle_secs: u32,
    /// While stopped: the first listed app that is keeping the PC awake (FR-026).
    pub holding_app: Option<String>,
}

#[tauri::command]
pub fn get_status(
    engine: State<'_, SharedEngine>,
    input: State<'_, SharedInput>,
    run: State<'_, SharedRun>,
    timetable: State<'_, SharedTimetable>,
    auto: State<'_, SharedAutopilot>,
    sampler: State<'_, Arc<Sampler>>,
) -> Status {
    let settings = *run.lock().unwrap();
    let snap = sampler.last();
    let (on, effective, holding_app) = {
        let e = engine.lock().unwrap();
        let on = running::is_running(&e);
        let holding = (!on && e.mode() != WakeMode::Off)
            .then(|| crate::core::apps::first_running(e.profile(), &snap.running_processes))
            .flatten();
        (on, e.mode(), holding)
    };
    let pause = if on {
        autopilot::pause_reason(&settings, &timetable.lock().unwrap().blackouts, &snap)
    } else {
        None
    };
    let (blocked, next_move_in_secs, idle_ms) = {
        let ie = input.lock().unwrap();
        (
            ie.enabled() && ie.blocked,
            ie.next_move_in_secs(),
            ie.system_idle_ms,
        )
    };
    let stops_at = if on {
        auto.lock().unwrap().deadline()
    } else {
        None
    };
    Status {
        kind: running::status_kind(on, settings.move_mouse, blocked, effective, pause.is_some()),
        running: on,
        next_move_in_secs,
        keep_screen_on: settings.keep_screen_on,
        pause,
        stops_at,
        idle_secs: idle_ms / 1000,
        holding_app,
    }
}

/// Start, optionally for a while (spec 006 FR-007). JS sends `{ runFor }`.
#[tauri::command]
pub fn start(app: AppHandle, run_for: Option<RunFor>) {
    crate::start_for(&app, run_for.unwrap_or(RunFor::Forever));
}

#[tauri::command]
pub fn set_run_for(app: AppHandle, run_for: RunFor) {
    crate::set_run_for(&app, run_for);
}

#[tauri::command]
pub fn get_appearance(appearance: State<'_, SharedAppearance>) -> Appearance {
    *appearance.lock().unwrap()
}

#[tauri::command]
pub fn set_appearance(app: AppHandle, appearance: Appearance) {
    crate::set_appearance(&app, appearance);
}

#[tauri::command]
pub fn get_timetable(timetable: State<'_, SharedTimetable>) -> Timetable {
    timetable.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_timetable(app: AppHandle, timetable: Timetable) -> Timetable {
    crate::set_timetable(&app, timetable)
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

#[tauri::command]
pub fn get_apps(engine: State<'_, SharedEngine>) -> Vec<String> {
    crate::core::apps::apps(engine.lock().unwrap().profile())
}

#[tauri::command]
pub fn set_apps(app: AppHandle, names: Vec<String>) -> Vec<String> {
    crate::set_apps(&app, names)
}

/// Running executables, for the "add an app" suggestions: sorted, one entry per name.
#[tauri::command]
pub fn list_running_apps(sampler: State<'_, Arc<Sampler>>) -> Vec<String> {
    let mut names: Vec<String> = sampler
        .last()
        .running_processes
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".exe"))
        .collect();
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}
