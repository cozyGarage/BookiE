use std::time::Instant;

use gpui::TestAppContext;

use crate::GridView;
use crate::text_input;
use crate::ui_tests::open;
use bookie_core::TableInfo;
use bookie_gpui::shell::table_info;

fn orders_fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/orders.sqlite")
}

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
    let before = view.read_with(cx, |view, _| view.sidebar.open);
    cx.simulate_keystrokes("ctrl-b");
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar.open, before, "the old chord is free")
    });
    cx.simulate_keystrokes("alt-b");
    view.read_with(cx, |view, _| assert_eq!(view.sidebar.open, !before));
}

#[gpui::test]
fn clicking_another_table_of_a_live_connection_loads_it_into_the_grid(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = GridView::with_source(table.clone(), Instant::now(), window, cx);
        view.attach_live(table.clone());
        view
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.catalog.grid_table, table_info("orders"));
        assert_eq!(view.catalog.tables, [table_info("customers"), table_info("orders")]);
        assert_eq!(view.source.total(), 6);
        assert!(
            view.source
                .origin()
                .contains("concurrent changes may skip or repeat rows")
        );
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_table_named(&table_info("customers"), window, cx)
        })
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.catalog.grid_table, table_info("customers"));
        assert_eq!(view.source.total(), 2);
        assert_eq!(view.source.columns().len(), 2);
        assert_eq!(
            view.tabs.current(),
            &bookie_gpui::shell::Tab::Table(table_info("customers"))
        );
        assert!(
            view.source
                .origin()
                .contains("concurrent changes may skip or repeat rows")
        );
        assert!(view.status.is_empty(), "{}", view.status);
    });
}

#[gpui::test]
fn a_live_connection_runs_typed_sql_into_a_result_tab_and_refuses_writes(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
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
        assert_eq!(view.run_tabs.slots.len(), 1, "{}", view.status);
        assert_eq!(view.source.total(), 3);
        assert!(
            view.run_tabs.slots[0].run.measured().is_some(),
            "a run against a database records its real time"
        );
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
        assert_eq!(view.run_tabs.slots.len(), 1, "a refused write adds no result tab");
        let failure = view.status.to_lowercase();
        assert!(
            failure.contains("read-only") || failure.contains("blocked by policy"),
            "{}",
            view.status
        );
    });
    run(cx, "SELECT id FROM orders");
    view.read_with(cx, |view, _| {
        assert_eq!(view.run_tabs.slots.len(), 2);
        assert_eq!(view.source.total(), 6);
    });
}

#[gpui::test]
fn live_sqlite_selection_copies_exact_typed_values(cx: &mut TestAppContext) {
    use bookie_core::Value;
    use bookie_gpui::live::{LiveSession, Origin};

    let session = LiveSession::open(&Origin::File(orders_fixture())).expect("open SQLite fixture");
    let result = std::sync::Arc::new(
        session
            .run_query(
                "SELECT 9223372036854775807 AS n, X'00FF' AS raw, NULL AS missing, 'α' AS text",
                100,
            )
            .expect("read typed values"),
    );
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(result, Instant::now(), window, cx));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.click_row(0, false);
            let selected = view.selected_rows(view.selection.expect("selected row"));
            assert_eq!(
                selected,
                vec![vec![
                    Value::Int(i64::MAX),
                    Value::Bytes(vec![0, 255]),
                    Value::Null,
                    Value::Text("α".into()),
                ]]
            );
            view.copy_selection(false, cx);
        })
    });
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some("9223372036854775807\t\\x00ff\t\\N\tα"));
}

#[gpui::test]
fn query_results_do_not_copy_inserts_to_the_previously_browsed_table(cx: &mut TestAppContext) {
    let origin = bookie_gpui::live::Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        bookie_gpui::live::LiveTable::load(
            &origin,
            &bookie_core::TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = GridView::with_source(table.clone(), Instant::now(), window, cx);
        view.attach_live(table.clone());
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.sql_input.update(cx, |input, cx| {
                input.set_value("SELECT customer FROM orders", window, cx)
            });
            view.run_query(cx);
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("unchanged".into()));
        view.update(cx, |view, cx| {
            view.click_row(0, false);
            view.copy_selection_as(bookie_app_services::result_copy::CopyFormat::Insert, cx);
        });
    });
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(clipboard.as_deref(), Some("unchanged"));
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Copy as INSERT is available for table rows only.");
        assert!(
            !view
                .palette_actions()
                .iter()
                .any(|action| action.id == "copy-insert" && action.enabled)
        );
    });
}

#[gpui::test]
fn copy_as_uses_the_live_tables_columns_driver_and_name(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
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
            view.copy_selection_as(bookie_app_services::result_copy::CopyFormat::Insert, cx);
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
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            view.copy_selection_as(bookie_app_services::result_copy::CopyFormat::InList, cx);
        })
    });
    assert_eq!(copied(cx).as_deref(), Some("('Ada', 'Grace')"));
}

#[gpui::test]
fn every_statement_is_logged_and_a_refused_one_opens_the_messages_view(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    use bookie_gpui::messages::MessageKind;
    use bookie_gpui::shell::ResultView;
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
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
        assert_eq!(view.panel.view, ResultView::Table, "a good run keeps the table view");
        let entries = view.panel.messages.entries();
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
        assert_eq!(view.panel.view, ResultView::Messages, "a refused statement shows why");
        let entries = view.panel.messages.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].kind, MessageKind::Error);
        assert!(
            entries[1].text.starts_with("DELETE FROM orders · "),
            "{}",
            entries[1].text
        );
        assert_eq!(view.run_tabs.slots.len(), 1, "no result tab for a refused statement");
    });
}

#[gpui::test]
fn connecting_from_the_home_page_swaps_the_synthetic_grid_for_the_first_table(cx: &mut TestAppContext) {
    use bookie_gpui::live::Origin;
    let (view, cx) = open(cx);
    view.read_with(cx, |view, _| {
        assert!(view.live.is_none());
        assert_eq!(view.catalog.grid_table, table_info("synthetic_rows"));
    });
    let origin = Origin::File(orders_fixture());
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.live.is_some(), "{}", view.status);
        assert_eq!(view.catalog.tables, [table_info("customers"), table_info("orders")]);
        assert_eq!(view.catalog.grid_table, table_info("customers"));
        assert_eq!(view.source.total(), 2);
        assert_eq!(
            view.tabs.current(),
            &bookie_gpui::shell::Tab::Table(table_info("customers"))
        );
        assert_eq!(view.status, "Connected");
        assert!(view.run_tabs.slots.is_empty(), "the synthetic run is gone");
    });
}

struct SqliteFixture(std::path::PathBuf);

impl Drop for SqliteFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn sqlite_fixture(statements: &[&str]) -> SqliteFixture {
    let path = std::env::temp_dir().join(format!("bookie-empty-{}.sqlite", uuid::Uuid::new_v4()));
    std::fs::File::create(&path).expect("create database");
    let fixture = SqliteFixture(path);
    tokio::runtime::Runtime::new().expect("runtime").block_on(async {
        let registry = drivers_registry::default_registry();
        let driver = registry.get("sqlite").expect("sqlite driver");
        let connection = driver
            .connect(bookie_core::ConnectOptions {
                database: fixture.0.to_string_lossy().into_owned(),
                ..Default::default()
            })
            .await
            .expect("open database");
        for statement in statements {
            connection.execute(statement).await.expect("prepare database");
        }
        connection.close().await.expect("close database");
    });
    fixture
}

#[gpui::test]
fn an_empty_database_opens_a_query_without_synthetic_rows(cx: &mut TestAppContext) {
    let fixture = sqlite_fixture(&["CREATE TABLE seed (id INTEGER)", "DROP TABLE seed"]);
    let (view, cx) = open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.live.is_some(), "{}", view.status);
        assert!(view.catalog.tables.is_empty());
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Query);
        assert_eq!(view.dataset_count(), 0);
        assert!(!view.source.origin().contains("Synthetic"));
        assert!(
            !view
                .palette_actions()
                .iter()
                .any(|action| action.id == "table" && action.enabled)
        );
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input
                .update(cx, |input, cx| input.set_value("SELECT 1", window, cx));
            view.run_query(cx);
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.source.total(), 1);
        assert_eq!(view.source.row(0)[0], bookie_gpui::model::Cell::Int(1));
    });
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("unchanged".into()));
        view.update(cx, |view, cx| {
            view.click_row(0, false);
            view.copy_selection_as(bookie_app_services::result_copy::CopyFormat::Insert, cx);
        });
    });
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(clipboard.as_deref(), Some("unchanged"));
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Copy as INSERT is available for table rows only.");
        assert!(
            !view
                .palette_actions()
                .iter()
                .any(|action| action.id == "copy-insert" && action.enabled)
        );
    });
}

#[gpui::test]
fn an_empty_table_name_is_not_an_empty_catalog(cx: &mut TestAppContext) {
    let fixture = sqlite_fixture(&[r#"CREATE TABLE "" (id INTEGER)"#]);
    let (view, cx) = open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.live.is_some(), "{}", view.status);
        assert_eq!(view.catalog.tables, [table_info("")]);
        assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Table(table_info("")));
        assert!(
            view.palette_actions()
                .iter()
                .any(|action| action.id == "table" && action.enabled)
        );
    });
}

#[gpui::test]
fn connecting_does_not_discard_an_active_cell_editor(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.start_edit(0, 1, window, cx);
            view.cell_edit.editor
                .update(cx, |editor, cx| editor.set_content("draft text", cx));
            view.connect(bookie_gpui::live::Origin::File("/missing/bookie.db".into()), cx);
        });
    });
    view.read_with(cx, |view, cx| {
        assert!(view.live.is_none());
        assert!(view.cell_edit.active.is_some());
        assert_eq!(view.cell_edit.editor.read(cx).content(), "draft text");
        assert!(view.status.contains("view-only edits"));
    });
}

#[gpui::test]
fn a_late_connection_does_not_replace_a_cell_editor(cx: &mut TestAppContext) {
    let fixture = sqlite_fixture(&["CREATE TABLE seed (id INTEGER)", "DROP TABLE seed"]);
    let (view, cx) = open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.connect(origin, cx);
            view.start_edit(0, 1, window, cx);
            view.cell_edit.editor
                .update(cx, |editor, cx| editor.set_content("late draft", cx));
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert!(view.live.is_none());
        assert!(view.cell_edit.active.is_some());
        assert_eq!(view.cell_edit.editor.read(cx).content(), "late draft");
        assert!(view.status.contains("view-only edits"));
    });
}

#[gpui::test]
fn connecting_keeps_view_edits_until_the_user_discards_them(cx: &mut TestAppContext) {
    let fixture = sqlite_fixture(&["CREATE TABLE seed (id INTEGER)", "DROP TABLE seed"]);
    let (view, cx) = open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("active".into()));
            view.cell_edit.table
                .insert((1, 1), bookie_gpui::model::Cell::Text("table".into()));
            view.run_tabs.slots[0]
                .edits
                .insert((2, 1), bookie_gpui::model::Cell::Text("run".into()));
            view.connect(origin.clone(), cx);
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.live.is_none());
        assert_eq!(
            (view.cell_edit.edits.len(), view.cell_edit.table.len(), view.run_tabs.slots[0].edits.len()),
            (1, 1, 1)
        );
        assert!(view.status.contains("Discard all view-only edits"));
    });
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("Discard all view-only edits");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty() && view.cell_edit.table.is_empty());
        assert!(view.run_tabs.slots.iter().all(|slot| slot.edits.is_empty()));
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.live.is_some(), "{}", view.status));
}

#[gpui::test]
fn a_late_connection_does_not_discard_a_new_view_edit(cx: &mut TestAppContext) {
    let fixture = sqlite_fixture(&["CREATE TABLE seed (id INTEGER)", "DROP TABLE seed"]);
    let (view, cx) = open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connect(origin, cx);
            view.cell_edit.edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("late edit".into()));
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.live.is_none());
        assert_eq!(
            view.cell_edit.edits.get(&(0, 1)),
            Some(&bookie_gpui::model::Cell::Text("late edit".into()))
        );
        assert!(view.status.contains("view-only edits"));
    });
}

#[gpui::test]
fn switching_tables_keeps_all_view_edits_until_the_user_discards_them(cx: &mut TestAppContext) {
    let fixture = sqlite_fixture(&[
        "CREATE TABLE alpha (id INTEGER, name TEXT)",
        "INSERT INTO alpha VALUES (1, 'Ada')",
        "CREATE TABLE beta (id INTEGER)",
    ]);
    let (view, cx) = open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("table draft".into()));
            view.cell_edit.table
                .insert((1, 1), bookie_gpui::model::Cell::Text("saved table draft".into()));
            view.run_tabs.slots.push(crate::runs::RunSlot::new(
                bookie_gpui::shell::Run {
                    number: 1,
                    rows: 0,
                    elapsed_ms: None,
                },
                None,
            ));
            view.run_tabs.slots[0]
                .edits
                .insert((2, 1), bookie_gpui::model::Cell::Text("result draft".into()));
            view.open_table_named(&table_info("beta"), window, cx);
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.catalog.grid_table, table_info("alpha"));
        assert_eq!(view.cell_edit.edits.len(), 1);
        assert_eq!(view.cell_edit.table.len(), 1);
        assert_eq!(view.run_tabs.slots[0].edits.len(), 1);
        assert!(view.status.contains("Discard all view-only edits"));
    });
}

#[gpui::test]
fn a_late_table_load_does_not_discard_a_new_view_edit(cx: &mut TestAppContext) {
    let fixture = sqlite_fixture(&[
        "CREATE TABLE alpha (id INTEGER, name TEXT)",
        "INSERT INTO alpha VALUES (1, 'Ada')",
        "CREATE TABLE beta (id INTEGER)",
    ]);
    let (view, cx) = open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_table_named(&table_info("beta"), window, cx);
            view.cell_edit.edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("late table draft".into()));
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.catalog.grid_table, table_info("alpha"));
        assert_eq!(
            view.cell_edit.edits.get(&(0, 1)),
            Some(&bookie_gpui::model::Cell::Text("late table draft".into()))
        );
    });
}

#[gpui::test]
fn a_failed_table_switch_keeps_the_current_table(cx: &mut TestAppContext) {
    let fixture = sqlite_fixture(&[
        "CREATE TABLE alpha (id INTEGER)",
        "INSERT INTO alpha VALUES (1)",
        "CREATE TABLE beta (id INTEGER)",
    ]);
    let (view, cx) = open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_table_named(&table_info("missing"), window, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.catalog.grid_table, table_info("alpha"));
        assert_eq!(
            view.tabs.current(),
            &bookie_gpui::shell::Tab::Table(table_info("alpha"))
        );
        assert!(!view.status.is_empty());
    });
}

#[gpui::test]
fn cancelling_a_table_switch_keeps_the_current_table(cx: &mut TestAppContext) {
    let fixture = sqlite_fixture(&[
        "CREATE TABLE alpha (id INTEGER)",
        "INSERT INTO alpha VALUES (1)",
        "CREATE TABLE beta (id INTEGER)",
    ]);
    let (view, cx) = open(cx);
    let origin = bookie_gpui::live::Origin::File(fixture.0.clone());
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(origin, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_table_named(&table_info("beta"), window, cx);
            view.loads.cancel.cancel();
        });
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.catalog.grid_table, table_info("alpha"));
        assert_eq!(
            view.tabs.current(),
            &bookie_gpui::shell::Tab::Table(table_info("alpha"))
        );
    });
}

#[gpui::test]
fn closing_a_query_tab_with_result_edits_requires_explicit_discard(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, _cx| {
        view.tabs.open_query();
        view.sync_grid_owner();
        view.add_run(None);
        view.run_tabs.slots[view.run_tabs.active]
            .edits
            .insert((0, 1), bookie_gpui::model::Cell::Text("result draft".into()));
        view.close_tab(2);
    });
    view.read_with(cx, |view, _| {
        assert!(
            view.tabs
                .items
                .iter()
                .any(|tab| matches!(tab, bookie_gpui::shell::Tab::Query))
        );
        assert!(view.run_tabs.slots.iter().any(|slot| !slot.edits.is_empty()));
        assert!(view.status.contains("Discard all view-only edits"));
    });
}

#[gpui::test]
fn closing_a_result_with_pending_edits_requires_explicit_discard(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, _cx| {
        view.tabs.open_query();
        view.sync_grid_owner();
        view.add_run(None);
        view.cell_edit.edits
            .insert((0, 1), bookie_gpui::model::Cell::Text("result draft".into()));
        view.close_run(view.run_tabs.active);
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.run_tabs.slots.len(), 2);
        assert_eq!(view.cell_edit.edits.len(), 1);
        assert!(view.status.contains("Discard all view-only edits"));
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
        assert_eq!(view.catalog.grid_table, table_info("synthetic_rows"));
        assert!(
            view.status.contains("none.sqlite"),
            "the failure must name the file that could not be opened: {}",
            view.status
        );
    });
}

#[gpui::test]
fn the_sql_editor_takes_several_lines_and_ctrl_enter_runs_them(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
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
        assert_eq!(view.run_tabs.slots.len(), 0, "nothing ran yet");
    });
    cx.simulate_keystrokes("ctrl-enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.run_tabs.slots.len(), 1, "{}", view.status);
        assert_eq!(view.source.total(), 6);
    });
}

#[gpui::test]
fn the_sessions_command_is_hidden_and_does_nothing_off_postgres(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
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
            "",
            "no activity query exists for SQLite"
        );
        assert!(!matches!(view.tabs.current(), crate::Tab::Query));
    });
}

#[gpui::test]
fn explaining_a_query_shows_the_plan_as_a_tree_and_a_normal_run_clears_it(cx: &mut TestAppContext) {
    use bookie_gpui::shell::ResultView;
    let (view, cx) = open_live(cx);
    let run = |view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, explain: bool| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.tabs.open_query();
                view.sync_grid_owner();
                view.sql_input.update(cx, |input, cx| {
                    input.set_value("SELECT customer FROM orders", window, cx)
                });
                if explain {
                    view.explain_query(cx);
                } else {
                    view.run_query(cx);
                }
            })
        });
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
    };
    run(&view, cx, true);
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.view, ResultView::Plan);
        let first = view.panel.plan.first().expect("a plan node");
        assert!(first.title.to_uppercase().contains("SCAN"), "{}", first.title);
    });
    run(&view, cx, false);
    view.read_with(cx, |view, _| {
        assert!(view.panel.plan.is_empty(), "a normal run drops the old plan");
        assert_ne!(view.panel.view, ResultView::Plan);
    });
}

fn open_live(cx: &mut TestAppContext) -> (gpui::Entity<GridView>, &mut gpui::VisualTestContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
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
    (view, cx)
}

#[gpui::test]
fn running_a_query_with_ctrl_enter_does_not_add_a_line(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|window, cx| {
        let editor = view.read(cx).sql_input.clone();
        editor.update(cx, |editor, cx| editor.focus(window, cx));
    });
    cx.simulate_input("SELECT id FROM orders");

    cx.simulate_keystrokes("ctrl-enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();

    view.read_with(cx, |view, cx| {
        assert_eq!(view.sql_input.read(cx).value(), "SELECT id FROM orders");
        assert_eq!(view.run_tabs.slots.len(), 1, "{}", view.status);
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
        let input = view.read(cx).palette.input.clone();
        input.update(cx, |input, cx| input.set_content(&address, cx));
    });
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let entries = view.panel.messages.entries();
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
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = GridView::with_source(table.clone(), Instant::now(), window, cx);
        view.attach_live(table.clone());
        view
    });
    cx.update(|_, cx| view.update(cx, |view, _| view.show_status()));
    view.read_with(cx, |view, _| {
        let entries = view.panel.messages.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, bookie_gpui::messages::MessageKind::Success);
        assert!(
            entries[0].text.ends_with(" · sqlite · healthy · direct"),
            "{}",
            entries[0].text
        );
    });
}

#[gpui::test]
fn a_new_query_tab_focuses_its_editor_so_typing_lands_there(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
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
    cx.run_until_parked();
    cx.simulate_input("select 1");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sql_input.read(cx).value(), "select 1");
    });
    cx.simulate_keystrokes("ctrl-b");
    view.read_with(cx, |view, _| {
        assert!(!view.sidebar.open, "the sidebar shortcut still works in the editor");
    });
    cx.simulate_keystrokes("ctrl-b");
    view.read_with(cx, |view, _| {
        assert!(view.sidebar.open, "the sidebar shortcut still reopens the sidebar");
    });
    cx.simulate_input(" + 1");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sql_input.read(cx).value(), "select 1 + 1");
    });
    cx.simulate_keystrokes("ctrl-shift-p");
    view.read_with(cx, |view, _| {
        assert!(view.palette.open, "global shortcuts still work in the editor")
    });
}

#[gpui::test]
fn the_sql_editor_edits_at_several_carets_and_ctrl_enter_still_runs_the_whole_buffer(cx: &mut TestAppContext) {
    use bookie_gpui::live::{LiveTable, Origin};
    #[cfg(target_os = "macos")]
    let add_cursor_below = "cmd-alt-down";
    #[cfg(target_os = "windows")]
    let add_cursor_below = "ctrl-alt-down";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let add_cursor_below = "alt-shift-down";
    let origin = Origin::File(orders_fixture());
    let table = std::sync::Arc::new(
        LiveTable::load(
            &origin,
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load"),
    );
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
        editor.update(cx, |editor, cx| {
            editor.set_value("SELECT id\nFROM orders\nLIMIT 3", window, cx);
            editor.set_cursor_position(gpui_kit::component::input::Position::new(0, 0), window, cx);
            editor.focus(window, cx);
        });
    });
    cx.simulate_keystrokes(&format!("{add_cursor_below} {add_cursor_below}"));
    cx.simulate_input("-- ");
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.sql_input.read(cx).value(),
            "-- SELECT id\n-- FROM orders\n-- LIMIT 3",
            "one keystroke burst edits every caret"
        );
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_input("x");
    view.read_with(cx, |view, cx| {
        let value = view.sql_input.read(cx).value().to_string();
        assert_eq!(
            value.matches('x').count(),
            1,
            "Escape returns to a single caret: {value}"
        );
    });
    cx.update(|window, cx| {
        let editor = view.read(cx).sql_input.clone();
        editor.update(cx, |editor, cx| {
            editor.set_value("SELECT id\nFROM orders\nLIMIT 3", window, cx);
            editor.set_cursor_position(gpui_kit::component::input::Position::new(0, 0), window, cx);
        });
    });
    cx.simulate_keystrokes(&format!("{add_cursor_below} ctrl-enter"));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.run_tabs.slots.len(), 1, "{}", view.status);
        assert_eq!(view.source.total(), 3, "the whole buffer ran, not one caret's line");
    });
}

#[gpui::test]
fn activity_palette_queries_reflect_driver_capabilities(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);

    view.update(cx, |view, _| {
        let session = view.live.as_ref().unwrap().with_driver_id("postgres");
        view.live = Some(session);
    });
    view.read_with(cx, |view, _| {
        let actions = view.palette_actions();
        for id in [
            "sessions",
            "blocking-locks",
            "long-running",
            "replication-lag",
            "server-variables",
            "server-status",
        ] {
            let action = actions.iter().find(|a| a.id == id).expect(id);
            assert!(action.enabled, "PostgreSQL should enable activity query {id}");
        }
    });

    view.update(cx, |view, _| {
        let session = view.live.as_ref().unwrap().with_driver_id("mysql");
        view.live = Some(session);
    });
    view.read_with(cx, |view, _| {
        let actions = view.palette_actions();
        for id in ["sessions", "long-running", "server-variables", "server-status"] {
            let action = actions.iter().find(|a| a.id == id).expect(id);
            assert!(action.enabled, "MySQL should enable activity query {id}");
        }
        for id in ["blocking-locks", "replication-lag"] {
            let action = actions.iter().find(|a| a.id == id).expect(id);
            assert!(!action.enabled, "MySQL should disable activity query {id}");
        }
    });

    view.update(cx, |view, _| {
        let session = view.live.as_ref().unwrap().with_driver_id("sqlite");
        view.live = Some(session);
    });
    view.read_with(cx, |view, _| {
        let actions = view.palette_actions();
        for id in [
            "sessions",
            "blocking-locks",
            "long-running",
            "replication-lag",
            "server-variables",
            "server-status",
        ] {
            let action = actions.iter().find(|a| a.id == id).expect(id);
            assert!(!action.enabled, "SQLite should disable activity query {id}");
        }
    });
}

#[gpui::test]
fn activity_query_execution_sets_sql_and_switches_tab(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    view.update(cx, |view, _| {
        let session = view.live.as_ref().unwrap().with_driver_id("postgres");
        view.live = Some(session);
    });

    for kind in [
        bookie_core::ActivityQuery::BlockingLocks,
        bookie_core::ActivityQuery::LongRunning,
        bookie_core::ActivityQuery::ReplicationLag,
        bookie_core::ActivityQuery::ServerVariables,
        bookie_core::ActivityQuery::ServerStatus,
    ] {
        cx.update(|window, cx| view.update(cx, |view, cx| view.show_activity(kind, window, cx)));
        view.read_with(cx, |view, cx| {
            let expected_sql = bookie_core::activity_sql("postgres", kind, None).unwrap();
            assert_eq!(view.sql_input.read(cx).value(), expected_sql);
            assert!(matches!(view.tabs.current(), crate::Tab::Query));
        });
    }
}

#[gpui::test]
fn export_selection_or_page_copies_csv_to_clipboard(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.export_selection_or_page(cx);
        });
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.status, "Exported 6 rows as CSV to clipboard");
        let clipboard = cx.read_from_clipboard().expect("clipboard text");
        let text = clipboard.text().expect("text");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 7, "1 header line + 6 data rows");
        assert!(lines[0].starts_with("id,customer,"));
    });

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selection = Some(crate::Selection { anchor: 0, focus: 1 });
            view.export_selection_or_page(cx);
        });
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.status, "Exported 2 rows as CSV to clipboard");
        let clipboard = cx.read_from_clipboard().expect("clipboard text");
        let text = clipboard.text().expect("text");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "1 header line + 2 data rows");
    });
}

#[gpui::test]
fn the_csv_delimiter_command_changes_exports_until_it_is_reset(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);

    view.update(cx, |view, _| {
        view.palette.delimiter = bookie_gpui::csv_delimiter::parse_command("csv delimiter ||");
        let actions = view.palette_actions();
        let set = actions
            .iter()
            .find(|action| action.id == "set-csv-delimiter")
            .expect("set action");
        assert!(set.enabled && set.label.starts_with("CSV delimiter ||"));
        assert!(bookie_gpui::palette::fuzzy_score("csv delimiter ||", &set.label) > 0);
        assert!(actions.iter().all(|action| action.id != "reset-csv-delimiter"));
        view.set_csv_delimiter();
        assert_eq!(view.status, "CSV delimiter set to \"||\"");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.export_selection_or_page(cx)));
    view.read_with(cx, |view, cx| {
        let text = cx.read_from_clipboard().expect("clipboard text").text().expect("text");
        assert!(
            text.lines()
                .next()
                .is_some_and(|header| header.starts_with("id||customer||")),
            "{text}"
        );
        assert!(
            view.palette_actions()
                .iter()
                .any(|action| action.id == "reset-csv-delimiter")
        );
    });

    view.update(cx, |view, _| {
        view.palette.delimiter = bookie_gpui::csv_delimiter::parse_command("csv delimiter 'x");
        let actions = view.palette_actions();
        let bad = actions
            .iter()
            .find(|action| action.id == "set-csv-delimiter")
            .expect("refusal row");
        assert!(bad.enabled && bad.label.ends_with("no quote characters"));
        view.set_csv_delimiter();
        assert_eq!(view.status, "CSV delimiter refused: no quote characters");
        view.set_csv_delimiter_to(None);
        assert_eq!(view.status, "CSV delimiter reset to the comma");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.export_selection_or_page(cx)));
    view.read_with(cx, |_, cx| {
        let text = cx.read_from_clipboard().expect("clipboard text").text().expect("text");
        assert!(
            text.lines()
                .next()
                .is_some_and(|header| header.starts_with("id,customer,"))
        );
    });
}

#[gpui::test]
fn the_export_limit_command_stops_the_csv_export_until_it_is_removed(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);

    view.update(cx, |view, _| {
        view.palette.limit = bookie_gpui::export_limit::parse_command("export limit 1");
        let actions = view.palette_actions();
        let set = actions
            .iter()
            .find(|action| action.id == "set-export-limit")
            .expect("set action");
        assert!(set.enabled && set.label.starts_with("Export limit 1"));
        assert!(actions.iter().all(|action| action.id != "reset-export-limit"));
        view.set_export_limit();
        assert_eq!(view.status, "Export limit set to 1 rows");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.export_selection_or_page(cx)));
    view.read_with(cx, |view, cx| {
        let text = cx.read_from_clipboard().expect("clipboard text").text().expect("text");
        assert_eq!(text.lines().count(), 2, "1 header line + 1 data row: {text}");
        assert!(view.status.contains("Exported the first 1 row"), "{}", view.status);
        assert!(
            view.palette_actions()
                .iter()
                .any(|action| action.id == "reset-export-limit")
        );
    });

    view.update(cx, |view, _| {
        view.palette.limit = bookie_gpui::export_limit::parse_command("export limit 0");
        view.set_export_limit();
        assert_eq!(view.status, "Export limit refused: a whole number of at least 1");
        view.set_export_limit_to(None);
        assert_eq!(view.status, "Export limit removed");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.export_selection_or_page(cx)));
    view.read_with(cx, |_, cx| {
        let text = cx.read_from_clipboard().expect("clipboard text").text().expect("text");
        assert!(text.lines().count() > 2, "{text}");
    });
}

#[gpui::test]
fn exporting_the_page_writes_every_row_of_the_page_from_the_first(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.export_selection_or_page(cx)));
    view.read_with(cx, |view, cx| {
        let text = cx.read_from_clipboard().expect("clipboard text").text().expect("text");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 7, "1 header line + all 6 rows: {text}");
        assert!(
            lines[1].starts_with("1,Ada,"),
            "the first row comes first: {}",
            lines[1]
        );
        assert!(lines[6].starts_with("6,"), "the last row comes last: {}", lines[6]);
        assert_eq!(view.status, "Exported 6 rows as CSV to clipboard");
    });
}

#[gpui::test]
fn header_collapse_all_and_feedback_actions(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);

    view.update(cx, |view, _| {
        assert!(view.sidebar.connection_open && view.sidebar.database_open);
        let collapse = view.sidebar.connection_open || view.sidebar.database_open;
        view.sidebar.connection_open = !collapse;
        view.sidebar.database_open = !collapse;
        assert!(!view.sidebar.connection_open && !view.sidebar.database_open);

        let collapse = view.sidebar.connection_open || view.sidebar.database_open;
        view.sidebar.connection_open = !collapse;
        view.sidebar.database_open = !collapse;
        assert!(view.sidebar.connection_open && view.sidebar.database_open);
    });

    view.update(cx, |view, _| {
        view.live = None;
        view.show_status();
        assert_eq!(view.status, "No active database connection.");
    });
}

fn connect_and_settle(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, path: std::path::PathBuf) {
    cx.update(|_, cx| view.update(cx, |view, cx| view.connect(bookie_gpui::live::Origin::File(path), cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
}

#[gpui::test]
fn switching_connection_drops_the_previous_sidebar_fields(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    connect_and_settle(&view, cx, orders_fixture());
    let customers = table_info("customers");
    cx.update(|_, cx| view.update(cx, |view, cx| view.toggle_fields(&customers, cx)));
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.sidebar.fields.len(), 1));
    let other = sqlite_fixture(&["create table t (a integer)"]);
    connect_and_settle(&view, cx, other.0.clone());
    view.read_with(cx, |view, _| {
        assert!(
            view.sidebar.fields.is_empty(),
            "fields of the old connection stayed in the sidebar"
        );
        assert!(view.sidebar.catalog.is_empty());
    });
}
