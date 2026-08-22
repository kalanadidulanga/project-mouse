//! Windows implementations of the platform traits.

pub mod foreground;
pub mod load;
pub mod power;
pub mod process;

/// Trim the working set back to the OS after startup (ARCHITECTURE §3). Cosmetic — pages fault
/// back in on next touch — but a genuinely idle tray never touches them again, and users judge
/// footprint by Task Manager. Never call on a hot path.
pub fn trim_working_set() {
    use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
    use windows::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let _ = EmptyWorkingSet(GetCurrentProcess());
    }
}

/// Local (weekday 0=Mon..6=Sun, minutes-of-day) via `GetLocalTime` — no timezone crate needed.
pub fn local_time() -> (u8, u16) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    // Windows wDayOfWeek is 0=Sunday..6=Saturday; convert to 0=Monday.
    let weekday = ((st.wDayOfWeek + 6) % 7) as u8;
    let minutes = (st.wHour * 60 + st.wMinute) as u16;
    (weekday, minutes)
}

/// System idle time in ms (`GetLastInputInfo`). Both values compared as u32 with wrapping, and the
/// result clamped, per WINDOWS-API gotcha 1 (49-day wrap + non-monotonic `dwTime`).
pub fn system_idle_ms() -> u64 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut lii = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        let _ = GetLastInputInfo(&mut lii);
        let raw = GetTickCount().wrapping_sub(lii.dwTime);
        const SANITY_MAX: u32 = 1000 * 60 * 60 * 24 * 7; // a week
        if raw > SANITY_MAX {
            0
        } else {
            raw as u64
        }
    }
}

/// Working-set bytes (`GetProcessMemoryInfo`) for the memory readout.
pub fn working_set_bytes() -> u64 {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut c = PROCESS_MEMORY_COUNTERS::default();
        let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut c, cb);
        c.WorkingSetSize as u64
    }
}
