//! Pure condition evaluation. Every branch is testable by hand-building a `Snapshot` — the whole
//! point of the Snapshot boundary (constitution V).

use crate::core::modes::WakeMode;
use crate::core::rule::{Condition, Profile, Rule};
use crate::core::snapshot::Snapshot;

impl Condition {
    pub fn eval(&self, s: &Snapshot) -> bool {
        match self {
            Condition::ProcessRunning(names) => names.iter().any(|n| {
                s.running_processes
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case(n))
            }),
            Condition::TimeWindow { days, from, to } => {
                time_window_holds(*days, *from, *to, s.weekday, s.minutes)
            }
            Condition::ExpiryAt(deadline) => s.epoch_secs < *deadline,
            Condition::OnACPower => s.on_ac,
            Condition::BatteryAbove(p) => s.battery_pct >= *p,
            Condition::SessionUnlocked => !s.session_locked,
            Condition::NotificationStateIn(set) => set.contains(&s.notification_state),
            Condition::ForegroundAppIn(list) => match &s.foreground_exe {
                Some(fg) => list.iter().any(|n| n.eq_ignore_ascii_case(fg)),
                None => false,
            },
            Condition::ForegroundAppNotIn(list) => match &s.foreground_exe {
                Some(fg) => !list.iter().any(|n| n.eq_ignore_ascii_case(fg)),
                None => true,
            },
            Condition::Not(c) => !c.eval(s),
            Condition::AnyOf(cs) => cs.iter().any(|c| c.eval(s)),
            Condition::AllOf(cs) => cs.iter().all(|c| c.eval(s)),
        }
    }
}

fn time_window_holds(days: [bool; 7], from: u16, to: u16, weekday: u8, minutes: u16) -> bool {
    let wd = (weekday as usize) % 7;
    let prev = (wd + 6) % 7;
    if from <= to {
        days[wd] && minutes >= from && minutes < to
    } else {
        // Crosses midnight: the day flag is the *start* day, so the [0,to) tail belongs to the
        // previous day's window occurrence.
        (days[wd] && minutes >= from) || (days[prev] && minutes < to)
    }
}

/// A rule contributes iff it is enabled and every condition holds (empty conditions = always).
pub fn rule_holds(rule: &Rule, s: &Snapshot) -> bool {
    rule.enabled && rule.conditions.iter().all(|c| c.eval(s))
}

/// The mode the machine should hold: the maximum over all contributing rules (ARCHITECTURE §5).
pub fn desired_mode(profile: &Profile, s: &Snapshot) -> WakeMode {
    WakeMode::combine(
        profile
            .rules
            .iter()
            .filter(|r| rule_holds(r, s))
            .map(|r| r.mode),
    )
}

/// Seconds until the soonest deadline among enabled rules that carry an `ExpiryAt`.
/// `None` when nothing is counting down. Drives the tray tooltip (FEATURES B3 / 002 T020).
pub fn soonest_expiry_secs(profile: &Profile, s: &Snapshot) -> Option<u64> {
    profile
        .rules
        .iter()
        .filter(|r| r.enabled)
        .flat_map(|r| r.conditions.iter())
        .filter_map(|c| match c {
            // `checked_sub` rather than a subtraction: an elapsed deadline holds nothing, and a
            // wrapped countdown in the tooltip would be a 584-billion-year lie.
            Condition::ExpiryAt(deadline) => deadline.checked_sub(s.epoch_secs).filter(|d| *d > 0),
            _ => None,
        })
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::modes::WakeMode::*;
    use crate::core::rule::{Condition::*, NotifState, Profile, Rule};

    fn snap() -> Snapshot {
        Snapshot::default()
    }

    mod expiry {
        use super::*;

        fn at(secs: u64) -> Snapshot {
            Snapshot {
                epoch_secs: secs,
                ..Snapshot::default()
            }
        }
        fn expiring(id: &str, enabled: bool, deadline: u64) -> Rule {
            Rule {
                id: id.into(),
                name: id.into(),
                enabled,
                conditions: vec![ExpiryAt(deadline)],
                mode: KeepRunning,
            }
        }

        #[test]
        fn nothing_counting_down_is_none() {
            let p = Profile::new("p", "p");
            assert_eq!(soonest_expiry_secs(&p, &at(1_000)), None);
        }

        #[test]
        fn a_disabled_timer_does_not_count_down() {
            let mut p = Profile::new("p", "p");
            p.rules.push(expiring("t", false, 1_300));
            assert_eq!(soonest_expiry_secs(&p, &at(1_000)), None);
        }

        #[test]
        fn reports_seconds_remaining() {
            let mut p = Profile::new("p", "p");
            p.rules.push(expiring("t", true, 1_300));
            assert_eq!(soonest_expiry_secs(&p, &at(1_000)), Some(300));
        }

        #[test]
        fn reports_the_soonest_of_several() {
            let mut p = Profile::new("p", "p");
            p.rules.push(expiring("late", true, 5_000));
            p.rules.push(expiring("soon", true, 1_060));
            assert_eq!(soonest_expiry_secs(&p, &at(1_000)), Some(60));
        }

        /// An elapsed deadline holds nothing, so it must not show a stale or wrapped countdown.
        #[test]
        fn an_elapsed_deadline_is_none_not_zero_and_never_underflows() {
            let mut p = Profile::new("p", "p");
            p.rules.push(expiring("gone", true, 900));
            assert_eq!(soonest_expiry_secs(&p, &at(1_000)), None);
        }

        #[test]
        fn a_rule_without_an_expiry_is_ignored() {
            let mut p = Profile::new("p", "p");
            p.rules.push(Rule {
                id: "plain".into(),
                name: "plain".into(),
                enabled: true,
                conditions: vec![OnACPower],
                mode: KeepRunning,
            });
            assert_eq!(soonest_expiry_secs(&p, &at(1_000)), None);
        }
    }
    fn rule(mode: crate::core::modes::WakeMode, conditions: Vec<Condition>) -> Rule {
        Rule {
            id: "r".into(),
            name: "r".into(),
            enabled: true,
            conditions,
            mode,
        }
    }

    #[test]
    fn process_running_is_case_insensitive() {
        let mut s = snap();
        s.running_processes = vec!["MSBuild.EXE".into()];
        assert!(ProcessRunning(vec!["msbuild.exe".into()]).eval(&s));
        assert!(!ProcessRunning(vec!["chrome.exe".into()]).eval(&s));
    }

    #[test]
    fn time_window_normal() {
        let mut s = snap();
        s.weekday = 2; // Wed
        s.minutes = 10 * 60; // 10:00
        let wd = TimeWindow {
            days: [true, true, true, true, true, false, false],
            from: 8 * 60,
            to: 18 * 60,
        };
        assert!(wd.eval(&s));
        s.minutes = 19 * 60;
        assert!(!wd.eval(&s));
        s.weekday = 5; // Sat — not in days
        s.minutes = 10 * 60;
        assert!(!wd.eval(&s));
    }

    #[test]
    fn time_window_crossing_midnight() {
        // 22:00 Mon -> 06:00 Tue
        let wd = TimeWindow {
            days: [true, false, false, false, false, false, false],
            from: 22 * 60,
            to: 6 * 60,
        };
        let mut s = snap();
        s.weekday = 0; // Mon
        s.minutes = 23 * 60; // 23:00 Mon -> in
        assert!(wd.eval(&s));
        s.weekday = 1; // Tue
        s.minutes = 5 * 60; // 05:00 Tue -> belongs to Mon's window
        assert!(wd.eval(&s));
        s.minutes = 7 * 60; // 07:00 Tue -> out
        assert!(!wd.eval(&s));
    }

    #[test]
    fn expiry_releases_at_deadline() {
        let mut s = snap();
        s.epoch_secs = 100;
        assert!(ExpiryAt(200).eval(&s));
        s.epoch_secs = 200;
        assert!(!ExpiryAt(200).eval(&s));
    }

    #[test]
    fn battery_default_guard_via_composition() {
        // "pause below 20% on battery" == AnyOf[OnACPower, BatteryAbove(20)]
        let guard = AnyOf(vec![OnACPower, BatteryAbove(20)]);
        let mut s = snap();
        s.on_ac = false;
        s.battery_pct = 15;
        assert!(!guard.eval(&s));
        s.battery_pct = 50;
        assert!(guard.eval(&s));
        s.on_ac = true;
        s.battery_pct = 5;
        assert!(guard.eval(&s)); // on AC, low battery is fine
    }

    #[test]
    fn session_and_notification_and_foreground() {
        let mut s = snap();
        s.session_locked = true;
        assert!(!SessionUnlocked.eval(&s));

        s.notification_state = NotifState::Presentation;
        assert!(NotificationStateIn(vec![NotifState::Presentation]).eval(&s));
        assert!(!Not(Box::new(NotificationStateIn(vec![
            NotifState::Presentation
        ])))
        .eval(&s));

        s.foreground_exe = Some("Photoshop.exe".into());
        assert!(ForegroundAppIn(vec!["photoshop.exe".into()]).eval(&s));
        assert!(!ForegroundAppNotIn(vec!["photoshop.exe".into()]).eval(&s));
        s.foreground_exe = None;
        assert!(!ForegroundAppIn(vec!["x.exe".into()]).eval(&s));
        assert!(ForegroundAppNotIn(vec!["x.exe".into()]).eval(&s)); // none focused -> not in list
    }

    #[test]
    fn composition() {
        let mut s = snap();
        s.running_processes = vec!["msbuild.exe".into()];
        s.on_ac = true;
        s.notification_state = NotifState::Normal;
        // weekday/time: make an all-day window
        let allday = TimeWindow {
            days: [true; 7],
            from: 0,
            to: 1440,
        };
        let composed = AllOf(vec![
            ProcessRunning(vec!["msbuild.exe".into()]),
            OnACPower,
            allday,
            Not(Box::new(NotificationStateIn(vec![
                NotifState::Presentation,
            ]))),
        ]);
        assert!(composed.eval(&s));
        s.notification_state = NotifState::Presentation;
        assert!(!composed.eval(&s)); // presenting -> stand down
    }

    #[test]
    fn desired_mode_combines_by_maximum() {
        let mut s = snap();
        s.running_processes = vec!["msbuild.exe".into()];
        let mut p = Profile::new("p", "P");
        p.rules.push(rule(
            KeepRunning,
            vec![ProcessRunning(vec!["msbuild.exe".into()])],
        ));
        p.rules.push(rule(KeepPresenting, vec![])); // unconditional presenting
        assert_eq!(desired_mode(&p, &s), KeepPresenting); // stronger wins

        // disabled strong rule is ignored
        p.rules[1].enabled = false;
        assert_eq!(desired_mode(&p, &s), KeepRunning);

        // no rule matches -> Off
        s.running_processes.clear();
        assert_eq!(desired_mode(&p, &s), Off);
    }
}
