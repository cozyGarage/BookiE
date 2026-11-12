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

fn stored_windows(windows: &[&[Uuid]]) -> Vec<u8> {
    serde_json::to_vec(&OpenSessionsRecord::from_windows(
        windows.iter().map(|keys| keys.to_vec()),
    ))
    .unwrap()
}

#[test]
fn startup_restores_every_key_of_the_window_and_opens_the_front_session_last() {
    let [first, front, third] = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let stored = stored_windows(&[&[first, front, third]]);
    assert_eq!(
        startup_restore_keys(Some(&stored), Some(front), &[first, front, third]),
        vec![first, third, front]
    );
}

#[test]
fn startup_skips_keys_whose_saved_connection_is_gone() {
    let [first, front] = [Uuid::new_v4(), Uuid::new_v4()];
    let stored = stored_windows(&[&[first, front]]);
    assert_eq!(startup_restore_keys(Some(&stored), Some(front), &[front]), vec![front]);
    assert!(startup_restore_keys(Some(&stored), Some(front), &[]).is_empty());
}

#[test]
fn startup_restores_the_window_that_holds_the_last_connection() {
    let [other, first, front] = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let stored = stored_windows(&[&[other], &[first, front]]);
    assert_eq!(
        startup_restore_keys(Some(&stored), Some(front), &[other, first, front]),
        vec![first, front]
    );
}

#[test]
fn startup_falls_back_to_the_single_last_connection_without_a_usable_record() {
    let [stale, front] = [Uuid::new_v4(), Uuid::new_v4()];
    assert_eq!(startup_restore_keys(None, Some(front), &[front]), vec![front]);
    assert_eq!(
        startup_restore_keys(Some(b"{ not json"), Some(front), &[front]),
        vec![front]
    );
    let unsupported = br#"{"version":99,"windows":[]}"#;
    assert_eq!(
        startup_restore_keys(Some(unsupported), Some(front), &[front]),
        vec![front]
    );
    let stored = stored_windows(&[&[stale]]);
    assert_eq!(
        startup_restore_keys(Some(&stored), Some(front), &[stale, front]),
        vec![front]
    );
}

#[test]
fn startup_restores_nothing_after_the_last_connection_was_cleared() {
    let key = Uuid::new_v4();
    let stored = stored_windows(&[&[key]]);
    assert!(startup_restore_keys(Some(&stored), None, &[key]).is_empty());
}

fn two_sessions_with_tabs() -> (WindowSessions, [Uuid; 2], [Uuid; 2]) {
    let connections = [Uuid::new_v4(), Uuid::new_v4()];
    let tabs = [Uuid::new_v4(), Uuid::new_v4()];
    let mut sessions = WindowSessions::default();
    sessions.open_working(connections[0]);
    sessions.open_working(connections[1]);
    sessions.attach_tab(tabs[0], connections[0]).unwrap();
    sessions.attach_tab(tabs[1], connections[1]).unwrap();
    (sessions, connections, tabs)
}

#[test]
fn a_tab_resolves_to_its_own_connection_even_when_another_session_is_in_front() {
    let (mut sessions, connections, tabs) = two_sessions_with_tabs();
    sessions.focus_working(connections[1]);
    assert_eq!(sessions.resolve_tab_connection(tabs[0]), Ok(connections[0]));
    assert_eq!(sessions.resolve_tab_connection(tabs[1]), Ok(connections[1]));
}

#[test]
fn a_tab_without_a_recorded_connection_is_refused_and_never_falls_back_to_the_front_session() {
    let (sessions, _, _) = two_sessions_with_tabs();
    let stray = Uuid::new_v4();
    assert_eq!(
        sessions.resolve_tab_connection(stray),
        Err(TabConnectionError::NoRecordedConnection { tab_id: stray })
    );
}

#[test]
fn a_tab_whose_session_was_closed_is_refused() {
    let (mut sessions, connections, tabs) = two_sessions_with_tabs();
    sessions.disconnect(connections[0]);
    assert_eq!(
        sessions.resolve_tab_connection(tabs[0]),
        Err(TabConnectionError::NoRecordedConnection { tab_id: tabs[0] })
    );
}

#[test]
fn dirty_tabs_across_sessions_make_one_prompt_naming_each_connection_in_window_order() {
    let (sessions, connections, tabs) = two_sessions_with_tabs();
    let prompt = plan_dirty_tabs(&sessions, &[tabs[1], tabs[0]]).unwrap();
    assert!(prompt.needs_prompt());
    assert_eq!(
        prompt.connections_in_order(sessions.working_order()),
        connections.to_vec()
    );
    assert_eq!(prompt.tabs_for_connection(connections[1]), vec![tabs[1]]);
}

#[test]
fn a_dirty_tab_without_a_connection_blocks_the_whole_prompt_plan() {
    let (sessions, _, tabs) = two_sessions_with_tabs();
    let stray = Uuid::new_v4();
    assert_eq!(
        plan_dirty_tabs(&sessions, &[tabs[0], stray]).map(|_| ()),
        Err(TabConnectionError::NoRecordedConnection { tab_id: stray })
    );
}

#[test]
fn disconnecting_one_session_prompts_only_for_its_own_dirty_tabs() {
    let (sessions, connections, tabs) = two_sessions_with_tabs();
    let prompt = plan_dirty_tabs(&sessions, &[tabs[0], tabs[1]]).unwrap();
    assert_eq!(prompt.tabs_for_connection(connections[0]), vec![tabs[0]]);
    assert!(prompt.tabs_for_connection(Uuid::new_v4()).is_empty());
}

#[test]
fn disconnecting_one_of_two_sessions_scopes_teardown_to_that_sessions_tabs() {
    let (sessions, connections, tabs) = two_sessions_with_tabs();
    assert_eq!(disconnect_scope(&sessions, connections[0], &tabs), vec![tabs[0]]);
    assert_eq!(disconnect_scope(&sessions, connections[1], &tabs), vec![tabs[1]]);
}

#[test]
fn disconnecting_the_only_session_covers_every_tab_in_the_window() {
    let connection = Uuid::new_v4();
    let tab = Uuid::new_v4();
    let unclaimed = Uuid::new_v4();
    let mut sessions = WindowSessions::default();
    sessions.open_working(connection);
    sessions.attach_tab(tab, connection).unwrap();
    assert_eq!(
        disconnect_scope(&sessions, connection, &[tab, unclaimed]),
        vec![tab, unclaimed]
    );
}

#[test]
fn startup_plan_restores_every_other_window_after_the_front_window() {
    let [other, first, front, third] = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let stored = stored_windows(&[&[other], &[first, front], &[third]]);
    let plan = startup_restore_plan(Some(&stored), Some(front), &[other, first, front, third]);
    assert_eq!(plan.first, vec![first, front]);
    assert_eq!(plan.others, vec![vec![other], vec![third]]);
}

#[test]
fn startup_plan_drops_deleted_connections_and_windows_left_empty() {
    let [gone, kept, front] = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let stored = stored_windows(&[&[gone], &[front], &[gone, kept]]);
    let plan = startup_restore_plan(Some(&stored), Some(front), &[kept, front]);
    assert_eq!(plan.first, vec![front]);
    assert_eq!(plan.others, vec![vec![kept]]);
}

#[test]
fn startup_plan_opens_a_connection_in_one_window_only() {
    let [shared, front] = [Uuid::new_v4(), Uuid::new_v4()];
    let stored = format!(r#"{{"version":1,"windows":[["{shared}","{front}"],["{shared}"]]}}"#);
    let plan = startup_restore_plan(Some(stored.as_bytes()), Some(front), &[shared, front]);
    assert_eq!(plan.first, vec![shared, front]);
    assert!(plan.others.is_empty());
}

#[test]
fn startup_plan_ignores_other_windows_when_the_record_is_unusable() {
    let [stale, front] = [Uuid::new_v4(), Uuid::new_v4()];
    let unsupported = br#"{"version":99,"windows":[]}"#;
    let plan = startup_restore_plan(Some(unsupported), Some(front), &[stale, front]);
    assert_eq!(plan.first, vec![front]);
    assert!(plan.others.is_empty());
    let plan = startup_restore_plan(Some(b"{ not json"), Some(front), &[front]);
    assert_eq!(plan.first, vec![front]);
    assert!(plan.others.is_empty());
}

#[test]
fn startup_plan_keeps_a_record_written_before_the_front_was_recorded() {
    let [stale, front] = [Uuid::new_v4(), Uuid::new_v4()];
    let stored = stored_windows(&[&[stale]]);
    let plan = startup_restore_plan(Some(&stored), Some(front), &[stale, front]);
    assert_eq!(plan.first, vec![front]);
    assert_eq!(plan.others, vec![vec![stale]]);
}

#[test]
fn startup_plan_is_empty_after_the_last_connection_was_cleared() {
    let key = Uuid::new_v4();
    let stored = stored_windows(&[&[key]]);
    assert_eq!(
        startup_restore_plan(Some(&stored), None, &[key]),
        StartupRestorePlan::default()
    );
}
