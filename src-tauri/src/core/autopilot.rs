//! When Start runs and when it pauses (spec 006 FR-007 to FR-013, FR-027). Pure: it is fed the
//! settings, the Run-for deadline, the previous tick's local time and a `Snapshot`. It decides,
//! and the shell acts. No OS code, so every rule here has a unit test.

#![allow(dead_code)] // wired in Task 7
use serde::{Deserialize, Serialize};

use crate::core::rule::NotifState;
use crate::core::running::RunSettings;
use crate::core::snapshot::Snapshot;

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleAction {
    Start,
    Stop,
}

/// "These days, at this time, Start (or Stop)" (FR-011). `days[0]` is Monday, and `at` is a local
/// minute of the day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub days: [bool; 7],
    pub at: u16,
    pub action: ScheduleAction,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// A quiet window with no mouse moves (FR-010). `from > to` crosses midnight, and `days` name the
/// day a window starts on. `from == to` is empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blackout {
    pub days: [bool; 7],
    pub from: u16,
    pub to: u16,
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl Blackout {
    /// `Some(end)` while this window is active at `weekday` (0 = Monday), `minute`.
    pub fn active_until(&self, weekday: u8, minute: u16) -> Option<u16> {
        if !self.enabled || self.from == self.to {
            return None;
        }
        let today = self.days[weekday as usize % 7];
        let yesterday = self.days[(weekday as usize + 6) % 7];
        let on = if self.from < self.to {
            today && minute >= self.from && minute < self.to
        } else {
            (today && minute >= self.from) || (yesterday && minute < self.to)
        };
        on.then_some(self.to)
    }
}

/// Everything time-based the user set up: schedules and blackouts. Saved as `timetable`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timetable {
    #[serde(default)]
    pub schedules: Vec<Schedule>,
    #[serde(default)]
    pub blackouts: Vec<Blackout>,
}

impl Timetable {
    /// Times past the end of a day are clamped to 23:59, so a hand-edited value is visible and
    /// fixable rather than silently never firing (Review Focus 3).
    pub fn sanitised(mut self) -> Self {
        for s in &mut self.schedules {
            s.at = s.at.min(1439);
        }
        for b in &mut self.blackouts {
            b.from = b.from.min(1439);
            b.to = b.to.min(1439);
        }
        self
    }
}

/// Why moves (and, on battery, keep-awake) are paused. Home shows it, tagged by `reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum PauseReason {
    Battery,
    Locked,
    Presenting,
    Blackout { until: u16 },
}

impl PauseReason {
    /// Only a battery pause lets go of the power request. The others pause moves and keep the PC
    /// awake, so whatever is running can continue and the moves can resume afterwards.
    pub fn holds_power(self) -> bool {
        !matches!(self, PauseReason::Battery)
    }
}

/// The first pause that applies, in priority order: battery, locked, presenting, blackout.
pub fn pause_reason(
    s: &RunSettings,
    blackouts: &[Blackout],
    snap: &Snapshot,
) -> Option<PauseReason> {
    if s.pause_on_battery && !snap.on_ac {
        return Some(PauseReason::Battery);
    }
    if s.pause_when_locked && snap.session_locked {
        return Some(PauseReason::Locked);
    }
    if s.pause_when_presenting
        && matches!(
            snap.notification_state,
            NotifState::Presentation | NotifState::Busy | NotifState::Game
        )
    {
        return Some(PauseReason::Presenting);
    }
    blackouts
        .iter()
        .find_map(|b| b.active_until(snap.weekday, snap.minutes))
        .map(|until| PauseReason::Blackout { until })
}

/// What made the autopilot act, for the notification text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Schedule,
    RunFor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Start(Cause),
    Stop(Cause),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Decision {
    pub command: Option<Command>,
    pub pause: Option<PauseReason>,
}

/// How long Start runs (FR-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunFor {
    Forever,
    Minutes { minutes: u32 },
    Until { at: u16 },
}

/// The epoch second a Run-for choice ends. "Until 09:00", set at 10:00, means tomorrow. An
/// "until" lands on the start of its minute.
pub fn deadline_for(run_for: RunFor, epoch: u64, minute_now: u16) -> Option<u64> {
    match run_for {
        RunFor::Forever => None,
        RunFor::Minutes { minutes } => Some(epoch + minutes.max(1) as u64 * 60),
        RunFor::Until { at } => {
            let ahead = (at.min(1439) as i64 - minute_now as i64).rem_euclid(1440);
            let ahead = if ahead == 0 { 1440 } else { ahead } as u64;
            Some(epoch - epoch % 60 + ahead * 60)
        }
    }
}

/// Schedule edges and the Run-for deadline, tick by tick.
#[derive(Debug, Default)]
pub struct Autopilot {
    /// The previous tick's local (weekday, minute). `None` before the first tick, so launching
    /// never fires an edge.
    last: Option<(u8, u16)>,
    deadline: Option<u64>,
}

impl Autopilot {
    pub fn set_deadline(&mut self, at: Option<u64>) {
        self.deadline = at;
    }

    pub fn deadline(&self) -> Option<u64> {
        self.deadline
    }

    /// One tick: the command to carry out, if any, and the pause that applies afterwards. A
    /// deadline is checked before schedules; any Stop clears the deadline.
    pub fn tick(
        &mut self,
        s: &RunSettings,
        t: &Timetable,
        snap: &Snapshot,
        running: bool,
    ) -> Decision {
        let now = (snap.weekday, snap.minutes);
        let mut command = None;
        if running && self.deadline.is_some_and(|d| snap.epoch_secs >= d) {
            command = Some(Command::Stop(Cause::RunFor));
        } else if let Some(prev) = self.last {
            command = match (crossed(&t.schedules, prev, now), running) {
                (Some(ScheduleAction::Start), false) => Some(Command::Start(Cause::Schedule)),
                (Some(ScheduleAction::Stop), true) => Some(Command::Stop(Cause::Schedule)),
                _ => None,
            };
        }
        self.last = Some(now);
        let running_after = match command {
            Some(Command::Start(_)) => true,
            Some(Command::Stop(_)) => {
                self.deadline = None;
                false
            }
            None => running,
        };
        let pause = if running_after {
            pause_reason(s, &t.blackouts, snap)
        } else {
            None
        };
        Decision { command, pause }
    }
}

/// The schedule edge crossed since the previous tick, today only. The latest time wins, and
/// Stop wins a tie (Review Focus 1). A clock going backwards within the day fires nothing.
fn crossed(
    schedules: &[Schedule],
    (pd, pm): (u8, u16),
    (nd, nm): (u8, u16),
) -> Option<ScheduleAction> {
    let lower: i32 = if pd == nd {
        if nm < pm {
            return None;
        }
        pm as i32
    } else {
        -1
    };
    schedules
        .iter()
        .filter(|s| s.enabled && s.days[nd as usize % 7] && s.at as i32 > lower && s.at <= nm)
        .max_by_key(|s| (s.at, s.action == ScheduleAction::Stop))
        .map(|s| s.action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rule::NotifState;

    const EVERY_DAY: [bool; 7] = [true; 7];
    const MONDAY_ONLY: [bool; 7] = [true, false, false, false, false, false, false];

    fn snap(weekday: u8, minutes: u16) -> Snapshot {
        Snapshot {
            weekday,
            minutes,
            epoch_secs: 1_000_050,
            ..Default::default()
        }
    }
    fn at(minute: u16, action: ScheduleAction) -> Schedule {
        Schedule {
            days: EVERY_DAY,
            at: minute,
            action,
            enabled: true,
        }
    }
    fn table(schedules: Vec<Schedule>) -> Timetable {
        Timetable {
            schedules,
            blackouts: Vec::new(),
        }
    }
    fn run() -> RunSettings {
        RunSettings::default()
    }
    use ScheduleAction::{Start, Stop};

    #[test]
    fn the_first_tick_fires_nothing() {
        let mut a = Autopilot::default();
        let d = a.tick(&run(), &table(vec![at(540, Start)]), &snap(0, 540), false);
        assert_eq!(d.command, None);
    }

    #[test]
    fn crossing_a_start_time_starts() {
        let mut a = Autopilot::default();
        let t = table(vec![at(540, Start)]);
        a.tick(&run(), &t, &snap(0, 539), false);
        assert_eq!(
            a.tick(&run(), &t, &snap(0, 540), false).command,
            Some(Command::Start(Cause::Schedule))
        );
        assert_eq!(
            a.tick(&run(), &t, &snap(0, 540), true).command,
            None,
            "fires once"
        );
    }

    #[test]
    fn a_start_while_running_and_a_stop_while_stopped_do_nothing() {
        let mut a = Autopilot::default();
        let t = table(vec![at(540, Start), at(600, Stop)]);
        a.tick(&run(), &t, &snap(0, 539), true);
        assert_eq!(a.tick(&run(), &t, &snap(0, 540), true).command, None);
        a.tick(&run(), &t, &snap(0, 599), false);
        assert_eq!(a.tick(&run(), &t, &snap(0, 600), false).command, None);
    }

    /// Review Focus 1: asleep across both edges, the later one wins.
    #[test]
    fn a_sleep_gap_fires_the_latest_edge_crossed() {
        let t = table(vec![at(540, Start), at(560, Stop)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 500), true);
        assert_eq!(
            a.tick(&run(), &t, &snap(0, 600), true).command,
            Some(Command::Stop(Cause::Schedule))
        );

        let t = table(vec![at(520, Stop), at(540, Start)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 500), false);
        assert_eq!(
            a.tick(&run(), &t, &snap(0, 600), false).command,
            Some(Command::Start(Cause::Schedule))
        );
    }

    #[test]
    fn a_start_and_a_stop_at_the_same_minute_stop_wins() {
        let t = table(vec![at(540, Start), at(540, Stop)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 539), true);
        assert_eq!(
            a.tick(&run(), &t, &snap(0, 540), true).command,
            Some(Command::Stop(Cause::Schedule))
        );
    }

    #[test]
    fn edges_from_a_previous_day_never_fire() {
        let t = table(vec![at(1430, Start)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 1420), false); // Monday 23:40
        assert_eq!(
            a.tick(&run(), &t, &snap(1, 5), false).command,
            None,
            "Tuesday 00:05"
        );
    }

    #[test]
    fn crossing_midnight_fires_a_midnight_edge() {
        let t = table(vec![at(0, Start)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 1439), false);
        assert_eq!(
            a.tick(&run(), &t, &snap(1, 0), false).command,
            Some(Command::Start(Cause::Schedule))
        );
    }

    #[test]
    fn days_and_the_enabled_switch_are_respected() {
        let mut only_monday = at(540, Start);
        only_monday.days = MONDAY_ONLY;
        let mut off = at(540, Start);
        off.enabled = false;
        for t in [table(vec![only_monday]), table(vec![off])] {
            let mut a = Autopilot::default();
            a.tick(&run(), &t, &snap(1, 539), false); // Tuesday
            assert_eq!(a.tick(&run(), &t, &snap(1, 540), false).command, None);
        }
    }

    #[test]
    fn the_clock_going_backwards_fires_nothing() {
        let t = table(vec![at(540, Start)]);
        let mut a = Autopilot::default();
        a.tick(&run(), &t, &snap(0, 600), false);
        assert_eq!(a.tick(&run(), &t, &snap(0, 530), false).command, None);
    }

    #[test]
    fn the_run_for_deadline_stops_and_clears() {
        let mut a = Autopilot::default();
        a.set_deadline(Some(1_000_100));
        let mut s = snap(0, 600);
        s.epoch_secs = 1_000_099;
        assert_eq!(a.tick(&run(), &table(vec![]), &s, true).command, None);
        s.epoch_secs = 1_000_100;
        assert_eq!(
            a.tick(&run(), &table(vec![]), &s, true).command,
            Some(Command::Stop(Cause::RunFor))
        );
        assert_eq!(a.deadline(), None);
    }

    #[test]
    fn a_scheduled_stop_clears_the_deadline() {
        let t = table(vec![at(540, Stop)]);
        let mut a = Autopilot::default();
        a.set_deadline(Some(9_999_999));
        a.tick(&run(), &t, &snap(0, 539), true);
        a.tick(&run(), &t, &snap(0, 540), true);
        assert_eq!(a.deadline(), None);
    }

    #[test]
    fn deadline_for_counts_minutes_and_finds_the_next_clock_time() {
        // epoch 1_000_050 is 30 s into a minute; "until" lands on the minute.
        assert_eq!(deadline_for(RunFor::Forever, 1_000_050, 540), None);
        assert_eq!(
            deadline_for(RunFor::Minutes { minutes: 30 }, 1_000_050, 540),
            Some(1_001_850)
        );
        assert_eq!(
            deadline_for(RunFor::Until { at: 600 }, 1_000_050, 540),
            Some(1_003_620)
        );
        assert_eq!(
            deadline_for(RunFor::Until { at: 540 }, 1_000_050, 600),
            Some(1_082_820),
            "tomorrow"
        );
        assert_eq!(
            deadline_for(RunFor::Until { at: 600 }, 1_000_050, 600),
            Some(1_086_420),
            "same minute: tomorrow"
        );
    }

    #[test]
    fn pauses_follow_their_switches_and_their_priority() {
        let mut s = snap(0, 600);
        s.on_ac = false;
        s.session_locked = true;
        s.notification_state = NotifState::Presentation;
        let all = RunSettings {
            pause_on_battery: true,
            pause_when_locked: true,
            pause_when_presenting: true,
            ..run()
        };
        assert_eq!(pause_reason(&all, &[], &s), Some(PauseReason::Battery));
        let no_battery = RunSettings {
            pause_on_battery: false,
            ..all
        };
        assert_eq!(
            pause_reason(&no_battery, &[], &s),
            Some(PauseReason::Locked)
        );
        let only_presenting = RunSettings {
            pause_when_locked: false,
            ..no_battery
        };
        assert_eq!(
            pause_reason(&only_presenting, &[], &s),
            Some(PauseReason::Presenting)
        );
        let none = RunSettings {
            pause_when_presenting: false,
            ..only_presenting
        };
        assert_eq!(pause_reason(&none, &[], &s), None);
    }

    #[test]
    fn the_default_pauses_are_locked_only() {
        let mut s = snap(0, 600);
        s.on_ac = false;
        s.notification_state = NotifState::Game;
        assert_eq!(pause_reason(&run(), &[], &s), None);
        s.session_locked = true;
        assert_eq!(pause_reason(&run(), &[], &s), Some(PauseReason::Locked));
    }

    #[test]
    fn presenting_means_presentation_full_screen_or_game() {
        let p = RunSettings {
            pause_when_presenting: true,
            ..run()
        };
        for (state, paused) in [
            (NotifState::Presentation, true),
            (NotifState::Busy, true),
            (NotifState::Game, true),
            (NotifState::QuietTime, false),
            (NotifState::Normal, false),
        ] {
            let mut s = snap(0, 600);
            s.notification_state = state;
            assert_eq!(pause_reason(&p, &[], &s).is_some(), paused, "{state:?}");
        }
    }

    #[test]
    fn a_blackout_pauses_inside_its_window_and_says_until_when() {
        let lunch = Blackout {
            days: EVERY_DAY,
            from: 750,
            to: 810,
            enabled: true,
        };
        assert_eq!(
            pause_reason(&run(), std::slice::from_ref(&lunch), &snap(2, 760)),
            Some(PauseReason::Blackout { until: 810 })
        );
        assert_eq!(
            pause_reason(&run(), std::slice::from_ref(&lunch), &snap(2, 810)),
            None
        );
        assert_eq!(pause_reason(&run(), &[lunch], &snap(2, 749)), None);
    }

    #[test]
    fn a_blackout_across_midnight_uses_its_start_day() {
        let night = Blackout {
            days: MONDAY_ONLY,
            from: 1320,
            to: 360,
            enabled: true,
        }; // Mon 22:00 to 06:00
        assert!(night.active_until(0, 1380).is_some(), "Monday 23:00");
        assert!(
            night.active_until(1, 300).is_some(),
            "Tuesday 05:00, Monday's window"
        );
        assert!(night.active_until(1, 360).is_none(), "Tuesday 06:00");
        assert!(
            night.active_until(1, 1380).is_none(),
            "Tuesday 23:00, not a Tuesday window"
        );
        assert!(
            night.active_until(0, 300).is_none(),
            "Monday 05:00, Sunday's window is off"
        );
    }

    /// Review Focus 3.
    #[test]
    fn an_empty_or_disabled_blackout_never_applies_and_bad_times_are_clamped() {
        let empty = Blackout {
            days: EVERY_DAY,
            from: 600,
            to: 600,
            enabled: true,
        };
        let off = Blackout {
            days: EVERY_DAY,
            from: 0,
            to: 1439,
            enabled: false,
        };
        assert!(empty.active_until(0, 600).is_none());
        assert!(off.active_until(0, 600).is_none());
        let t = Timetable {
            schedules: vec![at(5_000, Start)],
            blackouts: vec![Blackout {
                days: EVERY_DAY,
                from: 2_000,
                to: 3_000,
                enabled: true,
            }],
        }
        .sanitised();
        assert_eq!(t.schedules[0].at, 1439);
        assert_eq!((t.blackouts[0].from, t.blackouts[0].to), (1439, 1439));
    }

    #[test]
    fn no_pause_is_reported_while_stopped_and_battery_alone_releases_power() {
        let mut s = snap(0, 600);
        s.session_locked = true;
        let mut a = Autopilot::default();
        assert_eq!(a.tick(&run(), &table(vec![]), &s, false).pause, None);
        assert_eq!(
            a.tick(&run(), &table(vec![]), &s, true).pause,
            Some(PauseReason::Locked)
        );
        assert!(!PauseReason::Battery.holds_power());
        assert!(PauseReason::Locked.holds_power());
        assert!(PauseReason::Presenting.holds_power());
        assert!(PauseReason::Blackout { until: 0 }.holds_power());
    }

    #[test]
    fn pause_reasons_serialise_for_home() {
        assert_eq!(
            serde_json::to_string(&PauseReason::Battery).unwrap(),
            r#"{"reason":"battery"}"#
        );
        assert_eq!(
            serde_json::to_string(&PauseReason::Blackout { until: 810 }).unwrap(),
            r#"{"reason":"blackout","until":810}"#
        );
        let r: RunFor = serde_json::from_str(r#"{"kind":"until","at":1080}"#).unwrap();
        assert_eq!(r, RunFor::Until { at: 1080 });
    }
}
