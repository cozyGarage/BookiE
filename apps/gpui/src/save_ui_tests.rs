use std::time::Instant;

use gpui::TestAppContext;

use crate::GridView;
use crate::live_ui_tests::open_live;
use crate::text_input;
use bookie_core::TableInfo;

fn orders_fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/orders.sqlite")
}

fn writable_table(path: &std::path::Path) -> std::sync::Arc<bookie_gpui::live::LiveTable> {
    use bookie_gpui::live::{LiveSession, Origin};
    let mut session = LiveSession::open(&Origin::File(path.to_path_buf())).expect("open");
    std::sync::Arc::get_mut(&mut session)
        .expect("the session is not shared yet")
        .writable = Some(uuid::Uuid::new_v4());
    let table = TableInfo {
        schema: None,
        name: "orders".into(),
    };
    std::sync::Arc::new(
        session
            .load_table_controlled(&table, 100, &tokio_util::sync::CancellationToken::new())
            .expect("load"),
    )
}

fn write_session(path: &std::path::Path, read_only: bool) -> std::sync::Arc<bookie_gpui::live::WriteSession> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("runtime");
    let sessions = bookie_app_services::read_session_store::ReadSessionStore::new(
        std::sync::Arc::new(drivers_registry::default_registry()),
        std::sync::Arc::new(bookie_app_services::database_service::DatabaseService::new_isolated()),
    );
    let control = bookie_core::OperationControl::with_timeout(std::time::Duration::from_secs(10));
    let opened = runtime
        .block_on(sessions.open_file_writable_for_tests(path, read_only, &control))
        .expect("a writable session");
    std::sync::Arc::new(bookie_gpui::live::WriteSession::from_parts(
        runtime,
        sessions,
        opened.session_id,
    ))
}

fn saved_customer(path: &std::path::Path, id: u32) -> String {
    use bookie_gpui::live::{LiveSession, Origin};
    use bookie_gpui::model::CellText;
    use bookie_gpui::source::ResultSource;
    let live = LiveSession::open(&Origin::File(path.to_path_buf())).expect("open");
    let table = live
        .run_query(&format!("SELECT customer FROM orders WHERE id = {id}"), 10)
        .expect("query");
    table.row(0)[0].display()
}

fn saving_view<'a>(
    cx: &'a mut TestAppContext,
    path: &std::path::Path,
) -> (gpui::Entity<GridView>, &'a mut gpui::VisualTestContext) {
    let table = writable_table(path);
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
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    (view, cx)
}

fn settle_saving(cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
}

fn fixture_copy(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("bookie-gpui-{name}-{}.sqlite", uuid::Uuid::new_v4()));
    std::fs::copy(orders_fixture(), &path).unwrap();
    path
}

fn queue_edit(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, text: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cell_edit
                .edits
                .insert((0, 1), bookie_gpui::model::Cell::Text(text.into()));
            view.edits_changed();
        })
    });
}

#[gpui::test]
fn a_write_the_guard_denies_leaves_the_database_unchanged_and_keeps_the_edit(cx: &mut TestAppContext) {
    let path = fixture_copy("denied");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, true);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
        })
    });
    queue_edit(&view, cx, "Denied");
    cx.simulate_keystrokes("ctrl-s");
    settle_saving(cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.status.starts_with("Could not save") && view.status.contains("read-only"),
            "expected the read-only policy denial: {}",
            view.status
        );
        let (text, is_error) = view.save.banner.clone().expect("an error banner");
        assert!(is_error && text.starts_with("Could not save"), "{text}");
        assert_eq!(view.cell_edit.edits.len(), 1, "the edit is kept so it can be retried");
        assert!(
            view.save.write.is_none(),
            "a failed save drops the cached write session"
        );
    });
    assert_eq!(saved_customer(&path, 1), "Ada", "the denied statement did not run");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_plan_prepared_before_the_edits_changed_is_dropped_and_a_stale_review_cannot_be_confirmed(cx: &mut TestAppContext) {
    let path = fixture_copy("stale");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
        })
    });
    queue_edit(&view, cx, "First");
    cx.simulate_keystrokes("ctrl-s");
    cx.update(|_, cx| view.update(cx, |view, _| view.discard_view_edits()));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.save.pending.is_none(),
            "a plan prepared before a discard is dropped"
        );
        assert!(!view.save.busy);
    });

    queue_edit(&view, cx, "Second");
    cx.simulate_keystrokes("ctrl-s");
    settle_saving(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.save.pending.is_some());
            view.save.generation += 1;
            view.confirm_save(cx);
        })
    });
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.status,
            "The table or the edits changed since the review; press Ctrl+S to review again"
        );
    });
    assert_eq!(saved_customer(&path, 1), "Ada", "nothing was saved from a stale review");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_grid_switch_is_refused_while_a_save_is_running_so_no_other_tabs_edits_are_touched(cx: &mut TestAppContext) {
    let path = fixture_copy("switch");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.save.busy = true;
            view.tabs.open_query();
            view.sync_grid_owner();
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Finish or cancel the save before switching grids.");
        assert!(
            matches!(view.tabs.current(), bookie_gpui::shell::Tab::Table(_)),
            "the table grid stays in front while its save runs"
        );
    });
    std::fs::remove_file(&path).ok();
}

fn order_count(path: &std::path::Path) -> String {
    use bookie_gpui::live::{LiveSession, Origin};
    use bookie_gpui::model::CellText;
    use bookie_gpui::source::ResultSource;
    let live = LiveSession::open(&Origin::File(path.to_path_buf())).expect("open");
    live.run_query("SELECT COUNT(*) FROM orders", 10).expect("count").row(0)[0].display()
}

#[gpui::test]
fn selected_rows_are_reviewed_then_deleted_and_the_table_reloads_without_them(cx: &mut TestAppContext) {
    let path = fixture_copy("delete");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
            view.click_row(2, false);
        })
    });
    assert_eq!(order_count(&path), "6");

    cx.simulate_keystrokes("ctrl-delete");
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let pending = view
            .save
            .pending
            .as_ref()
            .unwrap_or_else(|| panic!("no review: {}", view.status));
        assert_eq!(pending.title, "Delete 1 row(s) from orders?");
        assert_eq!(pending.confirm, "Delete", "the button says what it will do");
        assert!(pending.plan.preview.contains("DELETE"), "{}", pending.plan.preview);
    });
    assert_eq!(order_count(&path), "6", "nothing is deleted before the user confirms");

    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.save.banner, Some(("Deleted 1 row(s)".to_string(), false)));
        assert_eq!(view.source.total(), 5, "the grid reloaded without the deleted row");
        assert!(!view.save.busy);
    });
    assert_eq!(order_count(&path), "5");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn deleting_needs_a_selection_and_no_pending_edits(cx: &mut TestAppContext) {
    let path = fixture_copy("delete-guards");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selection = None;
            view.cell_cursor = None;
            view.request_delete(cx);
        })
    });
    view.read_with(cx, |view, _| assert_eq!(view.status, "Select the rows to delete first"));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            view.request_delete(cx);
        })
    });
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let pending = view
            .save
            .pending
            .as_ref()
            .expect("the cursor row is offered for deletion");
        assert_eq!(pending.title, "Delete 1 row(s) from orders?");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.cancel_save(cx)));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.click_row(0, false);
            view.cell_edit
                .edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("x".into()));
            view.request_delete(cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Save or discard the pending edits before deleting rows");
        assert!(view.save.pending.is_none());
    });
    assert_eq!(order_count(&path), "6");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_cached_write_session_for_another_connection_is_never_used(cx: &mut TestAppContext) {
    let path = fixture_copy("other-connection");
    let (view, cx) = saving_view(cx, &path);
    let wrong = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.save.write = Some((uuid::Uuid::new_v4(), wrong));
        })
    });
    queue_edit(&view, cx, "Wrong target");
    cx.simulate_keystrokes("ctrl-s");
    settle_saving(cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.status.starts_with("Could not open a connection for saving"),
            "the cached session belongs to another connection: {}",
            view.status
        );
    });
    assert_eq!(
        saved_customer(&path, 1),
        "Ada",
        "the other connection's session did not run it"
    );
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn an_edited_cell_is_reviewed_then_saved_and_the_table_shows_the_saved_value(cx: &mut TestAppContext) {
    use bookie_gpui::model::CellText;
    let path = std::env::temp_dir().join(format!("bookie-gpui-save-{}.sqlite", uuid::Uuid::new_v4()));
    std::fs::copy(orders_fixture(), &path).unwrap();
    let table = writable_table(&path);
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
    let settle = |cx: &mut gpui::VisualTestContext| {
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
    };
    settle(cx);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view
                .live
                .as_ref()
                .and_then(|live| live.writable)
                .expect("a writable connection");
            view.save.write = Some((id, write));
            view.cell_edit
                .edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("Ada Lovelace".into()));
        })
    });

    cx.simulate_keystrokes("ctrl-s");
    settle(cx);
    view.read_with(cx, |view, _| {
        let pending = view
            .save
            .pending
            .as_ref()
            .unwrap_or_else(|| panic!("no review shown; status: {}", view.status));
        assert!(pending.plan.preview.contains("UPDATE"), "{}", pending.plan.preview);
        assert!(
            pending.plan.preview.contains("Ada Lovelace"),
            "{}",
            pending.plan.preview
        );
        assert_eq!(
            view.cell_edit.edits.len(),
            1,
            "nothing is saved before the user confirms"
        );
    });

    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.save.banner, Some(("Saved 1 change(s)".to_string(), false)));
        assert!(view.source.origin().contains("editable"), "{}", view.source.origin());
        assert!(view.cell_edit.edits.is_empty());
        assert!(view.save.pending.is_none());
        assert_eq!(
            view.source.row(0)[1].display(),
            "Ada Lovelace",
            "the grid shows the saved value"
        );
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn cancelling_the_review_keeps_the_edit_and_a_read_only_table_cannot_start_a_save(cx: &mut TestAppContext) {
    let path = std::env::temp_dir().join(format!("bookie-gpui-cancel-{}.sqlite", uuid::Uuid::new_v4()));
    std::fs::copy(orders_fixture(), &path).unwrap();
    let table = writable_table(&path);
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
    let settle = |cx: &mut gpui::VisualTestContext| {
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
    };
    settle(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cell_edit
                .edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("Changed".into()));
        })
    });
    cx.simulate_keystrokes("ctrl-s");
    settle(cx);
    view.read_with(cx, |view, _| assert!(view.save.pending.is_some(), "{}", view.status));
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 2, window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_none(), "no new edit while the review is open");
        assert_eq!(view.status, "Save or cancel the review before editing more cells");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.cancel_save(cx)));
    view.read_with(cx, |view, _| {
        assert!(view.save.pending.is_none());
        assert_eq!(view.cell_edit.edits.len(), 1, "cancelling keeps the pending edit");
    });
    cx.simulate_keystrokes("ctrl-s");
    settle(cx);
    cx.update(|_, cx| view.update(cx, |view, _| view.discard_view_edits()));
    view.read_with(cx, |view, _| {
        assert!(
            view.save.pending.is_none(),
            "discarding the edits closes a stale review"
        );
        assert!(view.cell_edit.edits.is_empty());
    });
    std::fs::remove_file(&path).ok();

    let (read_only, cx) = open_live(cx);
    cx.update(|_, cx| {
        read_only.update(cx, |view, cx| {
            view.cell_edit
                .edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("x".into()));
            view.request_save(cx);
        })
    });
    read_only.read_with(cx, |view, _| {
        assert_eq!(
            view.status,
            "Saving needs a table from a saved connection that is not read-only"
        );
        assert!(view.save.pending.is_none());
    });
}

#[gpui::test]
fn deleting_is_refused_in_compare_mode_where_rows_are_not_dataset_rows(cx: &mut TestAppContext) {
    let path = fixture_copy("delete-compare");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.compare = Some(std::sync::Arc::new(bookie_gpui::model::Diff {
                summary: bookie_gpui::model::DiffSummary::default(),
                rows: Vec::new(),
                capped: false,
            }));
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            view.request_delete(cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Leave compare mode before deleting rows");
        assert!(view.save.pending.is_none());
    });
    assert_eq!(order_count(&path), "6");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn under_a_filter_the_row_you_see_selected_is_the_row_that_is_deleted(cx: &mut TestAppContext) {
    let path = fixture_copy("delete-filtered");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
            view.set_mode(crate::Mode::Filtered {
                kept: std::sync::Arc::new(vec![1, 3, 4]),
                label: "test".into(),
            });
            view.click_row(1, false);
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.request_delete(cx)));
    settle_saving(cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    assert_eq!(
        remaining_ids(&path),
        "1,2,3,5,6",
        "the second visible row is source row 3 (id 4), and only that row is deleted"
    );
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_delete_the_guard_denies_leaves_every_row_in_place(cx: &mut TestAppContext) {
    let path = fixture_copy("delete-denied");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, true);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
            view.click_row(0, false);
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.request_delete(cx)));
    settle_saving(cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.status.starts_with("Could not save") && view.status.contains("read-only"),
            "{}",
            view.status
        );
    });
    assert_eq!(remaining_ids(&path), "1,2,3,4,5,6");
    std::fs::remove_file(&path).ok();
}

fn remaining_ids(path: &std::path::Path) -> String {
    use bookie_gpui::live::{LiveSession, Origin};
    use bookie_gpui::model::CellText;
    use bookie_gpui::source::ResultSource;
    let live = LiveSession::open(&Origin::File(path.to_path_buf())).expect("open");
    live.run_query("SELECT group_concat(id) FROM (SELECT id FROM orders ORDER BY id)", 10)
        .expect("ids")
        .row(0)[0]
        .display()
}

fn fill_insert_field(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, name: &str, text: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let field = view
                .insert
                .fields
                .iter()
                .find(|field| field.name == name)
                .unwrap_or_else(|| panic!("no field named {name}"));
            field.input.update(cx, |input, cx| input.set_content(text, cx));
        })
    });
}

#[gpui::test]
fn a_new_row_is_filled_in_reviewed_then_inserted_and_the_table_shows_it(cx: &mut TestAppContext) {
    let path = fixture_copy("insert");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
        })
    });
    cx.simulate_keystrokes("ctrl-n");
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let names: Vec<&str> = view.insert.fields.iter().map(|field| field.name.as_str()).collect();
        assert_eq!(
            names,
            ["customer", "total", "paid", "note"],
            "the key the database assigns is not asked for"
        );
        assert!(view.insert.open);
    });

    fill_insert_field(&view, cx, "customer", "Zed");
    fill_insert_field(&view, cx, "total", "4.5");
    fill_insert_field(&view, cx, "paid", "1");
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_insert(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let pending = view
            .save
            .pending
            .as_ref()
            .unwrap_or_else(|| panic!("no review: {}", view.status));
        assert_eq!(pending.title, "Insert 1 row into orders?");
        assert_eq!(pending.confirm, "Insert");
        assert!(pending.plan.preview.contains("INSERT"), "{}", pending.plan.preview);
        assert!(pending.plan.preview.contains("'Zed'"), "{}", pending.plan.preview);
    });
    assert_eq!(order_count(&path), "6", "nothing is inserted before the user confirms");

    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.save.banner, Some(("Inserted 1 row".to_string(), false)));
        assert!(!view.insert.open, "the form closes once the row is saved");
        assert_eq!(view.source.total(), 7, "the grid reloaded with the new row");
    });
    assert_eq!(order_count(&path), "7");
    assert_eq!(saved_customer(&path, 7), "Zed");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_value_that_does_not_fit_stops_the_review_keeps_the_form_and_cells_stay_locked(cx: &mut TestAppContext) {
    let path = fixture_copy("insert-invalid");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_insert_form(cx)));
    settle_saving(cx);
    fill_insert_field(&view, cx, "total", "not a number");
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_insert(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(view.status.starts_with("Cannot save: total"), "{}", view.status);
        assert!(view.save.pending.is_none());
        assert!(view.insert.open, "the form stays so the value can be fixed");
    });

    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_none());
        assert_eq!(view.status, "Finish or cancel the new row before editing cells");
    });

    cx.update(|_, cx| view.update(cx, |view, cx| view.cancel_insert_form(cx)));
    view.read_with(cx, |view, _| {
        assert!(!view.insert.open && view.insert.fields.is_empty())
    });
    assert_eq!(order_count(&path), "6");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_column_read_that_finishes_after_the_table_changed_does_not_open_a_form_for_the_old_table(cx: &mut TestAppContext) {
    let path = fixture_copy("insert-race");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_insert_form(cx)));
    cx.update(|_, cx| view.update(cx, |view, _| view.edits_changed()));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.insert.open, "a form for a table the user has left is never shown");
        assert!(!view.insert.loading);
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_denied_insert_keeps_the_form_and_what_was_typed_and_leaves_the_table_alone(cx: &mut TestAppContext) {
    let path = fixture_copy("insert-denied");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, true);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_insert_form(cx)));
    settle_saving(cx);
    fill_insert_field(&view, cx, "customer", "Denied");
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_insert(cx)));
    settle_saving(cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, cx| {
        assert!(
            view.status.starts_with("Could not save") && view.status.contains("read-only"),
            "{}",
            view.status
        );
        assert!(view.insert.open, "the form stays so the user can try again");
        let typed = view
            .insert
            .fields
            .iter()
            .find(|field| field.name == "customer")
            .expect("field");
        assert_eq!(typed.input.read(cx).content(), "Denied", "what was typed is kept");
    });
    assert_eq!(order_count(&path), "6");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn saving_and_deleting_are_refused_while_the_new_row_form_is_open(cx: &mut TestAppContext) {
    let path = fixture_copy("insert-locks");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_insert_form(cx)));
    settle_saving(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.click_row(0, false);
            view.request_delete(cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Finish or cancel the new row first");
        assert!(view.save.pending.is_none());
    });
    std::fs::remove_file(&path).ok();
}
