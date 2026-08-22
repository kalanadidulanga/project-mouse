//! The portability boundary (CROSS-PLATFORM §2, constitution IV).
//! No `#[cfg(windows)]` anywhere in `core/`; it lives only under here.

pub mod mock;
#[cfg(windows)]
pub mod windows;

use std::fmt;
use std::sync::Arc;

use crate::core::rule::NotifState;

pub type Result<T> = std::result::Result<T, PlatformError>;

#[derive(Debug)]
pub struct PlatformError(pub String);

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for PlatformError {}

/// Low-level power inhibition. The reconciler in `power/` is the single owner that decides
/// *when* to call these; the guard only realises the OS request. Idempotency lives in the
/// reconciler, not here.
pub trait PowerGuard: Send + Sync {
    /// Hold system+execution required, and display required iff `display`, with `reason`
    /// visible in `powercfg /requests`. Replaces any request already held.
    fn set(&self, display: bool, reason: &str) -> Result<()>;
    /// Release everything held. Safe to call when nothing is held.
    fn clear(&self) -> Result<()>;
}

/// Names of currently-running processes (executable file names). The caller cadence-limits how
/// often it asks — enumeration is ~1-3 ms. FEATURES B1.
pub trait ProcessMonitor: Send + Sync {
    fn running_process_names(&self) -> Vec<String>;
}

/// Foreground app name + the single-call notification state (presentation/fullscreen/game/locked).
/// FEATURES B7, B10.
pub trait ForegroundMonitor: Send + Sync {
    fn foreground_app(&self) -> Option<String>;
    fn notification_state(&self) -> NotifState;
}

/// AC line status + battery percent. FEATURES B5.
pub trait PowerSource: Send + Sync {
    /// `(on_ac, battery_percent)` where percent is 0..=100 (100 when unknown/no battery).
    fn power_status(&self) -> (bool, u8);
}

/// Synthetic input (the opt-in engine, FEATURES Part C). A non-error return does NOT mean the input
/// landed — UIPI can discard it silently (gotcha 3); verify via the idle clock (C7).
pub trait InputInjector: Send + Sync {
    /// A virtual jiggle: net-zero visible cursor movement that still resets the idle timer (C1).
    fn virtual_jiggle(&self) -> Result<()>;
    /// Press and release a virtual key (default `VK_F15`), down+up in one call (C3, gotcha 8).
    fn key(&self, vk: u16) -> Result<()>;
}

// ponytail: autostart is handled by the cross-platform `tauri-plugin-autostart` (HKCU\Run on
// Windows) at the shell layer, so it needs no trait in this OS-abstraction boundary.

/// What the current OS can actually do — the UI asks rather than assumes (CROSS-PLATFORM §2).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // consumed by the capability-aware UI in M3
pub struct Capabilities {
    pub can_prevent_system_sleep: bool,
    pub can_prevent_display_sleep: bool,
    pub can_autostart: bool,
}

/// Everything the engine is allowed to know about the OS.
pub struct Platform {
    /// `Arc` so the panic hook can hold a clone and release the request on an abort.
    pub power: Arc<dyn PowerGuard>,
    /// `Arc` so the scheduler thread can sample without going through Tauri state.
    pub processes: Arc<dyn ProcessMonitor>,
    pub foreground: Arc<dyn ForegroundMonitor>,
    pub power_source: Arc<dyn PowerSource>,
    pub input: Arc<dyn InputInjector>,
    #[allow(dead_code)] // consumed by the capability-aware UI in M3
    pub caps: Capabilities,
}

/// Trim the process working set back to the OS (post-startup / post-teardown). No-op off Windows.
pub fn trim_working_set() {
    #[cfg(windows)]
    windows::trim_working_set();
}

/// Local (weekday 0=Mon..6=Sun, minutes-of-day 0..1440) for schedule evaluation. (0,0) off Windows.
pub fn local_time() -> (u8, u16) {
    #[cfg(windows)]
    {
        windows::local_time()
    }
    #[cfg(not(windows))]
    {
        (0, 0)
    }
}

/// Raw `GetLastInputInfo.dwTime` tick (u32) — for the self-injection filter. 0 off Windows.
pub fn last_input_tick() -> u32 {
    #[cfg(windows)]
    {
        windows::last_input_tick()
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Raw `GetTickCount` (u32) — same clock domain as `last_input_tick`. 0 off Windows.
pub fn tick_now() -> u32 {
    #[cfg(windows)]
    {
        windows::tick_now()
    }
    #[cfg(not(windows))]
    {
        0
    }
}

// (system idle in ms is derived from last_input_tick/tick_now by the input engine's IdleTracker.)

/// This process's working-set bytes (for the memory readout). 0 off Windows.
pub fn working_set_bytes() -> u64 {
    #[cfg(windows)]
    {
        windows::working_set_bytes()
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// The real platform for this build. Windows in M1; other OSes get no-op guards so the crate
/// compiles cross-platform (the boundary is proven even while only Windows is implemented).
pub fn real() -> Platform {
    #[cfg(windows)]
    {
        Platform {
            power: Arc::new(windows::power::WindowsPowerGuard::new()),
            processes: Arc::new(windows::process::WindowsProcessMonitor::new()),
            foreground: Arc::new(windows::foreground::WindowsForegroundMonitor::new()),
            power_source: Arc::new(windows::load::WindowsPowerSource::new()),
            input: Arc::new(windows::input::WindowsInputInjector::new()),
            caps: Capabilities {
                can_prevent_system_sleep: true,
                can_prevent_display_sleep: true,
                can_autostart: true,
            },
        }
    }
    #[cfg(not(windows))]
    {
        Platform {
            power: Arc::new(mock::NoopPowerGuard::default()),
            processes: Arc::new(mock::NoopProcessMonitor::default()),
            foreground: Arc::new(mock::NoopForegroundMonitor::default()),
            power_source: Arc::new(mock::NoopPowerSource::default()),
            input: Arc::new(mock::NoopInjector::default()),
            caps: Capabilities {
                can_prevent_system_sleep: false,
                can_prevent_display_sleep: false,
                can_autostart: false,
            },
        }
    }
}
