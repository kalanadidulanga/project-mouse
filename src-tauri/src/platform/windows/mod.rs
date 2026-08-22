//! Windows implementations of the platform traits. The only real backend in M1.

pub mod power;

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
