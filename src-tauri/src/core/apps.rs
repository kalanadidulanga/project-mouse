//! "Keep awake while these apps run" (spec 006 FR-026): the plain-words face of one process rule.

use crate::core::modes::WakeMode;
use crate::core::rule::{Condition, Profile, Rule};

pub const APPS_RULE_ID: &str = "apps";

/// The names in the apps rule, in the order the user added them.
pub fn apps(profile: &Profile) -> Vec<String> {
    profile
        .rules
        .iter()
        .find(|r| r.id == APPS_RULE_ID)
        .and_then(|r| match r.conditions.first() {
            Some(Condition::ProcessRunning(n)) => Some(n.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Replace the list: names trimmed, empty ones dropped, and duplicates in any case removed. The
/// rule exists only while the list is not empty. Returns the list as stored.
pub fn set_apps(profile: &mut Profile, names: Vec<String>) -> Vec<String> {
    let mut list: Vec<String> = Vec::new();
    for n in names {
        let n = n.trim().to_string();
        if !n.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
            list.push(n);
        }
    }
    profile.rules.retain(|r| r.id != APPS_RULE_ID);
    if !list.is_empty() {
        profile.rules.push(Rule {
            id: APPS_RULE_ID.into(),
            name: "Keep awake while these apps run".into(),
            enabled: true,
            conditions: vec![Condition::ProcessRunning(list.clone())],
            mode: WakeMode::KeepRunning,
        });
    }
    list
}

/// The first listed app that is running now, for Home's "Stopped, but msbuild.exe is keeping the
/// PC awake".
pub fn first_running(profile: &Profile, running: &[String]) -> Option<String> {
    apps(profile)
        .into_iter()
        .find(|a| running.iter().any(|r| r.eq_ignore_ascii_case(a)))
}

/// A rule whose only condition is a process list: what config v4 folds into the apps list
/// (FR-031). The apps rule itself is not one.
pub fn process_only(rule: &Rule) -> Option<&[String]> {
    match rule.conditions.as_slice() {
        [Condition::ProcessRunning(n)] if rule.id != APPS_RULE_ID => Some(n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn set_apps_trims_dedupes_and_keeps_the_users_order() {
        let mut p = Profile::new("default", "Default");
        let list = set_apps(
            &mut p,
            names(&[" msbuild.exe ", "", "chrome.exe", "MSBuild.exe"]),
        );
        assert_eq!(list, names(&["msbuild.exe", "chrome.exe"]));
        assert_eq!(apps(&p), list);
        let rule = p.rules.iter().find(|r| r.id == APPS_RULE_ID).unwrap();
        assert!(rule.enabled);
        assert_eq!(rule.mode, WakeMode::KeepRunning);
        assert_eq!(rule.conditions, vec![Condition::ProcessRunning(list)]);
    }

    #[test]
    fn an_empty_list_removes_the_rule() {
        let mut p = Profile::new("default", "Default");
        set_apps(&mut p, names(&["a.exe"]));
        set_apps(&mut p, Vec::new());
        assert!(p.rules.is_empty());
        assert!(apps(&p).is_empty());
    }

    #[test]
    fn first_running_names_the_first_listed_app_that_is_running() {
        let mut p = Profile::new("default", "Default");
        set_apps(&mut p, names(&["msbuild.exe", "chrome.exe"]));
        assert_eq!(
            first_running(&p, &names(&["Chrome.exe", "MSBUILD.EXE"])),
            Some("msbuild.exe".into())
        );
        assert_eq!(first_running(&p, &names(&["notepad.exe"])), None);
    }

    #[test]
    fn process_only_finds_old_style_app_rules_but_not_the_apps_rule() {
        let r = |id: &str, c: Vec<Condition>| Rule {
            id: id.into(),
            name: id.into(),
            enabled: true,
            conditions: c,
            mode: WakeMode::KeepRunning,
        };
        let old = r("x", vec![Condition::ProcessRunning(names(&["a.exe"]))]);
        let mixed = r(
            "y",
            vec![
                Condition::ProcessRunning(names(&["a.exe"])),
                Condition::OnACPower,
            ],
        );
        let ours = r(
            APPS_RULE_ID,
            vec![Condition::ProcessRunning(names(&["a.exe"]))],
        );
        assert_eq!(process_only(&old), Some(&names(&["a.exe"])[..]));
        assert_eq!(process_only(&mixed), None);
        assert_eq!(process_only(&ours), None);
    }
}
