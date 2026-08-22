//! The scheduler tick. A waitable timer with a tolerable delay lets Windows coalesce our wake-up
//! with others, so idle CPU stays ~0 (ARCHITECTURE §7). `on_tick` returns `false` to stop.

/// Run `on_tick` every ~`period_ms`. Returns when `on_tick` returns `false`.
#[cfg(windows)]
pub fn run_loop(period_ms: u32, tolerable_ms: u32, on_tick: impl FnMut() -> bool) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CreateWaitableTimerExW, SetWaitableTimerEx, WaitForSingleObject,
        CREATE_WAITABLE_TIMER_MANUAL_RESET, TIMER_ALL_ACCESS,
    };

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
        if SetWaitableTimerEx(timer, &due, period_ms as i32, None, None, None, tolerable_ms).is_err() {
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

#[cfg(windows)]
fn sleep_loop(period_ms: u32, mut on_tick: impl FnMut() -> bool) {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(period_ms as u64));
        if !on_tick() {
            break;
        }
    }
}

#[cfg(not(windows))]
pub fn run_loop(period_ms: u32, _tolerable_ms: u32, mut on_tick: impl FnMut() -> bool) {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(period_ms as u64));
        if !on_tick() {
            break;
        }
    }
}
