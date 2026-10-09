//! Window and tray extras (spec 006 FR-015 to FR-017): always on top, the taskbar dot and
//! notifications. Shell code: it talks to Tauri, so it lives outside `core`.

use tauri::image::Image;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::core::autopilot::{Cause, Command};
use crate::SharedAppearance;

const SIZE: u32 = 16;

/// A filled circle in `rgb` on a clear 16x16 square: the taskbar dot.
pub fn dot(rgb: [u8; 3]) -> Vec<u8> {
    let c = (SIZE as f32 - 1.0) / 2.0;
    let r = SIZE as f32 / 2.0 - 1.0;
    (0..SIZE * SIZE)
        .flat_map(|i| {
            let (x, y) = ((i % SIZE) as f32 - c, (i / SIZE) as f32 - c);
            let alpha = if x * x + y * y <= r * r { 255 } else { 0 };
            [rgb[0], rgb[1], rgb[2], alpha]
        })
        .collect()
}

/// The two dots, made once at startup.
pub struct Dots {
    running: Image<'static>,
    paused: Image<'static>,
}

impl Dots {
    pub fn new() -> Self {
        Self {
            running: Image::new_owned(dot([0x2e, 0xa0, 0x43]), SIZE, SIZE),
            paused: Image::new_owned(dot([0xd2, 0x99, 0x22]), SIZE, SIZE),
        }
    }
}

impl Default for Dots {
    fn default() -> Self {
        Self::new()
    }
}

/// Bring an open window in line: always on top, and the dot for running, paused or stopped.
pub fn sync_window(app: &AppHandle, running: bool, paused: bool) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let a = *app.state::<SharedAppearance>().lock().unwrap();
    let _ = w.set_always_on_top(a.always_on_top);
    let icon = match (a.taskbar_dot, running, app.try_state::<Dots>()) {
        (true, true, Some(d)) => Some(if paused {
            d.paused.clone()
        } else {
            d.running.clone()
        }),
        _ => None,
    };
    let _ = w.set_overlay_icon(icon);
}

/// A toast, if notifications are on. Windows may still suppress it (Focus Assist), and a
/// failure is ignored: nothing else depends on it (Review Focus 5).
pub fn notify(app: &AppHandle, body: &str) {
    if !app
        .state::<SharedAppearance>()
        .lock()
        .unwrap()
        .notifications
    {
        return;
    }
    let _ = app
        .notification()
        .builder()
        .title("project-mouse")
        .body(body)
        .show();
}

/// What a notification says when the autopilot acts.
pub fn command_text(cmd: Command) -> &'static str {
    match cmd {
        Command::Start(Cause::Schedule) => "Started by your schedule.",
        Command::Stop(Cause::Schedule) => "Stopped by your schedule.",
        Command::Stop(Cause::RunFor) => "Run for is over, so it stopped.",
        Command::Start(Cause::RunFor) => "Started.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::autopilot::{Cause, Command};

    #[test]
    fn a_dot_is_an_opaque_circle_on_a_clear_square() {
        let px = dot([10, 20, 30]);
        assert_eq!(px.len(), (SIZE * SIZE * 4) as usize);
        let at =
            |x: u32, y: u32| &px[((y * SIZE + x) * 4) as usize..((y * SIZE + x) * 4 + 4) as usize];
        assert_eq!(at(8, 8), &[10, 20, 30, 255], "the centre is filled");
        assert_eq!(at(0, 0)[3], 0, "a corner is clear");
    }

    #[test]
    fn every_command_has_plain_words() {
        for c in [
            Command::Start(Cause::Schedule),
            Command::Stop(Cause::Schedule),
            Command::Stop(Cause::RunFor),
            Command::Start(Cause::RunFor),
        ] {
            let t = command_text(c);
            assert!(!t.is_empty() && !t.contains('\u{2014}'), "{t}");
        }
    }
}
