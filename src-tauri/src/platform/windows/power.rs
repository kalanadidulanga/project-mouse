//! `PowerGuard` via `PowerCreateRequest`/`PowerSetRequest` — the handle-scoped, auditable,
//! Modern-Standby-correct path (WINDOWS-API gotcha 0 & 2). Validated in the M0 spike.
//!
//! Not `SetThreadExecutionState`: it has no `PowerRequestExecutionRequired` equivalent and its
//! thread affinity is undocumented folklore.

use std::ffi::c_void;
use std::sync::Mutex;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Power::{
    PowerClearRequest, PowerCreateRequest, PowerSetRequest, PowerRequestDisplayRequired,
    PowerRequestExecutionRequired, PowerRequestSystemRequired,
};
use windows::Win32::System::SystemServices::POWER_REQUEST_CONTEXT_VERSION;
use windows::Win32::System::Threading::{
    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};

use crate::platform::{PlatformError, PowerGuard, Result};

/// Holds at most one power request at a time. The reason `Vec<u16>` is kept alive for the
/// handle's lifetime (the request references the string).
#[derive(Default)]
pub struct WindowsPowerGuard {
    held: Mutex<Option<Held>>,
}

struct Held {
    handle: isize,       // HANDLE as isize — HANDLE isn't Send
    _reason: Vec<u16>,   // kept alive while the request is held
}

// SAFETY: the raw HANDLE is only ever touched under the Mutex, on whatever thread holds the lock.
unsafe impl Send for WindowsPowerGuard {}
unsafe impl Sync for WindowsPowerGuard {}

impl WindowsPowerGuard {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PowerGuard for WindowsPowerGuard {
    fn set(&self, display: bool, reason: &str) -> Result<()> {
        // Replace whatever is held (mode may have changed).
        self.clear()?;

        let wide: Vec<u16> = reason.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let ctx = REASON_CONTEXT {
                Version: POWER_REQUEST_CONTEXT_VERSION,
                Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
                Reason: REASON_CONTEXT_0 {
                    SimpleReasonString: PWSTR(wide.as_ptr() as *mut u16),
                },
            };
            let handle =
                PowerCreateRequest(&ctx).map_err(|e| PlatformError(format!("PowerCreateRequest: {e}")))?;
            PowerSetRequest(handle, PowerRequestSystemRequired)
                .map_err(|e| PlatformError(format!("SystemRequired: {e}")))?;
            PowerSetRequest(handle, PowerRequestExecutionRequired)
                .map_err(|e| PlatformError(format!("ExecutionRequired: {e}")))?;
            if display {
                PowerSetRequest(handle, PowerRequestDisplayRequired)
                    .map_err(|e| PlatformError(format!("DisplayRequired: {e}")))?;
            }
            *self.held.lock().unwrap() = Some(Held { handle: handle.0 as isize, _reason: wide });
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        if let Some(held) = self.held.lock().unwrap().take() {
            unsafe {
                let h = HANDLE(held.handle as *mut c_void);
                // Clearing a type that was never set is harmless — ignore per-type errors.
                let _ = PowerClearRequest(h, PowerRequestDisplayRequired);
                let _ = PowerClearRequest(h, PowerRequestExecutionRequired);
                let _ = PowerClearRequest(h, PowerRequestSystemRequired);
                CloseHandle(h).map_err(|e| PlatformError(format!("CloseHandle: {e}")))?;
            }
        }
        Ok(())
    }
}

impl Drop for WindowsPowerGuard {
    fn drop(&mut self) {
        // Backstop against a leaked request if the guard is ever dropped without clear().
        let _ = self.clear();
    }
}
