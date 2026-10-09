//! `InputInjector` via `SendInput` (FEATURES Part C, WINDOWS-API gotchas 3/4/7/8). Every event is
//! tagged with a magic `dwExtraInfo`; down+up go in one call.

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SetCursorPos, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use crate::platform::{InputInjector, PathOutcome, PlatformError, Result};

/// Identifies our own synthetic events in the input stream (gotcha 4).
const MAGIC_EXTRA: usize = 0x504D_0001; // 'PM'

#[derive(Default)]
pub struct WindowsInputInjector;

impl WindowsInputInjector {
    pub fn new() -> Self {
        Self
    }
}

impl InputInjector for WindowsInputInjector {
    fn virtual_jiggle(&self) -> Result<()> {
        // +1px then -1px in one batch → net-zero visible movement, still registers as input
        // (gotcha 7: a 0,0 move can be coalesced away, so move by 1 and back).
        let mouse = |dx: i32| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE,
                    time: 0,
                    dwExtraInfo: MAGIC_EXTRA,
                },
            },
        };
        send(&[mouse(1), mouse(-1)])
    }

    fn key(&self, vk: u16) -> Result<()> {
        let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: MAGIC_EXTRA,
                },
            },
        };
        send(&[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)])
    }

    fn move_path(
        &self,
        steps: &[(i32, i32)],
        step_ms: u32,
        abortable: bool,
    ) -> Result<PathOutcome> {
        let started = std::time::Instant::now();
        let origin = cursor_pos().ok_or_else(|| PlatformError("GetCursorPos failed".into()))?;
        let desk = Desk::current();
        let mut target = origin;
        let mut aborted = false;
        for (i, &(dx, dy)) in steps.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(step_ms as u64));
                // The cursor is not where our last step put it: the user has the mouse. Let go.
                if abortable
                    && cursor_pos()
                        .is_some_and(|(x, y)| (x - target.0).abs() > 2 || (y - target.1).abs() > 2)
                {
                    aborted = true;
                    break;
                }
            }
            target = desk.clamp((target.0 + dx, target.1 + dy));
            send(&[absolute(desk.normalise(target))])?;
        }
        // Absolute coordinates round to 1/65535 of the desktop, and a step clipped at a screen
        // edge leaves the sum short. Either way, land exactly where we began. SetCursorPos is not
        // input, so this changes nothing about the idle clock.
        if !aborted && cursor_pos() != Some(origin) {
            unsafe {
                let _ = SetCursorPos(origin.0, origin.1);
            }
        }
        Ok(PathOutcome {
            elapsed_ms: started.elapsed().as_millis() as u32,
            aborted,
        })
    }
}

fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok().map(|_| (p.x, p.y))
}

/// The virtual desktop: every monitor's rectangle together, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Desk {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Desk {
    fn current() -> Self {
        unsafe {
            Desk {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
                w: GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2),
                h: GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2),
            }
        }
    }

    fn clamp(self, (px, py): (i32, i32)) -> (i32, i32) {
        (
            px.clamp(self.x, self.x + self.w - 1),
            py.clamp(self.y, self.y + self.h - 1),
        )
    }

    /// `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK` coordinates: 0..=65535 across the whole
    /// virtual desktop, rounded to the nearest so the pixel comes back unchanged.
    fn normalise(self, (px, py): (i32, i32)) -> (i32, i32) {
        let n = |v: i32, origin: i32, len: i32| {
            let span = (len - 1).max(1) as i64;
            (((v - origin) as i64 * 65_535 + span / 2) / span) as i32
        };
        (n(px, self.x, self.w), n(py, self.y, self.h))
    }
}

fn absolute((nx, ny): (i32, i32)) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: nx,
                dy: ny,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                time: 0,
                dwExtraInfo: MAGIC_EXTRA,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<()> {
    // NOTE: a full count does NOT mean the input landed, UIPI discards silently (gotcha 3).
    // That case is caught by verifying the idle clock reset (C7), not here.
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(PlatformError(
            "SendInput did not dispatch all events (blocked)".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: Desk = Desk {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    /// A second monitor to the left of the primary: the virtual desktop starts at x = -1280.
    const TWO: Desk = Desk {
        x: -1280,
        y: 0,
        w: 3200,
        h: 1080,
    };

    #[test]
    fn normalise_maps_the_corners_to_the_ends_of_the_range() {
        assert_eq!(ONE.normalise((0, 0)), (0, 0));
        assert_eq!(ONE.normalise((1919, 1079)), (65_535, 65_535));
        assert_eq!(TWO.normalise((-1280, 0)), (0, 0));
        assert_eq!(TWO.normalise((1919, 1079)), (65_535, 65_535));
    }

    #[test]
    fn normalise_round_trips_to_the_same_pixel() {
        // Windows maps back with x = nx * (w - 1) / 65535. The rounding must not move us a pixel.
        for px in [0, 1, 17, 640, 959, 1918, 1919] {
            let (nx, _) = ONE.normalise((px, 0));
            let back = (nx as i64 * (ONE.w as i64 - 1) + 32_767) / 65_535;
            assert_eq!(back, px as i64, "pixel {px} came back as {back}");
        }
    }

    #[test]
    fn clamp_keeps_a_point_on_the_desktop() {
        assert_eq!(ONE.clamp((-5, 2000)), (0, 1079));
        assert_eq!(TWO.clamp((-2000, -1)), (-1280, 0));
        assert_eq!(ONE.clamp((100, 100)), (100, 100));
    }
}
