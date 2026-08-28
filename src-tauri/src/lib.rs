//! project-mouse — wake engine. Tray-only, power inhibition by default. M2 adds the rule engine:
//! a scheduler thread ticks ~1 s, samples state, evaluates the active profile, and reconciles.

mod config;
mod core;
mod ipc;
mod logging;
mod platform;
mod power;
mod sampler;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_updater::UpdaterExt;

use crate::config::{model::Config, store};
use crate::core::engine::Engine;
use crate::core::evaluator::soonest_expiry_secs;
use crate::core::input_engine::{InputEngine, InputSettings};
use crate::core::modes::WakeMode;
use crate::core::profiles;
use crate::core::rule::{Condition, Profile, Rule};
use crate::platform::PowerGuard;
use crate::sampler::Sampler;

/// Panic-hook backstop: `panic = "abort"` skips `Drop`, and `app.exit`/`process::exit` don't run
/// destructors either — so the request is released explicitly on every path (FR-005).
static PANIC_GUARD: OnceLock<Arc<dyn PowerGuard>> = OnceLock::new();

/// Set before releasing power on exit, so the scheduler thread stops ticking (and cannot
/// re-acquire a request after release).
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// The version a background check found, if any. Held here rather than written straight to the
/// tray tooltip: the scheduler rewrites that tooltip whenever the mode changes, which used to
/// wipe the only notice the user ever got.
static UPDATE_AVAILABLE: Mutex<Option<String>> = Mutex::new(None);

/// Whether background checks run. Mirrors `Config::auto_update`.
static AUTO_UPDATE: AtomicBool = AtomicBool::new(true);

pub(crate) fn update_available() -> Option<String> {
    UPDATE_AVAILABLE.lock().unwrap().clone()
}

pub(crate) fn auto_update_enabled() -> bool {
    AUTO_UPDATE.load(Ordering::SeqCst)
}

pub(crate) fn set_auto_update(app: &tauri::AppHandle, on: bool) {
    AUTO_UPDATE.store(on, Ordering::SeqCst);
    persist_current(app);
}

type SharedEngine = Arc<Mutex<Engine>>;
type SharedInput = Arc<Mutex<InputEngine>>;
/// Every profile on disk. The engine holds one of them; this is the set (research R3).
type SharedProfiles = Arc<Mutex<Vec<Profile>>>;

/// Latched at startup: no config file existed, so the UI opens on the first-run question
/// (spec FR-008). Cleared by the answer, never re-read from disk.
static FIRST_RUN: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_first_run() -> bool {
    FIRST_RUN.load(Ordering::SeqCst)
}

pub(crate) fn clear_first_run() {
    FIRST_RUN.store(false, Ordering::SeqCst);
}

/// Where to persist mode, and whether saving is allowed. Saving is disabled when the on-disk
/// config was corrupt, so we never overwrite a file the user may want to recover (FEATURES D8).
struct Persist {
    path: PathBuf,
    enabled: bool,
}

fn log_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("logs")))
        .unwrap_or_else(std::env::temp_dir)
}

fn tooltip_for(mode: WakeMode) -> &'static str {
    match mode {
        WakeMode::Off => "project-mouse — not holding anything",
        WakeMode::KeepRunning => "project-mouse — keeping awake (screen may sleep)",
        WakeMode::KeepPresenting => "project-mouse — keeping the display on",
    }
}

/// `--while-process msbuild.exe [--while-process X ...]` → a profile with one process-bound rule
/// (SC-001). This is how M2 is exercised before the rule-builder UI (M3).
fn profile_from_args() -> Option<Profile> {
    let mut names = Vec::new();
    let mut args = std::env::args();
    while let Some(a) = args.next() {
        if a == "--while-process" {
            if let Some(name) = args.next() {
                names.push(name);
            }
        }
    }
    if names.is_empty() {
        return None;
    }
    let mut p = Profile::new("cli", "CLI");
    p.rules.push(Rule {
        id: "cli-while-process".into(),
        name: format!("Keep running while {}", names.join(", ")),
        enabled: true,
        conditions: vec![Condition::ProcessRunning(names)],
        mode: WakeMode::KeepRunning,
    });
    Some(p)
}

fn mode_from_cli(s: &str) -> Option<WakeMode> {
    match s.to_ascii_lowercase().as_str() {
        "off" => Some(WakeMode::Off),
        "running" | "keep_running" => Some(WakeMode::KeepRunning),
        "presenting" | "keep_presenting" => Some(WakeMode::KeepPresenting),
        _ => None,
    }
}

/// `--keep running|presenting|off` from argv — the startup initial mode (D10).
fn cli_keep_mode() -> Option<WakeMode> {
    let mut args = std::env::args();
    while let Some(a) = args.next() {
        if a == "--keep" {
            return args.next().and_then(|v| mode_from_cli(&v));
        }
    }
    None
}

/// Apply flags a *second* invocation forwarded to the running instance (single-instance), so
/// `project-mouse --keep presenting` / `--release` / `--show` controls the running app (D10).
fn apply_forwarded(app: &tauri::AppHandle, argv: &[String]) {
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--show" => open_window(app),
            "--release" => set_manual(app, WakeMode::Off),
            "--keep" => {
                if let Some(m) = it.next().and_then(|v| mode_from_cli(v)) {
                    set_manual(app, m);
                }
            }
            _ => {}
        }
    }
}

/// Create the settings window on demand (reusing the declared `create:false` config), or focus it
/// if it already exists. Destroyed — not hidden — on close (ARCHITECTURE §3).
fn open_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_focus();
        return;
    }
    match app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
    {
        Some(cfg) => match tauri::WebviewWindowBuilder::from_config(app, &cfg) {
            Ok(b) => {
                let _ = b.build();
            }
            Err(e) => tracing::error!("window build error: {e}"),
        },
        None => tracing::error!("no 'main' window config"),
    }
}

pub(crate) fn set_manual(app: &tauri::AppHandle, mode: WakeMode) {
    app.state::<SharedEngine>().lock().unwrap().set_manual(mode);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tooltip_for(mode)));
    }
    persist_current(app);
}

/// Persist the whole current config — manual mode + the active profile's rules — atomically.
/// Disabled when the on-disk config was corrupt, so we never overwrite a recoverable file.
pub(crate) fn persist_current(app: &tauri::AppHandle) {
    let engine = app.state::<SharedEngine>();
    let (mode, profile) = {
        let e = engine.lock().unwrap();
        (e.manual(), e.profile().clone())
    };
    let input = app.state::<SharedInput>();
    let (input_enabled, input_settings) = {
        let ie = input.lock().unwrap();
        (ie.enabled(), ie.settings())
    };
    let p = app.state::<Mutex<Persist>>();
    let p = p.lock().unwrap();
    if !p.enabled {
        return;
    }
    // Merge the live profile into the stored collection. Writing `vec![profile]` here — as this
    // did until 2026-08-28 — destroyed every other profile on the next save (research R3).
    let all = match app.try_state::<SharedProfiles>() {
        Some(state) => {
            let mut list = state.lock().unwrap();
            profiles::upsert(&mut list, profile.clone());
            list.clone()
        }
        None => vec![profile.clone()],
    };
    let cfg = Config {
        mode,
        active_profile: profile.id.clone(),
        profiles: all,
        input_enabled,
        input: input_settings,
        auto_update: auto_update_enabled(),
        ..Config::default()
    };
    if let Err(e) = store::save_atomic(&p.path, &cfg) {
        tracing::error!("config save failed: {e}");
    }
}

fn toggle_autostart(app: &tauri::AppHandle) {
    let mgr = app.autolaunch();
    let now = mgr.is_enabled().unwrap_or(false);
    match if now { mgr.disable() } else { mgr.enable() } {
        Ok(()) => {
            tracing::info!(enabled = !now, "autostart toggled");
            rebuild_tray_menu(app);
        }
        Err(e) => tracing::error!("autostart toggle failed: {e}"),
    }
}

/// The whole tray menu, built from current state. Rebuilt rather than mutated: a profile list and
/// a checkmark that must both stay truthful are cheaper to regenerate than to patch in place.
fn build_tray_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, txt: &str| MenuItem::with_id(app, id, txt, true, None::<&str>);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start with Windows",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;

    let active_id = app
        .try_state::<SharedEngine>()
        .map(|e| e.lock().unwrap().profile().id.clone())
        .unwrap_or_default();
    let stored: Vec<Profile> = app
        .try_state::<SharedProfiles>()
        .map(|p| p.lock().unwrap().clone())
        .unwrap_or_default();
    let entries: Vec<CheckMenuItem<Wry>> = stored
        .iter()
        .map(|p| {
            CheckMenuItem::with_id(
                app,
                format!("profile:{}", p.id),
                &p.name,
                true,
                p.id == active_id,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = entries
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect();
    let profiles_menu = Submenu::with_items(app, "Profile", !refs.is_empty(), &refs)?;

    Menu::with_items(
        app,
        &[
            &item("off", "Off")?,
            &item("keep_running", "Keep running")?,
            &item("keep_presenting", "Keep presenting")?,
            &PredefinedMenuItem::separator(app)?,
            &profiles_menu,
            &PredefinedMenuItem::separator(app)?,
            &autostart,
            &item("check_update", "Check for updates…")?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", "Quit")?,
        ],
    )
}

/// Load `id` into the engine, writing the live profile back into the collection first so
/// unsaved rule edits survive the switch.
pub(crate) fn switch_profile(app: &tauri::AppHandle, id: &str) {
    let (Some(engine), Some(stored)) = (
        app.try_state::<SharedEngine>(),
        app.try_state::<SharedProfiles>(),
    ) else {
        return;
    };
    {
        let mut e = engine.lock().unwrap();
        let mut list = stored.lock().unwrap();
        profiles::upsert(&mut list, e.profile().clone());
        match profiles::find(&list, id) {
            Some(p) => e.set_profile(p.clone()),
            None => {
                tracing::warn!(%id, "switch_profile: no such profile");
                return;
            }
        }
    }
    persist_current(app);
    rebuild_tray_menu(app);
}

pub(crate) fn rebuild_tray_menu(app: &tauri::AppHandle) {
    match (app.tray_by_id("main"), build_tray_menu(app)) {
        (Some(tray), Ok(menu)) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                tracing::error!("tray menu update failed: {e}");
            }
        }
        (_, Err(e)) => tracing::error!("tray menu build failed: {e}"),
        _ => {}
    }
}

/// Tooltip text: the mode, plus how long a running timer has left (002 T020).
fn tooltip_text(mode: WakeMode, remaining: Option<u64>) -> String {
    let base = match remaining {
        Some(s) => format!("{} — {} left", tooltip_for(mode), fmt_remaining(s)),
        None => tooltip_for(mode).to_string(),
    };
    match update_available() {
        Some(v) => format!("{base}\nUpdate {v} available — tray → Check for updates"),
        None => base,
    }
}

fn fmt_remaining(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        _ => format!("{}h {}m", secs / 3600, (secs % 3600) / 60),
    }
}

fn release_all(app: &tauri::AppHandle) {
    SHUTDOWN.store(true, Ordering::SeqCst);
    if let Some(engine) = app.try_state::<SharedEngine>() {
        engine.lock().unwrap().release();
    }
}

/// Check for an update (UPDATES.md §4). `auto` = a background check that only *hints* (never
/// interrupts); a manual check downloads + installs. Windows force-exits during install, so
/// `on_before_exit` releases the power request first.
async fn check_and_install(app: tauri::AppHandle, auto: bool) {
    let updater = match app
        .updater_builder()
        .on_before_exit(|| {
            if let Some(g) = PANIC_GUARD.get() {
                let _ = g.clear();
            }
        })
        .build()
    {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("updater build failed: {e}");
            return;
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let v = update.version.clone();
            tracing::info!("update available: {v}");
            if auto {
                // Record it and let the tooltip composer pick it up; the UI reads the same flag.
                // A background check never installs — UPDATES.md §6, "never interrupt".
                *UPDATE_AVAILABLE.lock().unwrap() = Some(v.clone());
                if app.get_webview_window("main").is_some() {
                    let _ = app.emit("state:changed", ());
                }
                return;
            }
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(_) => tracing::info!("update {v} installed; restarting"),
                Err(e) => tracing::error!("update install failed: {e}"),
            }
        }
        Ok(None) => {
            *UPDATE_AVAILABLE.lock().unwrap() = None;
            if !auto {
                tracing::info!("already up to date");
            }
        }
        Err(e) => tracing::error!("update check failed: {e}"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _log_guard = logging::init(&log_dir());

    let platform = platform::real();
    let power = platform.power.clone();
    let _ = PANIC_GUARD.set(power.clone());
    std::panic::set_hook(Box::new(|info| {
        if let Some(g) = PANIC_GUARD.get() {
            let _ = g.clear();
        }
        eprintln!("panic: {info}");
    }));

    // Restore last manual mode + active profile; a corrupt config disables saving so it is
    // preserved (FEATURES D8).
    let cfg_path = store::resolve_config_path();
    FIRST_RUN.store(!cfg_path.exists(), Ordering::SeqCst);
    let (
        initial_mode,
        initial_profile,
        initial_input,
        initial_input_settings,
        stored,
        save_enabled,
    ) = match store::load(&cfg_path) {
        Ok(c) => (
            c.mode,
            c.active().cloned(),
            c.input_enabled,
            c.input,
            c.profiles.clone(),
            true,
        ),
        Err(e) => {
            tracing::error!("config load failed ({e}); starting Off and preserving the file");
            (
                WakeMode::Off,
                None,
                false,
                InputSettings::default(),
                Vec::new(),
                false,
            )
        }
    };

    let mut engine = Engine::new(power);
    // --keep on the command line overrides the persisted mode for this launch (D10).
    engine.set_manual(cli_keep_mode().unwrap_or(initial_mode));
    if let Some(p) = initial_profile {
        engine.set_profile(p);
    }
    if let Some(profile) = profile_from_args() {
        tracing::info!("CLI --while-process overrides the active profile for this session");
        engine.set_profile(profile);
    }
    let engine: SharedEngine = Arc::new(Mutex::new(engine));
    // The engine always holds a profile, so the collection is never empty.
    let mut stored = stored;
    profiles::upsert(&mut stored, engine.lock().unwrap().profile().clone());
    let stored_profiles: SharedProfiles = Arc::new(Mutex::new(stored));

    let mut input_engine = InputEngine::new(platform.input.clone(), platform::tick_now());
    input_engine.set_enabled(initial_input);
    input_engine.set_settings(initial_input_settings);
    let input_engine: SharedInput = Arc::new(Mutex::new(input_engine));

    let sampler = Arc::new(Sampler::new(
        platform.processes.clone(),
        platform.foreground.clone(),
        platform.power_source.clone(),
        platform.session.clone(),
    ));
    let inspector = platform.inspector.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            tracing::info!("second instance forwarded args: {argv:?}");
            apply_forwarded(app, &argv);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        // Global hotkey (D3): Ctrl+Alt+K toggles the wake lock with no window.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["ctrl+alt+k"])
                .expect("valid shortcut")
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let cur = app.state::<SharedEngine>().lock().unwrap().manual();
                        let next = if cur == WakeMode::Off {
                            WakeMode::KeepRunning
                        } else {
                            WakeMode::Off
                        };
                        set_manual(app, next);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(engine.clone())
        .manage(input_engine.clone())
        .manage(stored_profiles.clone())
        .manage(sampler.clone())
        .manage(inspector)
        .manage(Mutex::new(Persist {
            path: cfg_path,
            enabled: save_enabled,
        }))
        .invoke_handler(tauri::generate_handler![
            ipc::get_state,
            ipc::set_mode,
            ipc::pause_all,
            ipc::resume_all,
            ipc::get_diagnostics,
            ipc::get_logs,
            ipc::get_rules,
            ipc::upsert_rule,
            ipc::delete_rule,
            ipc::set_rule_enabled,
            ipc::set_input_enabled,
            ipc::get_input_settings,
            ipc::set_input_settings,
            ipc::why_awake,
            ipc::list_profiles,
            ipc::set_profile,
            ipc::create_profile,
            ipc::delete_profile,
            ipc::is_first_run,
            ipc::complete_first_run,
            ipc::get_update_status,
            ipc::set_auto_update,
            ipc::check_for_update,
            ipc::install_update,
            ipc::import_move_mouse,
        ])
        .on_window_event(|window, event| {
            // Destroy the webview on close — never hide (ARCHITECTURE §3). This returns the
            // ~130 MB of WebView2 processes; the app stays alive via prevent_exit.
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let _ = window.destroy();
                // Destroying the webview frees the *child processes*, but our own working set
                // keeps whatever it grew to — Windows does not hand pages back unprompted.
                // Measured: 27 MB open, still 28 MB thirty seconds after close without this.
                // Same trim the app does at startup, once teardown has settled.
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    platform::trim_working_set();
                });
            }
        })
        .setup(move |app| {
            let icon = app.default_window_icon().cloned().unwrap();
            let restored = engine.lock().unwrap().manual();
            let menu = build_tray_menu(app.handle())?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip(tooltip_for(restored))
                .menu(&menu)
                .show_menu_on_left_click(false) // left = open window, right = menu (UI-UX §1)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        open_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "off" => set_manual(app, WakeMode::Off),
                    "keep_running" => set_manual(app, WakeMode::KeepRunning),
                    "keep_presenting" => set_manual(app, WakeMode::KeepPresenting),
                    "autostart" => toggle_autostart(app),
                    "check_update" => {
                        tauri::async_runtime::spawn(check_and_install(app.clone(), false));
                    }
                    "quit" => {
                        release_all(app);
                        app.exit(0);
                    }
                    id => {
                        if let Some(pid) = id.strip_prefix("profile:") {
                            switch_profile(app, pid);
                        }
                    }
                })
                .build(app)?;

            // Scheduler thread: owns nothing but a clone; ticks, evaluates, reconciles.
            let sched_engine = engine.clone();
            let sched_input = input_engine.clone();
            let sched_sampler = sampler.clone();
            let sched_app = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_tip = String::new();
                platform::run_tick_loop(1000, 200, move || {
                    if SHUTDOWN.load(Ordering::SeqCst) {
                        return false;
                    }
                    // Phase 1: reconcile the power engine against desired state.
                    let snap = sched_sampler.snapshot();
                    let (mode, remaining) = {
                        let mut e = sched_engine.lock().unwrap();
                        e.tick(&snap);
                        (e.mode(), soonest_expiry_secs(e.profile(), &snap))
                    };
                    // Phase 2: the input engine (off unless the user enabled it).
                    let blocked = {
                        let mut ie = sched_input.lock().unwrap();
                        ie.tick(platform::last_input_tick(), platform::tick_now());
                        ie.enabled() && ie.blocked
                    };
                    // Recomputed every tick but pushed only when the *text* changes, so a
                    // per-second countdown does not churn the tray once the minute has settled.
                    let tip = if blocked {
                        "project-mouse - input blocked (an elevated window has focus)".to_string()
                    } else {
                        tooltip_text(mode, remaining)
                    };
                    if tip != last_tip {
                        last_tip = tip.clone();
                        if let Some(tray) = sched_app.tray_by_id("main") {
                            let _ = tray.set_tooltip(Some(&tip));
                        }
                        // Notify the UI only when a window is actually alive (ARCHITECTURE §8).
                        if sched_app.get_webview_window("main").is_some() {
                            let _ = sched_app.emit("state:changed", ());
                        }
                    }
                    true
                });
            });

            tracing::info!("project-mouse started (tray + scheduler running)");
            if std::env::args().any(|a| a == "--show") {
                open_window(app.handle());
            }
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(3));
                platform::trim_working_set();
            });

            // Auto update-check: 10 s after start, then every 6 h. Background checks only *hint*
            // (tooltip) — never interrupt; the tray 'Check for updates…' item installs (UPDATES.md §6).
            let up = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(10));
                loop {
                    if SHUTDOWN.load(Ordering::SeqCst) {
                        break;
                    }
                    if auto_update_enabled() {
                        tauri::async_runtime::spawn(check_and_install(up.clone(), true));
                    }
                    // 6 h with up to 30 min of jitter, so a popular release does not produce a
                    // synchronised thundering herd at the top of the hour (UPDATES.md §6). Seeded
                    // from the tick — no RNG state, and it varies per machine.
                    let jitter = (crate::platform::tick_now() % 1_800) as u64;
                    std::thread::sleep(std::time::Duration::from_secs(6 * 3600 + jitter));
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building tauri app")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => api.prevent_exit(),
            tauri::RunEvent::Exit => release_all(app),
            _ => {}
        });
}
