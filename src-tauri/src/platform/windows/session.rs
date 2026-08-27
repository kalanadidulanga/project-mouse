//! `SessionMonitor`: is this a remote session (RDP / Citrix / Horizon)? FEATURES B6.
//!
//! `SM_REMOTESESSION` is documented, unprivileged, and a single call, so it is polled in the
//! existing ~1 s sampler tick. The `WM_WTSSESSION_CHANGE` notification the original task named
//! would need `WTSRegisterSessionNotification` and a window to receive messages — and this app
//! has no window at rest, which is the whole memory design. See research.md R2.

use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_REMOTESESSION};

use crate::platform::SessionMonitor;

#[derive(Default)]
pub struct WindowsSessionMonitor;

impl WindowsSessionMonitor {
    pub fn new() -> Self {
        Self
    }
}

impl SessionMonitor for WindowsSessionMonitor {
    fn is_remote_session(&self) -> bool {
        unsafe { GetSystemMetrics(SM_REMOTESESSION) != 0 }
    }
}
