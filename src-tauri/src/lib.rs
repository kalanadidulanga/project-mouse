//! project-mouse — M1 wake engine. Tray-only, no window, power inhibition by default.

mod config;
mod core;
mod logging;
mod platform;
mod power;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::config::{model::Config, store};
use crate::core::engine::Engine;
use crate::core::modes::WakeMode;
use crate::platform::PowerGuard;

/// Panic-hook backstop: `panic = "abort"` skips `Drop`, and `app.exit`/`process::exit` don't run
/// destructors either — so the request is released explicitly on every path (FR-005).
static PANIC_GUARD: OnceLock<Arc<dyn PowerGuard>> = OnceLock::new();

/// Where to persist mode, and whether saving is allowed. Saving is disabled when the on-disk
/// config was corrupt, so we never overwrite a file the user may want to recover (FEATURES D8).
struct Persist {
    path: PathBuf,
    enabled: bool,
}

fn log_dir() -> PathBuf {
    // Portable spirit: logs beside the exe; fall back to temp if that dir isn't writable.
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

fn set_mode(app: &tauri::AppHandle, mode: WakeMode) {
    app.state::<Mutex<Engine>>().lock().unwrap().set_mode(mode);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _log_guard = logging::init(&log_dir()); // keep alive for the whole process

    let platform = platform::real();
    let power = platform.power.clone();
    let _ = PANIC_GUARD.set(power.clone());
    std::panic::set_hook(Box::new(|info| {
        if let Some(g) = PANIC_GUARD.get() {
            let _ = g.clear();
        }
        eprintln!("panic: {info}");
    }));

    // Restore the last persisted mode. A corrupt config surfaces (logged) and disables saving
    // so the file is preserved — never silently reset (FEATURES D8).
    let cfg_path = store::resolve_config_path();
    let (initial_mode, save_enabled) = match store::load(&cfg_path) {
        Ok(c) => (c.mode, true),
        Err(e) => {
            tracing::error!(
                "config load failed ({e}); starting Off and NOT overwriting the file so it can be recovered"
            );
            (WakeMode::Off, false)
        }
    };

    let mut engine = Engine::new(power);
    engine.set_mode(initial_mode); // restore last state (reconciles power at startup)

    tauri::Builder::default()
        // single-instance MUST be registered first (TAURI-V2 §6).
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {
            tracing::info!("second instance attempted; ignoring (single instance)");
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(Mutex::new(engine))
        .manage(Mutex::new(Persist { path: cfg_path, enabled: save_enabled }))
        .setup(|app| {
            let icon = app.default_window_icon().cloned().unwrap();
            let h = app.handle().clone();
            let item = |id: &str, txt: &str| MenuItem::with_id(&h, id, txt, true, None::<&str>);

            let restored = app.state::<Mutex<Engine>>().lock().unwrap().mode();
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
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "off" => set_mode(app, WakeMode::Off),
                    "keep_running" => set_mode(app, WakeMode::KeepRunning),
                    "keep_presenting" => set_mode(app, WakeMode::KeepPresenting),
                    "autostart" => toggle_autostart(app, &ai),
                    "quit" => {
                        app.state::<Mutex<Engine>>().lock().unwrap().release();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            tracing::info!("project-mouse M1 started (tray only, no window)");
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building tauri app")
        .run(|app, event| match event {
            // Stay alive with zero windows (TAURI-V2 §2). Quit uses app.exit(0) → Some(0) → allowed.
            tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            // Final backstop: release the power request on any real exit.
            tauri::RunEvent::Exit => {
                if let Some(state) = app.try_state::<Mutex<Engine>>() {
                    state.lock().unwrap().release();
                }
            }
            _ => {}
        });
}
