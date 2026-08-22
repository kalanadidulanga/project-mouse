//! project-mouse — wake engine. Tray-only, power inhibition by default. M2 adds the rule engine:
//! a scheduler thread ticks ~1 s, samples state, evaluates the active profile, and reconciles.

mod config;
mod core;
mod ipc;
mod logging;
mod platform;
mod power;
mod sampler;
mod timing;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::config::{model::Config, store};
use crate::core::engine::Engine;
use crate::core::modes::WakeMode;
use crate::core::rule::{Condition, Profile, Rule};
use crate::platform::PowerGuard;
use crate::sampler::Sampler;

/// Panic-hook backstop: `panic = "abort"` skips `Drop`, and `app.exit`/`process::exit` don't run
/// destructors either — so the request is released explicitly on every path (FR-005).
static PANIC_GUARD: OnceLock<Arc<dyn PowerGuard>> = OnceLock::new();

/// Set before releasing power on exit, so the scheduler thread stops ticking (and cannot
/// re-acquire a request after release).
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

type SharedEngine = Arc<Mutex<Engine>>;

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

/// Create the settings window on demand (reusing the declared `create:false` config), or focus it
/// if it already exists. Destroyed — not hidden — on close (ARCHITECTURE §3).
fn open_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_focus();
        return;
    }
    match app.config().app.windows.iter().find(|w| w.label == "main").cloned() {
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
    let persist = app.state::<Mutex<Persist>>();
    let p = persist.lock().unwrap();
    if p.enabled {
        if let Err(e) = store::save_atomic(&p.path, &Config::with_mode(mode)) {
            tracing::error!("config save failed: {e}");
        }
    }
}

fn toggle_autostart(app: &tauri::AppHandle, item: &CheckMenuItem<Wry>) {
    let mgr = app.autolaunch();
    let now = mgr.is_enabled().unwrap_or(false);
    match if now { mgr.disable() } else { mgr.enable() } {
        Ok(()) => {
            let _ = item.set_checked(!now);
            tracing::info!(enabled = !now, "autostart toggled");
        }
        Err(e) => tracing::error!("autostart toggle failed: {e}"),
    }
}

fn release_all(app: &tauri::AppHandle) {
    SHUTDOWN.store(true, Ordering::SeqCst);
    if let Some(engine) = app.try_state::<SharedEngine>() {
        engine.lock().unwrap().release();
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

    // Restore last manual mode; a corrupt config disables saving so it is preserved (FEATURES D8).
    let cfg_path = store::resolve_config_path();
    let (initial_mode, save_enabled) = match store::load(&cfg_path) {
        Ok(c) => (c.mode, true),
        Err(e) => {
            tracing::error!("config load failed ({e}); starting Off and preserving the file");
            (WakeMode::Off, false)
        }
    };

    let mut engine = Engine::new(power);
    engine.set_manual(initial_mode);
    if let Some(profile) = profile_from_args() {
        tracing::info!("loaded CLI profile from --while-process");
        engine.set_profile(profile);
    }
    let engine: SharedEngine = Arc::new(Mutex::new(engine));
    let sampler = Arc::new(Sampler::new(
        platform.processes.clone(),
        platform.foreground.clone(),
        platform.power_source.clone(),
    ));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {
            tracing::info!("second instance attempted; ignoring (single instance)");
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
        .manage(engine.clone())
        .manage(Mutex::new(Persist { path: cfg_path, enabled: save_enabled }))
        .invoke_handler(tauri::generate_handler![
            ipc::get_state,
            ipc::set_mode,
            ipc::pause_all,
            ipc::resume_all,
            ipc::get_diagnostics,
            ipc::get_logs,
        ])
        .on_window_event(|window, event| {
            // Destroy the webview on close — never hide (ARCHITECTURE §3). This returns the
            // ~130 MB of WebView2 processes; the app stays alive via prevent_exit.
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let _ = window.destroy();
            }
        })
        .setup(move |app| {
            let icon = app.default_window_icon().cloned().unwrap();
            let h = app.handle().clone();
            let item = |id: &str, txt: &str| MenuItem::with_id(&h, id, txt, true, None::<&str>);

            let restored = engine.lock().unwrap().manual();
            let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
            let autostart_item = CheckMenuItem::with_id(
                &h,
                "autostart",
                "Start with Windows",
                true,
                autostart_on,
                None::<&str>,
            )?;

            let menu = Menu::with_items(
                &h,
                &[
                    &item("off", "Off")?,
                    &item("keep_running", "Keep running")?,
                    &item("keep_presenting", "Keep presenting")?,
                    &PredefinedMenuItem::separator(&h)?,
                    &autostart_item,
                    &PredefinedMenuItem::separator(&h)?,
                    &item("quit", "Quit")?,
                ],
            )?;

            let ai = autostart_item.clone();
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
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "off" => set_manual(app, WakeMode::Off),
                    "keep_running" => set_manual(app, WakeMode::KeepRunning),
                    "keep_presenting" => set_manual(app, WakeMode::KeepPresenting),
                    "autostart" => toggle_autostart(app, &ai),
                    "quit" => {
                        release_all(app);
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Scheduler thread: owns nothing but a clone; ticks, evaluates, reconciles.
            let sched_engine = engine.clone();
            let sched_sampler = sampler.clone();
            let sched_app = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_mode: Option<WakeMode> = None;
                timing::ticker::run_loop(1000, 200, move || {
                    if SHUTDOWN.load(Ordering::SeqCst) {
                        return false;
                    }
                    let snap = sched_sampler.snapshot();
                    let mode = {
                        let mut e = sched_engine.lock().unwrap();
                        e.tick(&snap);
                        e.mode()
                    };
                    if last_mode != Some(mode) {
                        last_mode = Some(mode);
                        if let Some(tray) = sched_app.tray_by_id("main") {
                            let _ = tray.set_tooltip(Some(tooltip_for(mode)));
                        }
                        // Notify the UI only when a window is actually alive (ARCHITECTURE §8).
                        if sched_app.get_webview_window("main").is_some() {
                            let _ = sched_app.emit("state:changed", ());
                        }
                    }
                    true
                });
            });

            tracing::info!("project-mouse M2 started (tray only, scheduler running)");
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(3));
                platform::trim_working_set();
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building tauri app")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            tauri::RunEvent::Exit => release_all(app),
            _ => {}
        });
}
