//! Windows implementations of the platform traits.

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
