//! Import a Move Mouse `Settings.xml` → our rules (MOVE-MOUSE.md §7). The report is part of the
//! feature: state what was imported, approximated, and dropped. Move Mouse's cursor action becomes Home's
//! movement (spec 005). Start now does what Move Mouse did, plus the power request it never
//! had.

use roxmltree::{Document, Node};
use std::path::{Path, PathBuf};

use crate::core::input_engine::InputSettings;
use crate::core::modes::WakeMode;
use crate::core::motion::Motion;
use crate::core::rule::{Condition, Profile, Rule};

pub struct Imported {
    pub profile: Profile,
    /// Move Mouse's cursor action as Home's movement, or `None` if it had no enabled one, in
    /// which case the current movement settings are left alone.
    pub input: Option<InputSettings>,
    pub report: Vec<String>,
}

/// Where Move Mouse keeps `Settings.xml`: the GitHub/portable build, then the Store build
/// (MOVE-MOUSE.md §7). The caller supplies `%APPDATA%` and `%LOCALAPPDATA%`.
pub fn default_paths(appdata: Option<&Path>, local_appdata: Option<&Path>) -> Vec<PathBuf> {
    let tail = ["Ellanet", "Move Mouse", "Settings.xml"];
    let mut out = Vec::new();
    if let Some(a) = appdata {
        out.push(tail.iter().fold(a.to_path_buf(), |p, s| p.join(s)));
    }
    if let Some(l) = local_appdata {
        let store = l
            .join("Packages")
            .join("1258EllAbi.MoveMouse_hjfwaxvfbwh7t")
            .join("LocalCache")
            .join("Roaming");
        out.push(tail.iter().fold(store, |p, s| p.join(s)));
    }
    out
}

/// Move Mouse's `Direction` → the nearest closed motion here, and whether that is exact.
fn motion_of(direction: &str) -> (Motion, bool) {
    match direction {
        "Square" => (Motion::Square, true),
        "None" => (Motion::Virtual, true), // Stealth
        "LeftAndRight" | "RightAndLeft" => (Motion::Line, true),
        _ => (Motion::Square, false),
    }
}

fn child_text<'a>(node: Node<'a, 'a>, name: &str) -> Option<String> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
        .and_then(|c| c.text())
        .map(|t| t.trim().to_string())
}

fn child_flag(node: Node, name: &str) -> bool {
    child_text(node, name).is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn desc_flag(root: Node, name: &str) -> Option<bool> {
    root.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .and_then(|n| n.text())
        .map(|t| t.trim().eq_ignore_ascii_case("true"))
}

/// `xs:duration` hours+minutes → total minutes (Move Mouse uses e.g. `PT18H`, `PT9H30M`).
fn xs_duration_minutes(s: &str) -> Option<u32> {
    let body = s.trim().strip_prefix("PT")?;
    let mut total = 0u32;
    let mut num = String::new();
    for c in body.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            let v: u32 = num.parse().ok()?;
            num.clear();
            match c {
                'H' => total += v * 60,
                'M' => total += v,
                'S' => {}
                _ => return None,
            }
        }
    }
    Some(total)
}

fn days_of(node: Node) -> [bool; 7] {
    const NAMES: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let mut days = [false; 7];
    for (i, n) in NAMES.iter().enumerate() {
        days[i] = child_flag(node, n);
    }
    days
}

pub fn import(xml: &str) -> Result<Imported, String> {
    let doc = Document::parse(xml).map_err(|e| format!("invalid XML: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "Settings" {
        return Err("not a Move Mouse Settings.xml (root is not <Settings>)".into());
    }

    let mut report = Vec::new();
    let mut conditions = Vec::new();

    if desc_flag(root, "PauseOnBattery").unwrap_or(false) {
        conditions.push(Condition::OnACPower);
        report.push("Pause on battery → hold only while on AC power.".into());
    }
    match desc_flag(root, "ActiveWhenLocked") {
        Some(false) | None => {
            conditions.push(Condition::SessionUnlocked);
            report.push("Continue when locked = off → hold only while unlocked.".into());
        }
        Some(true) => report.push("Continue when locked = on → holds through a lock.".into()),
    }

    // Blackouts → windows we must NOT hold in.
    for bo in root
        .descendants()
        .filter(|n| n.tag_name().name() == "Blackout")
    {
        let (Some(start), Some(dur)) = (
            child_text(bo, "Time")
                .as_deref()
                .and_then(xs_duration_minutes),
            child_text(bo, "Duration")
                .as_deref()
                .and_then(xs_duration_minutes),
        ) else {
            report.push("Skipped a blackout with an unparseable Time/Duration.".into());
            continue;
        };
        let from = (start % 1440) as u16;
        let to = ((start + dur) % 1440) as u16;
        conditions.push(Condition::Not(Box::new(Condition::TimeWindow {
            days: days_of(bo),
            from,
            to,
        })));
        report.push(format!(
            "Blackout {from}..{to} (min-of-day) → do not hold during it."
        ));
    }

    // Things we deliberately don't auto-translate, report them rather than guess.
    let n_actions = root
        .descendants()
        .filter(|n| n.tag_name().name().ends_with("Action"))
        .count();
    // The first enabled cursor action becomes Home's movement (spec 005).
    let cursor = root.descendants().find(|n| {
        n.tag_name().name() == "MoveMouseCursorAction"
            && child_text(*n, "IsEnabled").is_none_or(|v| v.eq_ignore_ascii_case("true"))
    });
    let input = cursor.map(|a| {
        let direction = child_text(a, "Direction").unwrap_or_else(|| "Square".into());
        let (motion, exact) = motion_of(&direction);
        let distance_px = child_text(a, "Distance")
            .and_then(|d| d.parse().ok())
            .unwrap_or(10);
        let interval_secs = child_text(root, "LowerInterval")
            .and_then(|t| t.parse().ok())
            .unwrap_or(30);
        report.push(format!(
            "Mouse movement → Home: {direction}, {distance_px} px, after {interval_secs} s with no \
             input. Press Start to use it."
        ));
        if !exact {
            report.push(format!(
                "'{direction}' has no exact match here, so it became a small square."
            ));
        }
        if desc_flag(root, "RandomInterval").unwrap_or(false) {
            report.push(
                "A random interval was not carried over. Settings → Vary does the same job.".into(),
            );
        }
        InputSettings {
            interval_secs,
            key: 0,
            motion,
            distance_px,
            vary_pct: 0,
        }
    });
    if input.is_none() {
        report.push(
            "No enabled cursor action found, so Home's movement settings are unchanged.".into(),
        );
    }
    let others = n_actions - usize::from(cursor.is_some());
    if others > 0 {
        report.push(format!(
            "{others} other Move Mouse action(s) not imported, click, scroll, keys and commands \
             have no equivalent here."
        ));
    }
    if root
        .descendants()
        .any(|n| n.tag_name().name() == "SimpleSchedule")
        || root
            .descendants()
            .any(|n| n.tag_name().name() == "AdvancedSchedule")
    {
        report.push(
            "Schedules were not auto-mapped (Move Mouse uses Start/Stop events), recreate the \
             window with a weekly schedule rule if you need it."
                .into(),
        );
    }

    let mut profile = Profile::new("imported", "Imported from Move Mouse");
    profile.rules.push(Rule {
        id: "imported-mm".into(),
        name: "Imported from Move Mouse".into(),
        enabled: false, // disabled by default (UI-UX §3), the user turns it on after reviewing
        conditions,
        mode: WakeMode::KeepRunning,
    });

    Ok(Imported {
        profile,
        input,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::input_engine::InputSettings;
    use crate::core::motion::Motion;
    use std::path::Path;

    /// Kalana's own Settings.xml, trimmed to what matters (spec 005 Input).
    const KALANA: &str = r#"<Settings>
      <Actions><MoveMouseCursorAction>
        <IsEnabled>true</IsEnabled><Direction>Square</Direction><Distance>10</Distance>
      </MoveMouseCursorAction></Actions>
      <LowerInterval>200</LowerInterval><UpperInterval>200</UpperInterval>
    </Settings>"#;

    #[test]
    fn rejects_non_movemouse_xml() {
        assert!(import("<Other/>").is_err());
        assert!(import("not xml").is_err());
    }

    #[test]
    fn maps_battery_and_locked_to_conditions() {
        let xml = r#"<Settings>
            <PauseOnBattery>true</PauseOnBattery>
            <ActiveWhenLocked>false</ActiveWhenLocked>
        </Settings>"#;
        let r = import(xml).unwrap();
        let conds = &r.profile.rules[0].conditions;
        assert!(conds.contains(&Condition::OnACPower));
        assert!(conds.contains(&Condition::SessionUnlocked));
        assert_eq!(r.profile.rules[0].mode, WakeMode::KeepRunning);
        assert!(!r.profile.rules[0].enabled); // disabled until reviewed
    }

    #[test]
    fn blackout_becomes_negated_window() {
        // 18:00 for 2h → 18:00..20:00 blackout on weekdays.
        let xml = r#"<Settings><Blackouts><Blackout>
            <Time>PT18H</Time><Duration>PT2H</Duration>
            <Monday>true</Monday><Tuesday>true</Tuesday><Wednesday>true</Wednesday>
            <Thursday>true</Thursday><Friday>true</Friday>
            <Saturday>false</Saturday><Sunday>false</Sunday>
        </Blackout></Blackouts></Settings>"#;
        let r = import(xml).unwrap();
        let has_neg = r.profile.rules[0].conditions.iter().any(|c| {
            matches!(c, Condition::Not(inner)
                if matches!(**inner, Condition::TimeWindow { from, to, .. } if from == 18 * 60 && to == 20 * 60))
        });
        assert!(has_neg, "expected Not(TimeWindow 1080..1200)");
    }

    #[test]
    fn xs_duration_parses_hours_and_minutes() {
        assert_eq!(xs_duration_minutes("PT18H"), Some(1080));
        assert_eq!(xs_duration_minutes("PT9H30M"), Some(570));
        assert_eq!(xs_duration_minutes("PT45M"), Some(45));
        assert_eq!(xs_duration_minutes("PT14H"), Some(840));
    }

    #[test]
    fn the_cursor_action_becomes_homes_movement() {
        let r = import(KALANA).unwrap();
        assert_eq!(
            r.input,
            Some(InputSettings {
                interval_secs: 200,
                key: 0,
                motion: Motion::Square,
                distance_px: 10,
                vary_pct: 0
            })
        );
        assert!(
            !r.report.iter().any(|l| l.contains("not imported")),
            "{:?}",
            r.report
        );
    }

    #[test]
    fn stealth_becomes_invisible() {
        let xml = KALANA.replace(
            "<Direction>Square</Direction>",
            "<Direction>None</Direction>",
        );
        assert_eq!(import(&xml).unwrap().input.unwrap().motion, Motion::Virtual);
    }

    #[test]
    fn an_unmatched_direction_is_approximated_and_said_so() {
        let xml = KALANA.replace(
            "<Direction>Square</Direction>",
            "<Direction>NorthEast</Direction>",
        );
        let r = import(&xml).unwrap();
        assert_eq!(r.input.unwrap().motion, Motion::Square);
        assert!(
            r.report.iter().any(|l| l.contains("no exact match")),
            "{:?}",
            r.report
        );
    }

    #[test]
    fn a_disabled_cursor_action_is_not_used() {
        let xml = KALANA.replace(
            "<IsEnabled>true</IsEnabled>",
            "<IsEnabled>false</IsEnabled>",
        );
        assert_eq!(import(&xml).unwrap().input, None);
    }

    #[test]
    fn no_cursor_action_leaves_movement_alone() {
        let r = import("<Settings/>").unwrap();
        assert_eq!(r.input, None);
        assert!(
            r.report.iter().any(|l| l.contains("unchanged")),
            "{:?}",
            r.report
        );
    }

    #[test]
    fn reports_dropped_actions() {
        let xml = r#"<Settings><Actions><MoveMouseCursorAction/><ClickMouseAction/></Actions></Settings>"#;
        let r = import(xml).unwrap();
        assert!(r.input.is_some());
        assert!(
            r.report
                .iter()
                .any(|l| l.contains("1 other Move Mouse action(s) not imported")),
            "{:?}",
            r.report
        );
    }

    #[test]
    fn looks_in_the_portable_place_then_the_store_place() {
        let p = default_paths(Some(Path::new("C:/R")), Some(Path::new("C:/L")));
        assert_eq!(p[0], Path::new("C:/R/Ellanet/Move Mouse/Settings.xml"));
        assert_eq!(
            p[1],
            Path::new("C:/L/Packages/1258EllAbi.MoveMouse_hjfwaxvfbwh7t/LocalCache/Roaming/Ellanet/Move Mouse/Settings.xml")
        );
        assert!(default_paths(None, None).is_empty());
    }
}
