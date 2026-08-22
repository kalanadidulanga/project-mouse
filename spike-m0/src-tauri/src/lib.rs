// project-mouse — M0 SPIKE. THROWAWAY CODE (ROADMAP.md M0). Do not build on this.
//
// Proves the two undocumented assumptions the whole stack rests on:
//   1. A destroyed Tauri webview actually returns memory to the OS (ARCHITECTURE §3).
//   2. PowerRequestExecutionRequired holds a machine awake and is auditable (WINDOWS-API gotcha 0).
//
// The tray menu drives every test by hand; a background "cycle x20" runs the
// open/destroy drift test unattended. Memory is written to %TEMP%\m0-app.log,
// and measure.ps1 samples the process externally as the authoritative record.
// (release build sets windows_subsystem="windows" → no console, hence file logging.)

use std::ffi::c_void;
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Power::{
    PowerClearRequest, PowerCreateRequest, PowerSetRequest, SetThreadExecutionState, ES_CONTINUOUS,
    ES_SYSTEM_REQUIRED, PowerRequestDisplayRequired, PowerRequestExecutionRequired,
    PowerRequestSystemRequired,
};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::SystemServices::POWER_REQUEST_CONTEXT_VERSION;
use windows::Win32::System::Threading::{
    GetCurrentProcess, POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};

#[derive(Default)]
struct AppState {
    power_handle: Option<isize>,   // PowerCreateRequest HANDLE as isize (HANDLE isn't Send)
    power_reason: Option<Vec<u16>>, // kept alive for the handle's lifetime
    stes_active: bool,
    close_hide: bool, // how the current window's X button behaves
}

fn working_set() -> (u64, u64) {
    unsafe {
        let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let _ = GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
            cb,
        );
        (c.WorkingSetSize as u64, c.PrivateUsage as u64)
    }
}

fn log(msg: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("m0-app.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{msg}");
    }
}

fn log_ws(tag: &str) {
    let (ws, pv) = working_set();
    log(&format!(
        "{tag}: WorkingSet={:.1} MB  PrivateUsage={:.1} MB",
        ws as f64 / 1e6,
        pv as f64 / 1e6
    ));
}

fn open_window(app: &AppHandle, hide_mode: bool) {
    app.state::<Mutex<AppState>>().lock().unwrap().close_hide = hide_mode;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let cfg = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned();
    match cfg {
        Some(cfg) => match tauri::WebviewWindowBuilder::from_config(app, &cfg) {
            Ok(b) => match b.build() {
                Ok(_) => log_ws(&format!("after open ({})", mode(hide_mode))),
                Err(e) => log(&format!("window build error: {e}")),
            },
            Err(e) => log(&format!("from_config error: {e}")),
        },
        None => log("no 'main' window config found"),
    }
}

fn mode(hide: bool) -> &'static str {
    if hide {
        "hide-mode"
    } else {
        "destroy-mode"
    }
}

fn hold_power(app: &AppHandle, execution: bool, display: bool) {
    let st = app.state::<Mutex<AppState>>();
    let mut s = st.lock().unwrap();
    if s.power_handle.is_some() {
        log("power already held — release first");
        return;
    }
    let reason: Vec<u16> = "project-mouse M0 spike — Keep running"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let ctx = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: PWSTR(reason.as_ptr() as *mut u16),
            },
        };
        match PowerCreateRequest(&ctx) {
            Ok(h) => {
                let _ = PowerSetRequest(h, PowerRequestSystemRequired);
                if execution {
                    let _ = PowerSetRequest(h, PowerRequestExecutionRequired);
                }
                if display {
                    let _ = PowerSetRequest(h, PowerRequestDisplayRequired);
                }
                s.power_handle = Some(h.0 as isize);
                s.power_reason = Some(reason);
                log(&format!(
                    "power HELD: system{}{}",
                    if execution { "+execution" } else { "" },
                    if display { "+display" } else { "" }
                ));
            }
            Err(e) => log(&format!("PowerCreateRequest error: {e}")),
        }
    }
}

fn hold_stes(app: &AppHandle) {
    // Comparison path only — the thing every competitor uses. Thread-affine, so it
    // rides the long-lived main thread (menu events run there). See WINDOWS-API gotcha 2.
    unsafe {
        let _ = SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
    }
    app.state::<Mutex<AppState>>().lock().unwrap().stes_active = true;
    log("STES set: ES_CONTINUOUS|ES_SYSTEM_REQUIRED (comparison only — no execution-required equivalent exists)");
}

fn release_power(app: &AppHandle) {
    let st = app.state::<Mutex<AppState>>();
    let mut s = st.lock().unwrap();
    if let Some(hi) = s.power_handle.take() {
        unsafe {
            let h = HANDLE(hi as *mut c_void);
            let _ = PowerClearRequest(h, PowerRequestDisplayRequired);
            let _ = PowerClearRequest(h, PowerRequestExecutionRequired);
            let _ = PowerClearRequest(h, PowerRequestSystemRequired);
            let _ = CloseHandle(h);
        }
        s.power_reason = None;
        log("power RELEASED (PowerRequest cleared + handle closed)");
    }
    if s.stes_active {
        unsafe {
            let _ = SetThreadExecutionState(ES_CONTINUOUS);
        }
        s.stes_active = false;
        log("STES cleared (ES_CONTINUOUS)");
    }
}

fn cycle_blocking(app: &AppHandle, n: usize) {
    log(&format!("=== CYCLE START: {n}x open/destroy ==="));
    for i in 0..n {
        let a1 = app.clone();
        let _ = app.run_on_main_thread(move || open_window(&a1, false));
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let a2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = a2.get_webview_window("main") {
                let _ = w.destroy();
            }
        });
        std::thread::sleep(std::time::Duration::from_millis(1500));
        log_ws(&format!("cycle {}/{} (after destroy+settle)", i + 1, n));
    }
    log("=== CYCLE DONE ===");
}

fn run_cycle(app: &AppHandle, n: usize) {
    let app = app.clone();
    std::thread::spawn(move || cycle_blocking(&app, n));
}

// Unattended full run of every automatable M0 test, so measurement needs no tray clicks.
// The external sampler (measure.ps1) reads the process tree; this thread only tags the timeline.
fn auto_run(app: AppHandle) {
    let sleep = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));

    sleep(6000);
    log_ws("T1 tray-only (settled 6s)  <-- expect <= 10 MB");

    let a = app.clone();
    let _ = app.run_on_main_thread(move || open_window(&a, false));
    sleep(3000);
    log_ws("single window open (settled)  <-- expect ~120-140 MB");

    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = a.get_webview_window("main") {
            let _ = w.destroy();
        }
    });
    sleep(8000);
    log_ws("T2 after destroy (settled 8s)  <-- expect back within 2 MB of T1");

    cycle_blocking(&app, 20);
    sleep(3000);
    log_ws("T3 after 20x cycle (settled)  <-- expect no upward drift vs T2");

    let a = app.clone();
    let _ = app.run_on_main_thread(move || open_window(&a, true));
    sleep(3000);
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = a.get_webview_window("main") {
            let _ = w.hide();
        }
    });
    sleep(8000);
    log_ws("T4 after hide (settled 8s)  <-- comparison: expect NO drop");

    hold_power(&app, true, false);
    log("T7 power held (system+execution) — now run: powercfg /requests");
    log("=== AUTO RUN COMPLETE (power still held; quit or taskkill for T8) ===");
}

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        "open_destroy" => open_window(app, false),
        "open_hide" => open_window(app, true),
        "power_run" => hold_power(app, true, false),   // Keep running: system + execution
        "power_present" => hold_power(app, true, true), // Keep presenting: + display
        "power_stes" => hold_stes(app),
        "release" => release_power(app),
        "cycle20" => run_cycle(app, 20),
        "print_ws" => log_ws("manual print"),
        "quit" => {
            release_power(app);
            app.exit(0);
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(AppState::default()))
        .setup(|app| {
            let icon = app.default_window_icon().cloned().unwrap();
            let h = app.handle().clone();
            let sep = || PredefinedMenuItem::separator(&h);
            let item = |id: &str, txt: &str| MenuItem::with_id(&h, id, txt, true, None::<&str>);

            let menu = Menu::with_items(
                &h,
                &[
                    &item("open_destroy", "Open window (destroy on close)")?,
                    &item("open_hide", "Open window (hide on close — comparison)")?,
                    &sep()?,
                    &item("power_run", "Hold power: Keep running (system+execution)")?,
                    &item("power_present", "Hold power: Keep presenting (+display)")?,
                    &item("power_stes", "Hold power: STES (comparison only)")?,
                    &item("release", "Release power")?,
                    &sep()?,
                    &item("cycle20", "Run 20x open/destroy cycle")?,
                    &item("print_ws", "Print working set to log")?,
                    &sep()?,
                    &item("quit", "Quit")?,
                ],
            )?;

            let _tray = TrayIconBuilder::with_id("m0")
                .icon(icon)
                .tooltip("project-mouse M0 spike")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| handle_menu(app, event.id.as_ref()))
                .build(&h)?;

            log("=== M0 spike started (tray only, no window) ===");
            log_ws("startup");

            if std::env::var("M0_AUTO").is_ok() {
                let auto = h.clone();
                std::thread::spawn(move || auto_run(auto));
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let hide = window
                    .app_handle()
                    .state::<Mutex<AppState>>()
                    .lock()
                    .unwrap()
                    .close_hide;
                if hide {
                    api.prevent_close();
                    let _ = window.hide();
                    log_ws("after hide (window still resident)");
                } else {
                    let _ = window.destroy();
                    log_ws("after destroy (immediate — WebView2 procs still settling)");
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error building tauri app")
        .run(|_app, event| {
            // Stay alive with zero windows — TAURI-V2 §2. Only code:None (last window closed)
            // is prevented; Quit uses app.exit(0) → Some(0) → allowed through.
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}
