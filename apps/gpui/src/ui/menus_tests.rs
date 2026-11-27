use super::*;

#[test]
fn destructive_commands_sit_last_in_their_menu() {
    for groups in MENUS {
        let ids: Vec<&str> = groups.iter().flat_map(|group| group.iter().copied()).collect();
        let first = ids.iter().position(|id| DESTRUCTIVE.contains(id));
        let Some(first) = first else { continue };
        assert!(
            ids[first..].iter().all(|id| DESTRUCTIVE.contains(id)),
            "{ids:?} puts a safe command after a destructive one"
        );
        let last = groups.last().copied().unwrap_or_default();
        assert!(
            last.iter().all(|id| DESTRUCTIVE.contains(id)),
            "{ids:?} mixes the destructive group"
        );
    }
}

#[test]
fn a_disabled_entry_stays_disabled() {
    let entry = |id, enabled| Entry {
        id,
        label: id.to_string(),
        enabled,
        shortcut: None,
        checked: false,
    };
    let mut groups = vec![vec![entry("compare", true), entry("insert-row", false)]];
    set_enabled(&mut groups, "compare", false);
    set_enabled(&mut groups, "insert-row", true);
    assert!(groups[0].iter().all(|entry| !entry.enabled));
}
