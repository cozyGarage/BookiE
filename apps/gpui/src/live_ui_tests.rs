use std::time::Instant;

use gpui::TestAppContext;

use crate::GridView;
use crate::text_input;
use crate::ui_tests::open;

#[gpui::test]
fn f3_toggles_a_detail_panel_that_follows_the_active_cell(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("down right right right");
    view.read_with(cx, |view, _| assert!(!view.detail_open));
    cx.simulate_keystrokes("f3");
    view.read_with(cx, |view, _| {
        assert!(view.detail_open);
        let (name, detail) = view.detail_target().expect("a cell is active");
        assert_eq!(name, "typed");
        let bookie_gpui::detail::Detail::Hex { lines, total, .. } = detail else {
            panic!("row 1 holds bytes");
        };
        assert_eq!(total, 4);
        assert_eq!(lines[0].hex, "de ad be ef");
    });
    cx.simulate_keystrokes("down");
    view.read_with(cx, |view, _| {
        let (_, detail) = view.detail_target().expect("still active");
        assert!(matches!(detail, bookie_gpui::detail::Detail::Text(text) if text == "2026-11-04"));
    });
    cx.simulate_keystrokes("f3");
    view.read_with(cx, |view, _| assert!(!view.detail_open));
}

#[gpui::test]
fn a_rebound_shortcut_replaces_the_default_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let keymap = bookie_gpui::keymap::Keymap::from_overrides(r#"{"toggle-sidebar":"alt-b"}"#).expect("keymap");
    view.update(cx, |view, _| view.keymap = keymap);
    let before = view.read_with(cx, |view, _| view.sidebar_open);
    cx.simulate_keystrokes("ctrl-b");
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_open, before, "the old chord is free")
    });
    cx.simulate_keystrokes("alt-b");
    view.read_with(cx, |view, _| assert_eq!(view.sidebar_open, !before));
}

#[gpui::test]
fn clicking_another_table_of_a_live_connection_loads_it_into_the_grid(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orders.sqlite"));
    let table = std::sync::Arc::new(LiveTable::load(&origin, "orders", 100).expect("load"));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = GridView::with_source(table.clone(), Instant::now(), window, cx);
        view.attach_live(table.clone());
        view
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.catalog.grid_table, "orders");
        assert_eq!(view.catalog.tables, ["customers", "orders"]);
        assert_eq!(view.source.total(), 6);
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_table_named("customers", window, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.catalog.grid_table, "customers");
        assert_eq!(view.source.total(), 2);
        assert_eq!(view.source.columns().len(), 2);
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Table("customers".into()));
        assert!(view.status.is_empty(), "{}", view.status);
    });
}

#[gpui::test]
fn a_live_connection_runs_typed_sql_into_a_result_tab_and_refuses_writes(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orders.sqlite"));
    let table = std::sync::Arc::new(LiveTable::load(&origin, "orders", 100).expect("load"));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = GridView::with_source(table.clone(), Instant::now(), window, cx);
        view.attach_live(table.clone());
        view
    });
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.simulate_keystrokes("ctrl-t");
    let run = |cx: &mut gpui::VisualTestContext, sql: &str| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.sql_input.update(cx, |input, cx| input.set_content(sql, cx))
            })
        });
        cx.simulate_keystrokes("ctrl-enter");
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
    };
    run(cx, "SELECT customer, total FROM orders WHERE paid = 1 ORDER BY id");
    view.read_with(cx, |view, _| {
        assert_eq!(view.runs.len(), 1, "{}", view.status);
        assert_eq!(view.source.total(), 3);
        let names: Vec<&str> = view
            .source
            .columns()
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        assert_eq!(names, ["customer", "total"]);
    });
    run(cx, "DELETE FROM orders");
    view.read_with(cx, |view, _| {
        assert_eq!(view.runs.len(), 1, "a refused write adds no result tab");
        assert!(
            view.status.contains("ReadOnly") || view.status.contains("PolicyDenied"),
            "{}",
            view.status
        );
    });
    run(cx, "SELECT id FROM orders");
    view.read_with(cx, |view, _| {
        assert_eq!(view.runs.len(), 2);
        assert_eq!(view.source.total(), 6);
    });
}

#[gpui::test]
fn copy_as_uses_the_live_tables_columns_driver_and_name(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orders.sqlite"));
    let table = std::sync::Arc::new(LiveTable::load(&origin, "orders", 100).expect("load"));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = GridView::with_source(table.clone(), Instant::now(), window, cx);
        view.attach_live(table.clone());
        view
    });
    let copied =
        |cx: &mut gpui::VisualTestContext| cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.click_row(0, false);
            view.click_row(1, true);
            view.copy_selection_as(bookie_gpui::copy_format::CopyFormat::Insert, cx);
        })
    });
    let insert = copied(cx).expect("clipboard text");
    let first = insert.lines().next().unwrap();
    assert!(
        first.starts_with(
            "INSERT INTO \"orders\" (\"id\", \"customer\", \"total\", \"paid\", \"note\") VALUES (1, 'Ada', "
        ),
        "{first}"
    );
    assert_eq!(insert.lines().count(), 2);
    assert!(
        insert.lines().nth(1).unwrap().contains("'Grace'") && insert.contains("NULL"),
        "{insert}"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_gpui::cursor::Cursor { row: 0, column: 1 });
            view.copy_selection_as(bookie_gpui::copy_format::CopyFormat::InList, cx);
        })
    });
    assert_eq!(copied(cx).as_deref(), Some("('Ada', 'Grace')"));
}
