//! Windows implementations of the platform traits.

pub mod foreground;
pub mod input;
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

/// Raw `GetLastInputInfo.dwTime` (u32) — same clock domain as `tick_now`.
pub fn last_input_tick() -> u32 {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut lii = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        let _ = GetLastInputInfo(&mut lii);
    }
    lii.dwTime
}

/// Raw `GetTickCount` (u32).
pub fn tick_now() -> u32 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    unsafe { GetTickCount() }
}

/// The scheduler tick: a waitable timer with a tolerable delay lets Windows coalesce our wake-up
/// with others, so idle CPU stays ~0 (ARCHITECTURE §7). `on_tick` returns `false` to stop.
pub fn run_tick_loop(period_ms: u32, tolerable_ms: u32, on_tick: impl FnMut() -> bool) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CreateWaitableTimerExW, SetWaitableTimerEx, WaitForSingleObject,
        CREATE_WAITABLE_TIMER_MANUAL_RESET, TIMER_ALL_ACCESS,
    };

    fn sleep_loop(period_ms: u32, mut on_tick: impl FnMut() -> bool) {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(period_ms as u64));
            if !on_tick() {
                break;
            }
        }
    }

    unsafe {
        let timer = match CreateWaitableTimerExW(
            None,
            PCWSTR::null(),
            CREATE_WAITABLE_TIMER_MANUAL_RESET,
            TIMER_ALL_ACCESS.0,
        ) {
            Ok(h) => h,
            Err(_) => return sleep_loop(period_ms, on_tick),
        };
        let due: i64 = -(period_ms as i64) * 10_000; // relative, 100 ns units
        if SetWaitableTimerEx(
            timer,
            &due,
            period_ms as i32,
            None,
            None,
            None,
            tolerable_ms,
        )
        .is_err()
        {
            let _ = CloseHandle(timer);
            return sleep_loop(period_ms, on_tick);
        }
        let mut on_tick = on_tick;
        loop {
            WaitForSingleObject(timer, u32::MAX);
            if !on_tick() {
                break;
            }
        }
        let _ = CloseHandle(timer);
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
