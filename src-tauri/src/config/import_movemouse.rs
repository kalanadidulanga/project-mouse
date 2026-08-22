//! Import a Move Mouse `Settings.xml` → our rules (MOVE-MOUSE.md §7). The report is part of the
//! feature: state what was imported, approximated, and dropped — and default to **power-only**
//! (Keep running, no synthetic input), which is the honest replacement for a Move Mouse jiggle.

use roxmltree::{Document, Node};

use crate::core::modes::WakeMode;
use crate::core::rule::{Condition, Profile, Rule};

pub struct Imported {
    pub profile: Profile,
    /// The honest default: false. Move Mouse's jiggle becomes Keep running (no synthetic input);
    /// the user can turn synthesis on later if they specifically need a session/presence timer.
    pub input_enabled: bool,
    pub report: Vec<String>,
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

    // Move Mouse is 100% synthetic input; its jiggle keeps the session alive. The honest
    // replacement is Keep running (power only), so we default input synthesis OFF.
    report.push(
        "Your Move Mouse jiggle is replaced by 'Keep running' — the machine stays awake with no \
         synthetic input. Turn on input synthesis in Settings only if you need a session or \
         presence timer that keeping awake cannot reset."
            .into(),
    );

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

    // Things we deliberately don't auto-translate — report them rather than guess.
    let n_actions = root
        .descendants()
        .filter(|n| n.tag_name().name().ends_with("Action"))
        .count();
    if n_actions > 0 {
        report.push(format!(
            "{n_actions} Move Mouse action(s) not imported — project-mouse is power-first; add input \
             synthesis manually if you need it."
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
            "Schedules were not auto-mapped (Move Mouse uses Start/Stop events) — recreate the \
             window with a weekly schedule rule if you need it."
                .into(),
        );
    }

    let mut profile = Profile::new("imported", "Imported from Move Mouse");
    profile.rules.push(Rule {
        id: "imported-mm".into(),
        name: "Imported from Move Mouse".into(),
        enabled: false, // disabled by default (UI-UX §3) — the user turns it on after reviewing
        conditions,
        mode: WakeMode::KeepRunning,
    });

    Ok(Imported {
        profile,
        input_enabled: false,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!r.input_enabled); // power-only default
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
    fn reports_dropped_actions() {
        let xml = r#"<Settings><Actions><MoveMouseCursorAction/><ClickMouseAction/></Actions></Settings>"#;
        let r = import(xml).unwrap();
        assert!(r
            .report
            .iter()
            .any(|l| l.contains("action(s) not imported")));
    }
}
