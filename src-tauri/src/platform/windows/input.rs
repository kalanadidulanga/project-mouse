//! `InputInjector` via `SendInput` (FEATURES Part C, WINDOWS-API gotchas 3/4/7/8). Every event is
//! tagged with a magic `dwExtraInfo`; down+up go in one call.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, MOUSEEVENTF_MOVE, MOUSEINPUT, VIRTUAL_KEY,
};

use crate::platform::{InputInjector, PlatformError, Result};

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
}

fn send(inputs: &[INPUT]) -> Result<()> {
    // NOTE: a full count does NOT mean the input landed — UIPI discards silently (gotcha 3).
    // That case is caught by verifying the idle clock reset (C7), not here.
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(PlatformError("SendInput did not dispatch all events (blocked)".into()));
    }
    Ok(())
}
