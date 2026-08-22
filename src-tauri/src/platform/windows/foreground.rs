//! `ForegroundMonitor`: the foreground app's exe name + the one-call notification state
//! (WINDOWS-API B7/B10). `SHQueryUserNotificationState` covers presentation, fullscreen, game,
//! and locked/screensaver in a single cheap call — no window-rect heuristics.

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP, QUNS_BUSY, QUNS_NOT_PRESENT,
    QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use crate::core::rule::NotifState;
use crate::platform::ForegroundMonitor;

#[derive(Default)]
pub struct WindowsForegroundMonitor;

impl WindowsForegroundMonitor {
    pub fn new() -> Self {
        Self
    }
}

impl ForegroundMonitor for WindowsForegroundMonitor {
    fn foreground_app(&self) -> Option<String> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            // PROCESS_QUERY_LIMITED_INFORMATION works against elevated/protected processes where
            // the full right is denied (WINDOWS-API gotcha 11).
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; MAX_PATH as usize];
            let mut len = buf.len() as u32;
            let res = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(handle);
            if res.is_err() {
                return None;
            }
            let full = String::from_utf16_lossy(&buf[..len as usize]);
            full.rsplit(['\\', '/']).next().map(str::to_string)
        }
    }

    fn notification_state(&self) -> NotifState {
        let s = match unsafe { SHQueryUserNotificationState() } {
            Ok(s) => s,
            Err(_) => return NotifState::Normal,
        };
        if s == QUNS_NOT_PRESENT {
            NotifState::NotPresent
        } else if s == QUNS_BUSY {
            NotifState::Busy
        } else if s == QUNS_RUNNING_D3D_FULL_SCREEN {
            NotifState::Game
        } else if s == QUNS_PRESENTATION_MODE {
            NotifState::Presentation
        } else if s == QUNS_QUIET_TIME {
            NotifState::QuietTime
        } else if s == QUNS_APP {
            NotifState::App
        } else {
            debug_assert!(s == QUNS_ACCEPTS_NOTIFICATIONS);
            NotifState::Normal
        }
    }
}
