use gpui::TestAppContext;

use super::*;
use crate::live_ui_tests::{SqliteFixture, sqlite_fixture};

const KEYED: [&str; 4] = [
    "CREATE TABLE alpha (id INTEGER NOT NULL PRIMARY KEY, name TEXT)",
    "CREATE TABLE beta (id INTEGER NOT NULL PRIMARY KEY, name TEXT)",
    "INSERT INTO alpha VALUES (1, 'one'), (2, 'two'), (3, 'three')",
    "INSERT INTO beta VALUES (2, 'two'), (3, 'changed'), (4, 'four')",
];

fn connect<'a>(
    cx: &'a mut TestAppContext,
    statements: &[&str],
) -> (SqliteFixture, Entity<GridView>, &'a mut gpui::VisualTestContext) {
    let fixture = sqlite_fixture(statements);
    let (view, cx) = crate::ui_tests::open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    (fixture, view, cx)
}

fn compare(view: &Entity<GridView>, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.search.data_compare.source = Some(bookie_gpui::shell::table_info("alpha"));
            view.start_data_compare(bookie_gpui::shell::table_info("beta"), cx);
        })
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

#[gpui::test]
fn data_compare_counts_differences_and_copies_a_sync_script(cx: &mut TestAppContext) {
    let (_fixture, view, cx) = connect(cx, &KEYED);
    view.read_with(cx, |view, _| {
        assert!(
            view.palette_actions()
                .iter()
                .any(|action| action.id == "compare-table-data" && action.enabled),
            "{}",
            view.status
        );
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_data_compare_picker(window, cx)));
    view.read_with(cx, |view, cx| {
        let titles = view
            .picker_items(cx)
            .into_iter()
            .map(|item| item.title)
            .collect::<Vec<_>>();
        assert_eq!(titles, ["beta"]);
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_picker(window, cx)));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.debug_bounds("copy-data-compare-script").is_some());
    view.read_with(cx, |view, _| {
        let Some(Ok(result)) = &view.search.data_compare.outcome else {
            panic!("no compare result: {}", view.status);
        };
        assert_eq!(
            (result.counts.insert, result.counts.delete, result.counts.update),
            (1, 1, 1)
        );
        assert_eq!(result.counts.same, 1);
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.search.data_compare.include_deletes = true;
            view.copy_data_compare_script(cx);
        })
    });
    let script = cx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap_or_default();
    for statement in ["INSERT", "UPDATE", "DELETE"] {
        assert!(script.contains(statement), "{script}");
    }
}

#[gpui::test]
fn data_compare_refuses_tables_without_a_primary_key(cx: &mut TestAppContext) {
    let (_fixture, view, cx) = connect(
        cx,
        &[
            "CREATE TABLE alpha (id INTEGER, name TEXT)",
            "CREATE TABLE beta (id INTEGER, name TEXT)",
        ],
    );
    compare(&view, cx);
    view.read_with(cx, |view, _| {
        assert!(
            matches!(view.search.data_compare.outcome, Some(Err(_))),
            "{}",
            view.status
        );
        assert!(view.search.data_compare.open);
    });
}

#[gpui::test]
fn data_compare_ignores_a_stale_reply(cx: &mut TestAppContext) {
    let (_fixture, view, cx) = connect(cx, &KEYED);
    compare(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let Some(session) = view.live.clone() else {
                panic!("not connected");
            };
            let stale = view.search.data_compare.request.generation - 1;
            view.finish_data_compare(&session, stale, Err("late".into()), cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert!(
            matches!(view.search.data_compare.outcome, Some(Ok(_))),
            "{}",
            view.status
        );
    });
}

#[gpui::test]
fn escape_closes_data_compare(cx: &mut TestAppContext) {
    let (_fixture, view, cx) = connect(cx, &KEYED);
    compare(&view, cx);
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert!(!view.search.data_compare.open);
        assert!(view.search.data_compare.outcome.is_none());
    });
}

#[test]
fn the_no_key_refusal_says_a_primary_key_is_needed() {
    let message = failure_message(&DataCompareError::NoKey);
    assert!(message.contains("primary key"), "{message}");
}
