//! Start/Stop (spec 005 FR-003). One running state over the two engines, which stay separate
//! (constitution I): Start holds a power mode and, unless *Move the mouse* is off, enables the
//! input engine. Everything that starts or stops, window, tray, hotkey, CLI, goes through here.

use serde::{Deserialize, Serialize};

use crate::core::autopilot::PauseReason;
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
    /// Pause moves and keep-awake while on battery, so the PC can sleep (spec 006 FR-008).
    pub pause_on_battery: bool,
    /// Pause moves while the screen is locked; keep-awake continues (FR-009).
    pub pause_when_locked: bool,
    /// Pause moves while presenting or a full-screen app or game is up; keep-awake continues (FR-027).
    pub pause_when_presenting: bool,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            move_mouse: true,
            keep_screen_on: true,
            start_on_launch: false,
            pause_on_battery: false,
            pause_when_locked: true,
            pause_when_presenting: false,
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

/// Set the manual mode and bring both engines into line (spec 005 FR-003, spec 006 FR-012):
/// - input on <=> running and Move the mouse and not paused
/// - Start's power request held <=> running and not battery-paused
///
/// A pause never changes whether it is running.
pub fn apply(
    engine: &mut Engine,
    input: &mut InputEngine,
    settings: &RunSettings,
    mode: WakeMode,
    pause: Option<PauseReason>,
) {
    engine.set_manual(mode);
    engine.set_manual_suspended(pause.is_some_and(|p| !p.holds_power()));
    input.set_enabled(mode != WakeMode::Off && settings.move_mouse && pause.is_none());
}

/// Start or stop with what Start means right now. Calling it with the current state re-applies
/// changed settings without changing whether it runs.
#[cfg(test)] // the shell's `set_running` (lib.rs) does this under its own lock order; tests pin the rule
pub fn set_running(engine: &mut Engine, input: &mut InputEngine, settings: &RunSettings, on: bool) {
    let mode = if on {
        settings.start_mode()
    } else {
        WakeMode::Off
    };
    apply(engine, input, settings, mode, None);
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
    /// Running, with moves paused (battery, locked, presenting or a blackout).
    Paused,
}

pub fn status_kind(
    running: bool,
    move_mouse: bool,
    blocked: bool,
    effective: WakeMode,
    paused: bool,
) -> StatusKind {
    match (running, paused, move_mouse, blocked) {
        (false, ..) if effective != WakeMode::Off => StatusKind::StoppedButRuleHolds,
        (false, ..) => StatusKind::Stopped,
        (true, true, ..) => StatusKind::Paused,
        (true, false, false, _) => StatusKind::RunningPowerOnly,
        (true, false, true, true) => StatusKind::RunningBlocked,
        (true, false, true, false) => StatusKind::Running,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::core::autopilot::PauseReason;
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
            None,
        );
        assert!(i.enabled());
        let off = RunSettings {
            move_mouse: false,
            ..RunSettings::default()
        };
        apply(&mut e, &mut i, &off, WakeMode::KeepRunning, None);
        assert!(!i.enabled());
        apply(&mut e, &mut i, &RunSettings::default(), WakeMode::Off, None);
        assert!(!i.enabled());
    }

    #[test]
    fn status_says_what_is_true() {
        use StatusKind::*;
        use WakeMode::*;
        assert_eq!(status_kind(false, true, false, Off, false), Stopped);
        assert_eq!(
            status_kind(false, true, false, KeepRunning, false),
            StoppedButRuleHolds
        );
        assert_eq!(
            status_kind(true, true, false, KeepPresenting, false),
            Running
        );
        assert_eq!(
            status_kind(true, true, true, KeepPresenting, false),
            RunningBlocked
        );
        assert_eq!(
            status_kind(true, false, false, KeepPresenting, false),
            RunningPowerOnly
        );
        // A stale `blocked` flag means nothing once moves are off.
        assert_eq!(
            status_kind(true, false, true, KeepPresenting, false),
            RunningPowerOnly
        );
    }

    fn engines_with_guard() -> (Engine, InputEngine, MockPowerGuard) {
        let guard = MockPowerGuard::default();
        (
            Engine::new(Arc::new(guard.clone())),
            InputEngine::new(Arc::new(MockInjector::default()), 0),
            guard,
        )
    }

    /// Review Focus 2: a battery pause keeps Start on, but lets go of power and moves.
    #[test]
    fn a_battery_pause_keeps_running_but_releases_power_and_moves() {
        let (mut e, mut i, guard) = engines_with_guard();
        let s = RunSettings::default();
        apply(&mut e, &mut i, &s, s.start_mode(), None);
        e.tick(&Default::default());
        assert_eq!(guard.held(), Some(true));
        apply(
            &mut e,
            &mut i,
            &s,
            s.start_mode(),
            Some(PauseReason::Battery),
        );
        e.tick(&Default::default());
        assert!(is_running(&e));
        assert!(!i.enabled());
        assert_eq!(guard.held(), None);
    }

    #[test]
    fn a_locked_pause_stops_moves_and_keeps_power() {
        let (mut e, mut i, guard) = engines_with_guard();
        let s = RunSettings::default();
        apply(
            &mut e,
            &mut i,
            &s,
            s.start_mode(),
            Some(PauseReason::Locked),
        );
        e.tick(&Default::default());
        assert!(!i.enabled());
        assert_eq!(guard.held(), Some(true), "keep presenting is still held");
    }

    #[test]
    fn resuming_restores_moves_and_power() {
        let (mut e, mut i, guard) = engines_with_guard();
        let s = RunSettings::default();
        apply(
            &mut e,
            &mut i,
            &s,
            s.start_mode(),
            Some(PauseReason::Battery),
        );
        e.tick(&Default::default());
        apply(&mut e, &mut i, &s, s.start_mode(), None);
        e.tick(&Default::default());
        assert!(i.enabled());
        assert_eq!(guard.held(), Some(true));
    }

    #[test]
    fn a_pause_shows_as_paused_only_while_running() {
        use StatusKind::*;
        use WakeMode::*;
        assert_eq!(status_kind(true, true, false, KeepPresenting, true), Paused);
        assert_eq!(status_kind(true, false, false, KeepRunning, true), Paused);
        assert_eq!(status_kind(false, true, false, Off, true), Stopped);
    }
}
