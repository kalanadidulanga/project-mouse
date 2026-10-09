//! The profile collection. Pure, no Win32, no `AppHandle`, no config I/O (constitution IV/V).
//! The engine holds exactly one `Profile`; this is the set it is chosen from.

use crate::core::rule::Profile;

/// Insert `profile`, or replace the existing entry with the same id **in place**.
pub fn upsert(list: &mut Vec<Profile>, profile: Profile) {
    match list.iter_mut().find(|p| p.id == profile.id) {
        Some(existing) => *existing = profile,
        None => list.push(profile),
    }
}

/// The profile with `id`, if present.
pub fn find<'a>(list: &'a [Profile], id: &str) -> Option<&'a Profile> {
    list.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::modes::WakeMode;
    use crate::core::rule::{Condition, Rule};

    fn profile(id: &str, rules: usize) -> Profile {
        let mut p = Profile::new(id, id);
        for i in 0..rules {
            p.rules.push(Rule {
                id: format!("{id}-r{i}"),
                name: "r".into(),
                enabled: true,
                conditions: vec![Condition::OnACPower],
                mode: WakeMode::KeepRunning,
            });
        }
        p
    }

    /// The regression this whole module exists for: saving the active profile must not destroy
    /// the others. `persist_current` used to write `profiles: vec![active]`.
    #[test]
    fn upsert_replaces_by_id_and_keeps_the_others() {
        let mut list = vec![profile("work", 1), profile("home", 2), profile("kiosk", 3)];
        upsert(&mut list, profile("home", 9));

        assert_eq!(list.len(), 3, "the other profiles survived");
        assert_eq!(list[1].id, "home", "replaced in place, order preserved");
        assert_eq!(list[1].rules.len(), 9);
        assert_eq!(list[0].rules.len(), 1, "work untouched");
        assert_eq!(list[2].rules.len(), 3, "kiosk untouched");
    }

    #[test]
    fn upsert_appends_an_unknown_id() {
        let mut list = vec![profile("work", 1)];
        upsert(&mut list, profile("new", 0));
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].id, "new");
    }

    #[test]
    fn find_returns_none_for_an_unknown_id() {
        let list = vec![profile("a", 0)];
        assert_eq!(find(&list, "a").map(|p| p.id.as_str()), Some("a"));
        assert!(find(&list, "b").is_none());
    }
}
