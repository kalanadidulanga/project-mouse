//! The portability boundary (CROSS-PLATFORM §2, constitution IV).
//! No `#[cfg(windows)]` anywhere in `core/`; it lives only under here.

pub mod mock;
#[cfg(windows)]
pub mod windows;

use std::fmt;
use std::sync::Arc;

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

/// Optional launch-at-login. FEATURES D4.
pub trait AutoStart: Send + Sync {
    fn is_enabled(&self) -> Result<bool>;
    fn set_enabled(&self, on: bool) -> Result<()>;
}

/// What the current OS can actually do — the UI asks rather than assumes (CROSS-PLATFORM §2).
#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub can_prevent_system_sleep: bool,
    pub can_prevent_display_sleep: bool,
    pub can_autostart: bool,
}

/// Everything the engine is allowed to know about the OS.
pub struct Platform {
    /// `Arc` so the panic hook can hold a clone and release the request on an abort.
    pub power: Arc<dyn PowerGuard>,
    pub autostart: Box<dyn AutoStart>,
    pub caps: Capabilities,
}

/// The real platform for this build. Windows in M1; other OSes get no-op guards so the crate
/// compiles cross-platform (the boundary is proven even while only Windows is implemented).
pub fn real() -> Platform {
    #[cfg(windows)]
    {
        Platform {
            power: Arc::new(windows::power::WindowsPowerGuard::new()),
            // ponytail: autostart is Phase 4 (FEATURES D4) — Noop until then, so caps says so.
            autostart: Box::new(mock::NoopAutoStart::default()),
            caps: Capabilities {
                can_prevent_system_sleep: true,
                can_prevent_display_sleep: true,
                can_autostart: false,
            },
        }
    }
    #[cfg(not(windows))]
    {
        Platform {
            power: Arc::new(mock::NoopPowerGuard::default()),
            autostart: Box::new(mock::NoopAutoStart::default()),
            caps: Capabilities {
                can_prevent_system_sleep: false,
                can_prevent_display_sleep: false,
                can_autostart: false,
            },
        }
    }
}
