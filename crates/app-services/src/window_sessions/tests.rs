use super::*;
use uuid::Uuid;

#[test]
fn opening_a_second_working_session_keeps_both_in_order() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut sessions = WindowSessions::default();
    assert!(sessions.open_working(first));
    assert!(sessions.open_working(second));
    assert!(!sessions.open_working(first));
    assert_eq!(sessions.working_order(), &[first, second]);
    assert_eq!(sessions.front(), Some(first));
    assert!(sessions.names_tabs_with_connection());
}

#[test]
fn focusing_a_working_session_moves_front_without_a_tab() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut sessions = WindowSessions::default();
    sessions.open_working(first);
    sessions.open_working(second);
    assert!(sessions.focus_working(second));
    assert_eq!(sessions.front(), Some(second));
    assert!(!sessions.focus_working(Uuid::new_v4()));
    assert_eq!(sessions.front(), Some(second));
}

#[test]
fn selecting_a_tab_moves_the_front_session_to_that_tabs_connection() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let tab_a = Uuid::new_v4();
    let tab_b = Uuid::new_v4();
    let mut sessions = WindowSessions::default();
    sessions.open_working(first);
    sessions.open_working(second);
    sessions.attach_tab(tab_a, first).unwrap();
    sessions.attach_tab(tab_b, second).unwrap();
    assert_eq!(sessions.select_tab(tab_b), Some(second));
    assert_eq!(sessions.front(), Some(second));
    assert_eq!(sessions.select_tab(tab_a), Some(first));
    assert_eq!(sessions.front(), Some(first));
}

#[test]
fn a_tab_cannot_move_to_another_connection() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let tab = Uuid::new_v4();
    let mut sessions = WindowSessions::default();
    sessions.open_working(first);
    sessions.open_working(second);
    sessions.attach_tab(tab, first).unwrap();
    assert_eq!(
        sessions.attach_tab(tab, second),
        Err(AttachTabError::TabBelongsToOtherConnection)
    );
    assert_eq!(sessions.connection_for_tab(tab), Some(first));
}

#[test]
fn disconnecting_one_session_closes_only_its_tabs_and_keeps_the_other() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let tab_a = Uuid::new_v4();
    let tab_b = Uuid::new_v4();
    let tab_c = Uuid::new_v4();
    let mut sessions = WindowSessions::default();
    sessions.open_working(first);
    sessions.open_working(second);
    sessions.attach_tab(tab_a, first).unwrap();
    sessions.attach_tab(tab_b, first).unwrap();
    sessions.attach_tab(tab_c, second).unwrap();
    sessions.select_tab(tab_a);
    let closed = sessions.disconnect(first);
    assert_eq!(closed.len(), 2);
    assert!(closed.contains(&tab_a));
    assert!(closed.contains(&tab_b));
    assert_eq!(sessions.working_order(), &[second]);
    assert_eq!(sessions.front(), Some(second));
    assert_eq!(sessions.connection_for_tab(tab_c), Some(second));
    assert!(sessions.connection_for_tab(tab_a).is_none());
}

#[test]
fn mcp_sessions_stay_outside_the_working_list_and_restore_keys() {
    let working = Uuid::new_v4();
    let mcp = Uuid::new_v4();
    let mut sessions = WindowSessions::default();
    sessions.open_working(working);
    assert!(sessions.open_mcp(mcp));
    assert!(!sessions.open_mcp(mcp));
    assert!(!sessions.open_mcp(working));
    assert!(sessions.is_working(working));
    assert!(sessions.is_mcp(mcp));
    assert!(!sessions.is_working(mcp));
    assert_eq!(sessions.restore_keys(), vec![working]);
    assert!(!sessions.names_tabs_with_connection());
}

#[test]
fn a_legacy_single_connection_record_restores_as_one_window_session() {
    let id = Uuid::new_v4();
    let record = OpenSessionsRecord::from_legacy_last_connection(Some(id));
    assert_eq!(record.version, OPEN_SESSIONS_VERSION);
    assert_eq!(record.windows, vec![vec![id]]);
    assert!(OpenSessionsRecord::from_legacy_last_connection(None).windows.is_empty());
}

#[test]
fn multi_key_restore_keeps_window_groups_and_drops_unknown_or_duplicate_keys() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();
    let missing = Uuid::new_v4();
    let record = OpenSessionsRecord::from_windows([vec![a, b, a], vec![c, missing], Vec::new()]);
    assert_eq!(record.windows, vec![vec![a, b], vec![c, missing]]);
    assert_eq!(restore_window_keys(&record.windows[0], &[a, c]), vec![a]);
    assert_eq!(restore_window_keys(&record.windows[1], &[a, c]), vec![c]);
}

#[test]
fn large_restore_keeps_first_occurrences_in_window_order() {
    let keys = (1..=10_000).map(Uuid::from_u128).collect::<Vec<_>>();
    let record = OpenSessionsRecord::from_windows([keys.clone(), keys.clone()]);
    assert_eq!(record.windows.len(), 1);
    assert_eq!(record.windows[0], keys);
}

#[test]
fn restoring_available_keys_preserves_key_order_and_duplicate_behavior() {
    let first = Uuid::from_u128(1);
    let second = Uuid::from_u128(2);
    assert_eq!(
        restore_window_keys(&[first, second, first], &[second, first]),
        vec![first, second, first]
    );
}

#[test]
fn an_unsupported_open_sessions_version_is_refused_without_restoring_keys() {
    let id = Uuid::new_v4();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version": OPEN_SESSIONS_VERSION + 1,
        "windows": [[id]],
    }))
    .unwrap();
    assert_eq!(
        parse_open_sessions(&bytes),
        OpenSessionsLoad::UnsupportedVersion(OPEN_SESSIONS_VERSION + 1)
    );
}

#[test]
fn a_current_open_sessions_record_round_trips_and_corrupt_bytes_load_empty() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let record = OpenSessionsRecord::from_windows([vec![first, second]]);
    let bytes = serde_json::to_vec(&record).unwrap();
    assert_eq!(parse_open_sessions(&bytes), OpenSessionsLoad::Restored(record));
    assert_eq!(parse_open_sessions(b"{ not json"), OpenSessionsLoad::Empty);
    assert_eq!(
        parse_open_sessions(br#"{"version":1,"windows":[]}"#),
        OpenSessionsLoad::Empty
    );
}

#[test]
fn window_close_asks_once_across_sessions_and_cancel_clears_the_prompt_only() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let tab_a = Uuid::new_v4();
    let tab_b = Uuid::new_v4();
    let mut prompt = WindowSavePrompt::default();
    prompt.mark_dirty(tab_a, first);
    prompt.mark_dirty(tab_b, second);
    assert!(prompt.needs_prompt());
    assert_eq!(prompt.dirty_count(), 2);
    let mut connections = prompt.dirty_connections();
    connections.sort();
    let mut expected = vec![first, second];
    expected.sort();
    assert_eq!(connections, expected);
    prompt.cancel();
    assert!(!prompt.needs_prompt());
    assert_eq!(prompt.dirty_count(), 0);
}

#[test]
fn clearing_one_connection_leaves_other_sessions_dirty_tabs() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let tab_a = Uuid::new_v4();
    let tab_b = Uuid::new_v4();
    let mut prompt = WindowSavePrompt::default();
    prompt.mark_dirty(tab_a, first);
    prompt.mark_dirty(tab_b, second);
    prompt.clear_connection(first);
    assert_eq!(prompt.dirty_count(), 1);
    assert_eq!(prompt.dirty_connections(), vec![second]);
}
