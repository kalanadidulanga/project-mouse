//! The tray (spec 005 FR-009): colour while running, grey while stopped, a Start|Stop menu, and a
//! tooltip that says what is true right now.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::core::running::StatusKind;
use crate::{SharedEngine, SharedProfiles};

/// The running and stopped icons, made once at startup.
pub struct Icons {
    pub on: Image<'static>,
    pub off: Image<'static>,
}

impl Icons {
    pub fn new(on: Image<'static>) -> Self {
        let off = Image::new_owned(greyscale(on.rgba()), on.width(), on.height());
        Self { on, off }
    }
}

/// The stopped icon: the same mark, desaturated and dimmed. Both icons have the same shape, so
/// only saturation and alpha differ; the tooltip says the state in words, so it never rests on
/// colour alone (UI-UX §7).
pub fn greyscale(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let l = ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000) as u8;
            [l, l, l, (p[3] as u32 * 3 / 5) as u8]
        })
        .collect()
}

/// `m:ss`, as Home shows it.
pub fn countdown(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn remaining(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        _ => format!("{}h {}m", secs / 3600, (secs % 3600) / 60),
    }
}

/// The tooltip: what is true now, when the next move is, how long a timed rule has left, and
/// whether an update is waiting.
pub fn tooltip(
    kind: StatusKind,
    next_move_secs: Option<u32>,
    rule_left_secs: Option<u64>,
    update: Option<&str>,
) -> String {
    let state = match kind {
        StatusKind::Stopped => "Stopped".to_string(),
        StatusKind::StoppedButRuleHolds => "Stopped · a rule is keeping the PC awake".to_string(),
        StatusKind::Running => match next_move_secs {
            Some(n) => format!("Running · next move in {}", countdown(n)),
            None => "Running".to_string(),
        },
        StatusKind::RunningPowerOnly => "Running · keeping the PC awake".to_string(),
        StatusKind::RunningBlocked => "Running · Windows blocked the last move".to_string(),
    };
    let mut s = format!("project-mouse: {state}");
    if let Some(left) = rule_left_secs {
        s.push_str(&format!(" · rule ends in {}", remaining(left)));
    }
    if let Some(v) = update {
        s.push_str(&format!("\nUpdate {v} available"));
    }
    s
}

/// The menu, rebuilt rather than patched: the Start|Stop label, the autostart check and the
/// profile list must all stay truthful, and regenerating is cheaper than tracking each.
pub fn menu(app: &AppHandle, running: bool) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let toggle = item("toggle", if running { "Stop" } else { "Start" })?;
    let open = item("open", "Open project-mouse")?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start with Windows",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let update = item("check_update", "Check for updates…")?;
    let quit = item("quit", "Quit")?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let profiles = profiles_menu(app)?;

    let mut items: Vec<&dyn IsMenuItem<Wry>> = Vec::with_capacity(8);
    items.push(&toggle);
    items.push(&open);
    items.push(&sep1);
    if let Some(p) = &profiles {
        items.push(p);
    }
    items.push(&autostart);
    items.push(&update);
    items.push(&sep2);
    items.push(&quit);
    Menu::with_items(app, &items)
}

/// Profiles earn a place in the tray only once there is more than one to switch between.
fn profiles_menu(app: &AppHandle) -> tauri::Result<Option<Submenu<Wry>>> {
    let active = app
        .try_state::<SharedEngine>()
        .map(|e| e.lock().unwrap().profile().id.clone())
        .unwrap_or_default();
    let stored = app
        .try_state::<SharedProfiles>()
        .map(|p| p.lock().unwrap().clone())
        .unwrap_or_default();
    if stored.len() < 2 {
        return Ok(None);
    }
    let entries = stored
        .iter()
        .map(|p| {
            CheckMenuItem::with_id(
                app,
                format!("profile:{}", p.id),
                &p.name,
                true,
                p.id == active,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let refs: Vec<&dyn IsMenuItem<Wry>> =
        entries.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    Submenu::with_items(app, "Profile", true, &refs).map(Some)
}

/// Bring the icon and the menu in line with the running state.
pub fn sync(app: &AppHandle) {
    let running = crate::is_running(app);
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    if let Some(icons) = app.try_state::<Icons>() {
        let icon = if running { &icons.on } else { &icons.off };
        let _ = tray.set_icon(Some(icon.clone()));
    }
    match menu(app, running) {
        Ok(m) => {
            if let Err(e) = tray.set_menu(Some(m)) {
                tracing::error!("tray menu update failed: {e}");
            }
        }
        Err(e) => tracing::error!("tray menu build failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greyscale_keeps_the_size_and_dims_the_icon() {
        let g = greyscale(&[255, 0, 0, 255, 10, 20, 30, 0]);
        assert_eq!(g.len(), 8);
        assert_eq!(&g[..4], &[76, 76, 76, 153]);
        assert_eq!(g[7], 0, "transparent stays transparent");
    }

    #[test]
    fn grey_stays_grey() {
        assert_eq!(greyscale(&[128, 128, 128, 255]), vec![128, 128, 128, 153]);
    }

    #[test]
    fn the_countdown_reads_like_a_clock() {
        assert_eq!(countdown(0), "0:00");
        assert_eq!(countdown(42), "0:42");
        assert_eq!(countdown(200), "3:20");
        assert_eq!(countdown(3_600), "60:00");
    }

    #[test]
    fn the_tooltip_says_what_is_true() {
        use StatusKind::*;
        assert_eq!(tooltip(Stopped, None, None, None), "project-mouse: Stopped");
        assert_eq!(
            tooltip(Running, Some(42), None, None),
            "project-mouse: Running · next move in 0:42"
        );
        assert!(tooltip(RunningBlocked, None, None, None).contains("blocked"));
        assert!(tooltip(RunningPowerOnly, None, None, None).contains("keeping the PC awake"));
        assert!(tooltip(StoppedButRuleHolds, None, Some(4_000), None).contains("1h 6m"));
        assert!(tooltip(Stopped, None, None, Some("0.3.0")).ends_with("\nUpdate 0.3.0 available"));
    }

    /// Windows cuts a tray tooltip at 127 characters.
    #[test]
    fn every_tooltip_fits() {
        use StatusKind::*;
        for k in [
            Stopped,
            StoppedButRuleHolds,
            Running,
            RunningBlocked,
            RunningPowerOnly,
        ] {
            let t = tooltip(k, Some(3_600), Some(86_399), Some("10.10.10"));
            assert!(t.chars().count() <= 127, "{} chars: {t}", t.chars().count());
        }
    }
}
