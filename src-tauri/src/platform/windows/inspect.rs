//! `PowerInspector`: the machine-wide `EXECUTION_STATE` aggregate, for "Why is my PC awake?" (E1).
//!
//! Deliberately **not** `GetPowerRequestList`. That information class names the holder of each
//! request, which is what we actually want — and it returns `STATUS_INVALID_PARAMETER` to an
//! unelevated caller on Windows 11 26200 at every buffer size, while sibling info levels succeed
//! with identical argument shapes. Since this app never asks for admin, the aggregate is what we
//! can read, and it is enough to say whether something holds a request and whether it is us.
//! Measurements: `specs/003-settings-ui/research.md` R1.

use windows::Win32::System::Power::{CallNtPowerInformation, SystemExecutionState};

use crate::platform::PowerInspector;

#[derive(Default)]
pub struct WindowsPowerInspector;

impl WindowsPowerInspector {
    pub fn new() -> Self {
        Self
    }
}

impl PowerInspector for WindowsPowerInspector {
    fn execution_state(&self) -> Option<u32> {
        let mut es: u32 = 0;
        let status = unsafe {
            CallNtPowerInformation(
                SystemExecutionState,
                None,
                0,
                Some(&mut es as *mut u32 as *mut core::ffi::c_void),
                size_of::<u32>() as u32,
            )
        };
        // Any non-success is reported as "cannot read" rather than as "nothing is held" —
        // the panel must never turn a refusal into a confident negative (FEATURES E1).
        status.is_ok().then_some(es)
    }
}
