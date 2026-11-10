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
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let origin = Origin::File(std::path::PathBuf::from(manifest_dir).join("tests/fixtures/orders.sqlite"));
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
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let origin = Origin::File(std::path::PathBuf::from(manifest_dir).join("tests/fixtures/orders.sqlite"));
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
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.sql_input.update(cx, |input, cx| input.set_value(sql, window, cx))
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

#[gpui::test]
fn every_statement_is_logged_and_a_refused_one_opens_the_messages_view(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    use bookie_gpui::messages::MessageKind;
    use bookie_gpui::shell::ResultView;
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
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.sql_input.update(cx, |input, cx| input.set_value(sql, window, cx))
            })
        });
        cx.simulate_keystrokes("ctrl-enter");
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
    };
    run(cx, "SELECT id\n  FROM orders");
    view.read_with(cx, |view, _| {
        assert_eq!(view.result_view, ResultView::Table, "a good run keeps the table view");
        let entries = view.messages.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, MessageKind::Success);
        assert!(
            entries[0].text.starts_with("SELECT id FROM orders · 6 rows · "),
            "{}",
            entries[0].text
        );
    });
    run(cx, "DELETE FROM orders");
    view.read_with(cx, |view, _| {
        assert_eq!(view.result_view, ResultView::Messages, "a refused statement shows why");
        let entries = view.messages.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].kind, MessageKind::Error);
        assert!(
            entries[1].text.starts_with("DELETE FROM orders · "),
            "{}",
            entries[1].text
        );
        assert_eq!(view.runs.len(), 1, "no result tab for a refused statement");
    });
}

#[gpui::test]
fn connecting_from_the_home_page_swaps_the_synthetic_grid_for_the_first_table(cx: &mut TestAppContext) {
    use bookie_gpui::live::Origin;
    let (view, cx) = open(cx);
    view.read_with(cx, |view, _| {
        assert!(view.live.is_none());
        assert_eq!(view.catalog.grid_table, "synthetic_rows");
    });
    let origin = Origin::File(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orders.sqlite"));
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.live.is_some(), "{}", view.status);
        assert_eq!(view.catalog.tables, ["customers", "orders"]);
        assert_eq!(view.catalog.grid_table, "customers");
        assert_eq!(view.source.total(), 2);
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Table("customers".into()));
        assert_eq!(view.status, "Connected");
        assert!(view.runs.is_empty(), "the synthetic run is gone");
    });
}

#[gpui::test]
fn a_connection_that_cannot_be_opened_keeps_the_synthetic_view_and_says_why(cx: &mut TestAppContext) {
    use bookie_gpui::live::Origin;
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connect(Origin::File("/nonexistent/none.sqlite".into()), cx)
        })
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.live.is_none());
        assert_eq!(view.catalog.grid_table, "synthetic_rows");
        assert!(
            view.status.contains("NotADatabase") || view.status.contains("Other"),
            "{}",
            view.status
        );
    });
}

#[gpui::test]
fn the_sql_editor_takes_several_lines_and_ctrl_enter_runs_them(cx: &mut TestAppContext) {
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
    cx.update(|window, cx| {
        let editor = view.read(cx).sql_input.clone();
        editor.update(cx, |editor, cx| editor.focus(window, cx));
    });
    cx.simulate_input("SELECT id");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("FROM orders");
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.sql_input.read(cx).value(),
            "SELECT id\nFROM orders",
            "Enter adds a line, it does not run"
        );
        assert_eq!(view.runs.len(), 0, "nothing ran yet");
    });
    cx.simulate_keystrokes("ctrl-enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.runs.len(), 1, "{}", view.status);
        assert_eq!(view.source.total(), 6);
    });
}

#[gpui::test]
fn the_sessions_command_is_hidden_off_postgres_and_fills_the_editor_with_the_sessions_query(cx: &mut TestAppContext) {
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
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("sessions");
    view.read_with(cx, |view, cx| {
        assert!(view.palette_shown(cx).is_empty(), "SQLite has no pg_stat_activity");
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.show_sessions(window, cx)));
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.sql_input.read(cx).value(),
            bookie_app_services::pg_sessions::SESSIONS_SQL
        );
        assert!(matches!(view.tabs.current(), crate::Tab::Query));
    });
}

#[gpui::test]
fn a_typed_host_and_port_offers_a_tcp_check_that_lands_in_messages(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("local");
    view.read_with(cx, |view, cx| {
        let labels: Vec<String> = view
            .palette_shown(cx)
            .into_iter()
            .map(|index| view.palette_actions()[index].label.clone())
            .collect();
        assert!(labels.iter().all(|label| !label.starts_with("Check TCP")), "{labels:?}");
    });
    cx.simulate_input("");
    cx.update(|_, cx| {
        let input = view.read(cx).palette_input.clone();
        input.update(cx, |input, cx| input.set_content(&address, cx));
    });
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let entries = view.messages.entries();
        assert_eq!(entries.len(), 1, "{}", view.status);
        assert_eq!(entries[0].kind, bookie_gpui::messages::MessageKind::Success);
        assert!(
            entries[0].text.starts_with(&format!("{address} is up · ")),
            "{}",
            entries[0].text
        );
        assert!(entries[0].text.ends_with("up 1/1 checks"));
    });
    drop(listener);
}

#[gpui::test]
fn the_redis_listen_command_is_not_offered_on_other_engines(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("news.*");
    view.read_with(cx, |view, cx| {
        let labels: Vec<String> = view
            .palette_shown(cx)
            .into_iter()
            .map(|index| view.palette_actions()[index].label.clone())
            .collect();
        assert!(labels.iter().all(|label| !label.contains("Redis")), "{labels:?}");
    });
}

#[gpui::test]
fn the_status_command_reports_a_healthy_direct_connection_in_messages(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orders.sqlite"));
    let table = std::sync::Arc::new(LiveTable::load(&origin, "orders", 100).expect("load"));
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = GridView::with_source(table.clone(), Instant::now(), window, cx);
        view.attach_live(table.clone());
        view
    });
    cx.update(|_, cx| view.update(cx, |view, _| view.show_status()));
    view.read_with(cx, |view, _| {
        let entries = view.messages.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, bookie_gpui::messages::MessageKind::Success);
        assert!(
            entries[0].text.ends_with(" · sqlite · healthy · direct"),
            "{}",
            entries[0].text
        );
    });
}
