//! Start/Stop (spec 005 FR-003). One running state over the two engines, which stay separate
//! (constitution I): Start holds a power mode and, unless *Move the mouse* is off, enables the
//! input engine. Everything that starts or stops — window, tray, hotkey, CLI — goes through here.

#![allow(dead_code)] // wired in Task 4/5

use serde::{Deserialize, Serialize};

use crate::core::engine::Engine;
use crate::core::input_engine::InputEngine;
use crate::core::modes::WakeMode;

/// What Start means. Saved as `run` in config v3. Whether it is running is deliberately not
/// saved: `start_on_launch` is the explicit way to come back running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunSettings {
    /// Synthesize input while running. Off = a power-only Start.
    pub move_mouse: bool,
    /// Hold the display on as well as the system.
    pub keep_screen_on: bool,
    /// Start as soon as the app opens.
    pub start_on_launch: bool,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            move_mouse: true,
            keep_screen_on: true,
            start_on_launch: false,
        }
    }
}

impl RunSettings {
    /// The power mode Start holds.
    pub fn start_mode(&self) -> WakeMode {
        if self.keep_screen_on {
            WakeMode::KeepPresenting
        } else {
            WakeMode::KeepRunning
        }
    }
}

/// Running ⇔ the manual mode is not Off.
pub fn is_running(engine: &Engine) -> bool {
    engine.manual() != WakeMode::Off
}

/// Set the manual mode and bring the input engine into line: on ⇔ running ∧ Move the mouse.
pub fn apply(engine: &mut Engine, input: &mut InputEngine, settings: &RunSettings, mode: WakeMode) {
    engine.set_manual(mode);
    input.set_enabled(mode != WakeMode::Off && settings.move_mouse);
}

/// Start or stop with what Start means right now. Calling it with the current state re-applies
/// changed settings without changing whether it runs.
pub fn set_running(engine: &mut Engine, input: &mut InputEngine, settings: &RunSettings, on: bool) {
    let mode = if on {
        settings.start_mode()
    } else {
        WakeMode::Off
    };
    apply(engine, input, settings, mode);
}

/// What Home says (spec 005 FR-010). The words live in the UI; the decision lives here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    Stopped,
    /// Stopped, but a rule (Advanced) holds power anyway.
    StoppedButRuleHolds,
    Running,
    /// Running, but Windows discarded or refused the last move.
    RunningBlocked,
    /// Running with Move the mouse off.
    RunningPowerOnly,
}

pub fn status_kind(
    running: bool,
    move_mouse: bool,
    blocked: bool,
    effective: WakeMode,
) -> StatusKind {
    match (running, move_mouse, blocked) {
        (false, _, _) if effective != WakeMode::Off => StatusKind::StoppedButRuleHolds,
        (false, _, _) => StatusKind::Stopped,
        (true, false, _) => StatusKind::RunningPowerOnly,
        (true, true, true) => StatusKind::RunningBlocked,
        (true, true, false) => StatusKind::Running,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::platform::mock::{MockInjector, MockPowerGuard};

    fn engines() -> (Engine, InputEngine) {
        (
            Engine::new(Arc::new(MockPowerGuard::default())),
            InputEngine::new(Arc::new(MockInjector::default()), 0),
        )
    }

    #[test]
    fn defaults_move_the_mouse_and_keep_the_screen_on_but_do_not_autostart() {
        let s = RunSettings::default();
        assert!(s.move_mouse && s.keep_screen_on && !s.start_on_launch);
    }

    #[test]
    fn start_holds_power_and_enables_moves() {
        let (mut e, mut i) = engines();
        set_running(&mut e, &mut i, &RunSettings::default(), true);
        assert!(is_running(&e));
        assert_eq!(e.manual(), WakeMode::KeepPresenting);
        assert!(i.enabled());
    }

    #[test]
    fn stop_releases_both() {
        let (mut e, mut i) = engines();
        let s = RunSettings::default();
        set_running(&mut e, &mut i, &s, true);
        set_running(&mut e, &mut i, &s, false);
        assert!(!is_running(&e));
        assert_eq!(e.manual(), WakeMode::Off);
        assert!(!i.enabled());
    }

    #[test]
    fn move_the_mouse_off_is_a_power_only_start() {
        let (mut e, mut i) = engines();
        let s = RunSettings {
            move_mouse: false,
            ..RunSettings::default()
        };
        set_running(&mut e, &mut i, &s, true);
        assert!(is_running(&e));
        assert!(!i.enabled());
    }

    #[test]
    fn keep_the_screen_on_off_starts_keep_running() {
        let (mut e, mut i) = engines();
        let s = RunSettings {
            keep_screen_on: false,
            ..RunSettings::default()
        };
        set_running(&mut e, &mut i, &s, true);
        assert_eq!(e.manual(), WakeMode::KeepRunning);
    }

    /// Review Focus 4.
    #[test]
    fn new_settings_take_effect_while_running_and_keep_it_running() {
        let (mut e, mut i) = engines();
        let mut s = RunSettings::default();
        set_running(&mut e, &mut i, &s, true);
        s.move_mouse = false;
        let on = is_running(&e);
        set_running(&mut e, &mut i, &s, on);
        assert!(is_running(&e));
        assert!(!i.enabled());
        s.keep_screen_on = false;
        let on = is_running(&e);
        set_running(&mut e, &mut i, &s, on);
        assert_eq!(e.manual(), WakeMode::KeepRunning);
    }

    #[test]
    fn new_settings_do_not_start_a_stopped_app() {
        let (mut e, mut i) = engines();
        let s = RunSettings {
            keep_screen_on: false,
            ..RunSettings::default()
        };
        let on = is_running(&e);
        set_running(&mut e, &mut i, &s, on);
        assert!(!is_running(&e));
        assert!(!i.enabled());
    }

    /// `--keep running` from the CLI picks the mode; Move the mouse still decides the input.
    #[test]
    fn an_explicit_mode_still_follows_move_the_mouse() {
        let (mut e, mut i) = engines();
        apply(
            &mut e,
            &mut i,
            &RunSettings::default(),
            WakeMode::KeepRunning,
        );
        assert!(i.enabled());
        let off = RunSettings {
            move_mouse: false,
            ..RunSettings::default()
        };
        apply(&mut e, &mut i, &off, WakeMode::KeepRunning);
        assert!(!i.enabled());
        apply(&mut e, &mut i, &RunSettings::default(), WakeMode::Off);
        assert!(!i.enabled());
    }

    #[test]
    fn status_says_what_is_true() {
        use StatusKind::*;
        use WakeMode::*;
        assert_eq!(status_kind(false, true, false, Off), Stopped);
        assert_eq!(
            status_kind(false, true, false, KeepRunning),
            StoppedButRuleHolds
        );
        assert_eq!(status_kind(true, true, false, KeepPresenting), Running);
        assert_eq!(
            status_kind(true, true, true, KeepPresenting),
            RunningBlocked
        );
        assert_eq!(
            status_kind(true, false, false, KeepPresenting),
            RunningPowerOnly
        );
        // A stale `blocked` flag means nothing once moves are off.
        assert_eq!(
            status_kind(true, false, true, KeepPresenting),
            RunningPowerOnly
        );
    }
}
