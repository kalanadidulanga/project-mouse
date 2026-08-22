//! `PowerSource`: AC line status + battery percent via `GetSystemPowerStatus` (WINDOWS-API B5).

use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

use crate::platform::PowerSource;

#[derive(Default)]
pub struct WindowsPowerSource;

impl WindowsPowerSource {
    pub fn new() -> Self {
        Self
    }
}

impl PowerSource for WindowsPowerSource {
    fn power_status(&self) -> (bool, u8) {
        unsafe {
            let mut s = SYSTEM_POWER_STATUS::default();
            if GetSystemPowerStatus(&mut s).is_ok() {
                // ACLineStatus: 0 = on battery, 1 = plugged in, 255 = unknown (usually a desktop).
                let on_ac = s.ACLineStatus != 0;
                // BatteryLifePercent: 0..100, or 255 when unknown / no battery.
                let pct = if s.BatteryLifePercent == 255 {
                    100
                } else {
                    s.BatteryLifePercent
                };
                (on_ac, pct)
            } else {
                (true, 100)
            }
        }
    }
}
