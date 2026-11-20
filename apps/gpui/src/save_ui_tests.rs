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

fn read_when_written(cx: &mut gpui::VisualTestContext, path: &std::path::Path, ready: impl Fn(&str) -> bool) -> String {
    let mut text = String::new();
    for _ in 0..200 {
        settle_saving(cx);
        text = std::fs::read_to_string(path).unwrap_or_default();
        if ready(&text) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    text
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
        assert_eq!(
            pending.note,
            Some("Rows that depend on these (foreign keys, triggers) may be affected too."),
            "the note must not claim approvals are unavailable now that the prompt exists"
        );
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
        assert_eq!(view.status, "Finish or cancel the open form before editing cells");
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
        assert_eq!(view.status, "Finish or cancel the open form first");
        assert!(view.save.pending.is_none());
    });
    std::fs::remove_file(&path).ok();
}

fn csv_file(name: &str, text: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("bookie-gpui-{name}-{}.csv", uuid::Uuid::new_v4()));
    std::fs::write(&path, text).unwrap();
    path
}

fn set_import_path(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, path: &std::path::Path) {
    let text = path.display().to_string();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let input = view.import.input.clone().expect("the path field exists once the bar opened");
            input.update(cx, |input, cx| input.set_content(&text, cx));
        })
    });
}

#[gpui::test]
fn a_csv_file_is_reviewed_then_imported_in_one_transaction_and_the_table_shows_the_rows(cx: &mut TestAppContext) {
    let path = fixture_copy("import");
    let file = csv_file("rows", "customer,total,ignored\nCsvOne,1.5,x\nCsvTwo,2.5,y\n");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_import_bar(cx)));
    view.read_with(cx, |view, _| assert!(view.import.open));
    set_import_path(&view, cx, &file);
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_import(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let pending = view
            .save
            .pending
            .as_ref()
            .unwrap_or_else(|| panic!("no review: {}", view.status));
        assert_eq!(pending.title, "Import 2 row(s) into orders?");
        assert_eq!(pending.confirm, "Import");
        assert!(pending.plan.preview.contains("customer, total"), "{}", pending.plan.preview);
        assert!(pending.plan.preview.contains("ignored"), "the unmatched field is reported: {}", pending.plan.preview);
    });
    assert_eq!(order_count(&path), "6", "nothing is imported before the user confirms");

    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.save.banner, Some(("Imported 2 row(s)".to_string(), false)));
        assert!(!view.import.open, "the bar closes once the rows are saved");
        assert_eq!(view.source.total(), 8);
    });
    assert_eq!(order_count(&path), "8");
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&file).ok();
}

#[gpui::test]
fn a_missing_or_unmatched_file_stops_the_review_keeps_the_bar_and_locks_editing(cx: &mut TestAppContext) {
    let path = fixture_copy("import-bad");
    let unmatched = csv_file("unmatched", "alpha,beta\n1,2\n");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_import_bar(cx)));
    set_import_path(&view, cx, std::path::Path::new("/no/such/file.csv"));
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_import(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(view.status.starts_with("Cannot save:"), "{}", view.status);
        assert!(view.save.pending.is_none());
        assert!(view.import.open, "the bar stays so the path can be fixed");
    });

    set_import_path(&view, cx, &unmatched);
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_import(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(view.status.contains("no column of this table matches"), "{}", view.status);
    });

    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_none());
        assert_eq!(view.status, "Finish or cancel the open form before editing cells");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.cancel_import_bar(cx)));
    view.read_with(cx, |view, _| assert!(!view.import.open));
    assert_eq!(order_count(&path), "6");
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&unmatched).ok();
}

fn set_structure_text(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, text: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let input = view.structure.input.clone().expect("the structure bar has an input");
            input.update(cx, |input, cx| input.set_content(text, cx));
        })
    });
}

fn scalar_text(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, sql: &str) -> String {
    use bookie_gpui::model::CellText;
    use bookie_gpui::source::ResultSource;
    view.read_with(cx, |view, _| {
        let live = view.live.as_ref().expect("a live session");
        live.run_query(sql, 10).expect("query").row(0)[0].display()
    })
}

fn table_columns_of(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext) -> String {
    scalar_text(view, cx, "SELECT COALESCE(group_concat(name), '') FROM pragma_table_info('orders')")
}

#[gpui::test]
fn a_column_is_added_renamed_and_dropped_only_after_each_change_is_reviewed(cx: &mut TestAppContext) {
    use bookie_app_services::structure_change_plan::StructureChangeKind::{AddColumn, DropColumn, RenameColumn};
    let path = fixture_copy("structure");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
        })
    });
    let before = table_columns_of(&view, cx);
    let mut prior = before.clone();
    for (kind, text, expected) in [
        (AddColumn, "flag INTEGER", "flag"),
        (RenameColumn, "flag marker", "marker"),
        (DropColumn, "marker", ""),
    ] {
        cx.update(|_, cx| view.update(cx, |view, cx| view.open_structure_bar(kind, cx)));
        view.read_with(cx, |view, _| assert!(view.structure.open));
        set_structure_text(&view, cx, text);
        cx.update(|_, cx| view.update(cx, |view, cx| view.review_structure(cx)));
        settle_saving(cx);
        view.read_with(cx, |view, _| {
            let pending = view.save.pending.as_ref().unwrap_or_else(|| panic!("no review: {}", view.status));
            assert!(pending.title.contains(text.split_whitespace().next().unwrap()), "{}", pending.title);
            assert_ne!(pending.confirm, "Apply", "the button says what it does");
            assert!(pending.plan.preview.contains("ALTER TABLE"), "{}", pending.plan.preview);
        });
        assert_eq!(table_columns_of(&view, cx), prior, "nothing changes before the user confirms");
        cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
        settle_saving(cx);
        view.read_with(cx, |view, _| {
            assert!(!view.structure.open, "the bar closes once applied");
            assert!(view.save.banner.as_ref().is_some_and(|(_, error)| !error), "{:?}", view.save.banner);
        });
        let columns = table_columns_of(&view, cx);
        prior = columns.clone();
        if expected.is_empty() {
            assert_eq!(columns, before);
        } else {
            assert!(columns.contains(expected), "{columns}");
        }
    }
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_bad_structure_request_is_refused_before_any_review_and_keeps_the_bar(cx: &mut TestAppContext) {
    use bookie_app_services::structure_change_plan::StructureChangeKind::DropColumn;
    let path = fixture_copy("structure-bad");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_structure_bar(DropColumn, cx)));
    set_structure_text(&view, cx, "no_such_column");
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_structure(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(view.save.pending.is_none());
        assert!(view.structure.open);
        assert!(view.status.contains("no column named"), "{}", view.status);
        assert!(view.form_open(), "editing stays locked while the bar is open");
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_save_that_finishes_after_the_reload_was_cancelled_still_frees_every_write_feature(cx: &mut TestAppContext) {
    let path = fixture_copy("busy");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.busy = true;
            view.finish_save(id, "Saved".into(), Ok((write, Ok(vec![1]))), cx);
            view.invalidate_background();
        })
    });
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.save.busy, "the write is done, so the save is no longer running: {}", view.status);
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn opening_a_table_or_running_a_query_is_refused_while_a_save_is_writing(cx: &mut TestAppContext) {
    let path = fixture_copy("busy-refuse");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.save.busy = true;
            assert!(view.reject_view_edits());
            view.run_sql("SELECT 1".into(), cx);
            assert!(view.status.contains("A save is running"), "{}", view.status);
        })
    });
    std::fs::remove_file(&path).ok();
}

fn orders_indexes_of(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext) -> String {
    scalar_text(
        view,
        cx,
        "SELECT COALESCE(group_concat(name), '') FROM pragma_index_list('orders') WHERE origin = 'c'",
    )
}

#[gpui::test]
fn a_unique_index_is_reviewed_created_then_dropped_by_its_live_name_only_on_confirm(cx: &mut TestAppContext) {
    use bookie_app_services::structure_change_plan::StructureChangeKind::{AddIndex, DropIndex};
    let path = fixture_copy("indexes");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_structure_bar(AddIndex, cx)));
    set_structure_text(&view, cx, "unique by_customer id, customer");
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_structure(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let pending = view.save.pending.as_ref().unwrap_or_else(|| panic!("no review: {}", view.status));
        assert_eq!(pending.title, "Create unique index by_customer on orders?");
        assert!(pending.plan.preview.contains("CREATE UNIQUE INDEX"), "{}", pending.plan.preview);
    });
    assert_eq!(orders_indexes_of(&view, cx), "", "nothing is created before the user confirms");
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    assert_eq!(orders_indexes_of(&view, cx), "by_customer");

    cx.update(|_, cx| view.update(cx, |view, cx| view.open_structure_bar(DropIndex, cx)));
    set_structure_text(&view, cx, "no_such_index");
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_structure(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(view.save.pending.is_none());
        assert!(view.status.contains("no index named"), "{}", view.status);
    });
    set_structure_text(&view, cx, "BY_CUSTOMER");
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_structure(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let pending = view.save.pending.as_ref().unwrap_or_else(|| panic!("no review: {}", view.status));
        assert_eq!(pending.title, "Drop index BY_CUSTOMER on orders?");
        assert!(pending.plan.preview.contains("by_customer"), "{}", pending.plan.preview);
    });
    assert_eq!(orders_indexes_of(&view, cx), "by_customer", "nothing is dropped before the user confirms");
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    assert_eq!(orders_indexes_of(&view, cx), "");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn the_structure_panel_lists_columns_indexes_and_keys_of_the_open_table_and_closes_on_request(cx: &mut TestAppContext) {
    let path = fixture_copy("structure-info");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_structure(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let text = view.structure.info.as_deref().unwrap_or_else(|| panic!("no panel: {}", view.status));
        assert!(text.starts_with("Columns ("), "{text}");
        assert!(text.contains("customer"), "{text}");
        assert!(text.contains("Indexes (") && text.contains("Foreign keys ("), "{text}");
        assert!(!view.structure_info_enabled(), "one panel at a time");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_structure_bar(bookie_app_services::structure_change_plan::StructureChangeKind::AddColumn, cx)));
    view.read_with(cx, |view, _| assert!(view.structure.info.is_none(), "a change bar replaces the panel"));
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn only_a_table_rename_asks_for_a_reconnect_after_it_is_applied(cx: &mut TestAppContext) {
    use bookie_app_services::structure_change_plan::StructureChangeKind::{AddColumn, RenameTable};
    let path = fixture_copy("rename-flag");
    let (view, cx) = saving_view(cx, &path);
    for (kind, text, expected) in [(RenameTable, "invoices", true), (AddColumn, "flag INTEGER", false)] {
        cx.update(|_, cx| view.update(cx, |view, cx| view.open_structure_bar(kind, cx)));
        set_structure_text(&view, cx, text);
        cx.update(|_, cx| view.update(cx, |view, cx| view.review_structure(cx)));
        settle_saving(cx);
        view.read_with(cx, |view, _| {
            let pending = view.save.pending.as_ref().unwrap_or_else(|| panic!("no review: {}", view.status));
            assert_eq!(pending.reconnect.is_some(), expected, "{}", pending.title);
            assert!(!expected || pending.reconnect.as_ref().is_some_and(|table| table.name == "invoices"));
        });
        cx.update(|_, cx| view.update(cx, |view, cx| view.cancel_save(cx)));
        cx.update(|_, cx| view.update(cx, |view, cx| view.cancel_structure_bar(cx)));
    }
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn resolving_an_outcome_is_offered_for_a_writable_connection_and_not_while_the_panel_is_open(cx: &mut TestAppContext) {
    let path = fixture_copy("resolve-offer");
    let (view, cx) = saving_view(cx, &path);
    view.read_with(cx, |view, _| {
        assert!(view.resolve_enabled());
        assert!(view.resolve_panel_is_closed());
    });
    cx.update(|_, cx| view.update(cx, |view, _| view.resolve.open = true));
    view.read_with(cx, |view, _| assert!(!view.resolve_enabled()));
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_blocked_save_points_to_the_resolve_action_and_other_errors_are_left_alone() {
    let blocked = "Blocked by policy: governed writes are disabled because a prior audit outcome could not be persisted";
    assert!(crate::save_ui::with_resolve_hint(blocked).contains("Resolve unknown write outcome"));
    assert_eq!(crate::save_ui::with_resolve_hint("duplicate key"), "duplicate key");
}

fn set_export_path(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, path: &std::path::Path) {
    let text = path.display().to_string();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let input = view.export.file.input.clone().expect("the path field exists once the bar opened");
            input.update(cx, |input, cx| input.set_content(&text, cx));
        })
    });
}

#[gpui::test]
fn the_page_is_written_to_a_file_in_the_format_of_its_extension_and_never_over_an_existing_file(cx: &mut TestAppContext) {
    let path = fixture_copy("export-file");
    let (view, cx) = saving_view(cx, &path);
    let directory = std::env::temp_dir().join(format!("bookie-export-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_export_file_bar(cx)));
    view.read_with(cx, |view, _| assert!(view.export.file.open));

    let jsonl = directory.join("rows.jsonl");
    set_export_path(&view, cx, &jsonl);
    cx.update(|_, cx| view.update(cx, |view, cx| view.write_export_file(cx)));
    let written = read_when_written(cx, &jsonl, |text| text.lines().count() >= 6);
    assert_eq!(written.lines().count(), 6, "{written}");
    assert!(written.lines().next().unwrap().starts_with('{'), "{written}");
    view.read_with(cx, |view, _| assert!(!view.export.file.open && view.status.starts_with("Wrote 6 row(s)")));

    cx.update(|_, cx| view.update(cx, |view, cx| view.open_export_file_bar(cx)));
    let sql = directory.join("rows.sql");
    set_export_path(&view, cx, &sql);
    cx.update(|_, cx| view.update(cx, |view, cx| view.write_export_file(cx)));
    let inserts = read_when_written(cx, &sql, |text| text.contains("INSERT INTO"));
    assert!(inserts.contains("INSERT INTO"), "a table exports as SQL inserts");

    cx.update(|_, cx| view.update(cx, |view, cx| view.open_export_file_bar(cx)));
    set_export_path(&view, cx, &jsonl);
    cx.update(|_, cx| view.update(cx, |view, cx| view.write_export_file(cx)));
    view.read_with(cx, |view, _| {
        assert!(view.status.contains("already exists"), "{}", view.status);
        assert!(view.export.file.open, "the bar stays so another name can be typed");
    });
    set_export_path(&view, cx, &directory.join("rows.txt"));
    cx.update(|_, cx| view.update(cx, |view, cx| view.write_export_file(cx)));
    view.read_with(cx, |view, _| assert!(view.status.contains("ending in"), "{}", view.status));
    assert!(!directory.join("rows.txt").exists());
    std::fs::remove_dir_all(&directory).ok();
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn dropping_the_table_needs_its_exact_name_then_reviews_the_statement_and_reconnects(cx: &mut TestAppContext) {
    use bookie_app_services::structure_change_plan::StructureChangeKind::DropTable;
    let path = fixture_copy("drop-table");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_structure_bar(DropTable, cx)));
    set_structure_text(&view, cx, "order");
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_structure(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(view.save.pending.is_none());
        assert!(view.status.contains("exactly"), "{}", view.status);
        assert!(view.structure.open, "the bar stays so the name can be fixed");
    });
    set_structure_text(&view, cx, "orders");
    cx.update(|_, cx| view.update(cx, |view, cx| view.review_structure(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let pending = view.save.pending.as_ref().unwrap_or_else(|| panic!("no review: {}", view.status));
        assert_eq!(pending.confirm, "Drop table");
        assert!(pending.plan.preview.contains("DROP TABLE"), "{}", pending.plan.preview);
        assert!(pending.reconnect.is_some(), "the open table is gone afterwards");
    });
    assert_eq!(order_count(&path), "6", "nothing is dropped before the user confirms");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn show_create_table_fills_the_panel_with_a_create_statement(cx: &mut TestAppContext) {
    let path = fixture_copy("create-script");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_create_table(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let text = view.structure.info.as_deref().unwrap_or_else(|| panic!("no panel: {}", view.status));
        assert!(text.starts_with("CREATE TABLE"), "{text}");
        assert!(text.contains("customer"), "{text}");
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn one_value_is_staged_on_every_selected_row_of_the_active_column(cx: &mut TestAppContext) {
    let path = fixture_copy("set-cells");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_set_cells_bar(cx)));
    view.read_with(cx, |view, _| assert!(!view.cell_edit.set.open, "nothing selected yet"));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selection = Some(crate::Selection { anchor: 0, focus: 2 });
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            view.open_set_cells_bar(cx);
            let input = view
                .cell_edit
                .set
                .input
                .clone()
                .expect("the field exists once the bar opened");
            input.update(cx, |input, cx| input.set_content("bulk", cx));
            view.apply_set_cells(false, cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert!(!view.cell_edit.set.open, "{}", view.status);
        assert_eq!(view.cell_edit.edits.len(), 3, "{}", view.status);
        assert!(
            view.cell_edit
                .edits
                .values()
                .all(|cell| *cell == bookie_gpui::model::Cell::Text("bulk".into()))
        );
        assert!((0..3).all(|row| view.cell_edit.edits.contains_key(&(row, 1))));
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn the_selected_rows_are_summarised_per_numeric_column(cx: &mut TestAppContext) {
    let path = fixture_copy("selection-stats");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_selection_stats(cx)));
    view.read_with(cx, |view, _| assert!(!view.selection_stats_enabled()));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selection = Some(crate::Selection { anchor: 0, focus: 2 });
            view.show_selection_stats(cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert!(view.status.contains("id: count 3, min 1, max 3, sum 6"), "{}", view.status);
        assert!(!view.status.contains("customer"), "text columns are not summarised: {}", view.status);
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn health_reports_are_offered_only_on_postgres(cx: &mut TestAppContext) {
    let path = fixture_copy("health-sqlite");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.health_supported());
            assert!(view.health_actions().is_empty());
            view.show_health_report(bookie_core::HealthReport::TableVacuum, cx);
        })
    });
    view.read_with(cx, |view, _| assert!(view.status.contains("PostgreSQL"), "{}", view.status));
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn searching_every_table_for_a_value_reports_the_matching_rows_in_messages(cx: &mut TestAppContext) {
    let path = fixture_copy("value-search");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.open_value_search_bar(cx);
            let input = view.search.values.input.clone().expect("the field exists once the bar opened");
            input.update(cx, |input, cx| input.set_content("Ada", cx));
            view.run_value_search(cx);
        })
    });
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let lines: Vec<&str> = view.panel.messages.entries().iter().map(|entry| entry.text.as_str()).collect();
        assert!(lines.iter().any(|line| line.contains("orders") && line.contains("matching row(s)")), "{lines:?}");
        assert!(!view.search.values.open);
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn the_schema_diagram_is_written_to_a_new_file_in_the_format_of_its_extension(cx: &mut TestAppContext) {
    let path = fixture_copy("schema-diagram");
    let (view, cx) = saving_view(cx, &path);
    let directory = std::env::temp_dir().join(format!("bookie-diagram-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let target = directory.join("schema.mmd");
    let type_path = |view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, file: &std::path::Path| {
        let text = file.display().to_string();
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.open_schema_diagram_bar(cx);
                let input = view.search.diagram.input.clone().expect("the field exists once the bar opened");
                input.update(cx, |input, cx| input.set_content(&text, cx));
                view.write_schema_diagram(cx);
            })
        });
    };
    type_path(&view, cx, &target);
    let written = read_when_written(cx, &target, |text| text.starts_with("erDiagram"));
    assert!(written.starts_with("erDiagram") && written.contains("orders") && written.contains("customers"), "{written}");
    view.read_with(cx, |view, _| assert!(!view.search.diagram.open, "{}", view.status));

    type_path(&view, cx, &target);
    view.read_with(cx, |view, _| assert!(view.status.contains("already exists"), "{}", view.status));
    type_path(&view, cx, &directory.join("schema.png"));
    view.read_with(cx, |view, _| assert!(view.status.contains(".mmd"), "{}", view.status));
    std::fs::remove_dir_all(&directory).ok();
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn pasting_clipboard_rows_stages_them_down_the_column_from_the_active_cell(cx: &mut TestAppContext) {
    let path = fixture_copy("paste-cells");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("Zed\nYan".into()));
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            view.paste_cells(cx);
        })
    });
    view.read_with(cx, |view, _| {
        let edits = &view.cell_edit.edits;
        assert_eq!(edits.get(&(0, 1)), Some(&bookie_gpui::model::Cell::Text("Zed".into())), "{}", view.status);
        assert_eq!(edits.get(&(1, 1)), Some(&bookie_gpui::model::Cell::Text("Yan".into())));
        assert_eq!(edits.len(), 2);
    });
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("a\tb\tc\td\te\tf".into()));
        view.update(cx, |view, cx| {
            view.cell_edit.edits.clear();
            view.paste_cells(cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert!(view.status.starts_with("Nothing was pasted"), "{}", view.status);
        assert!(view.cell_edit.edits.is_empty());
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_structured_rule_filters_the_table_into_a_result(cx: &mut TestAppContext) {
    let path = fixture_copy("filter-rule");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            view.open_filter_rule_bar(cx);
            let column = view.search.rule.column.clone().expect("column field");
            assert_eq!(column.read(cx).content().to_string(), "customer", "the active column is prefilled");
            let value = view.search.rule.value.clone().expect("value field");
            value.update(cx, |input, cx| input.set_content("Ada", cx));
            view.run_filter_rule(cx);
        })
    });
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.search.rule.open);
        assert_eq!(view.run_tabs.slots.len(), 1, "{}", view.status);
        assert_eq!(view.run_tabs.slots[0].run.rows, 2, "two orders belong to Ada");
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn the_schema_diagram_opens_in_its_own_tab_with_one_card_per_table(cx: &mut TestAppContext) {
    let path = fixture_copy("diagram-tab");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_schema_diagram(cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(*view.tabs.current(), bookie_gpui::shell::Tab::Diagram, "{}", view.status);
        let diagram = view.search.diagram.view.as_ref().expect("a diagram was built");
        assert_eq!(diagram.scene.cards.len(), 2, "orders and customers");
        assert!(!diagram.needs_fit, "the first frame fits the diagram to the window");
        assert!(diagram.zoom > 0.0 && diagram.zoom <= 1.0, "{}", diagram.zoom);
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn backups_are_offered_only_on_postgres_and_a_refused_bar_stays_closed(cx: &mut TestAppContext) {
    let path = fixture_copy("backup-sqlite");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.backup_actions().iter().all(|action| !action.enabled));
            view.open_backup_bar(false, cx);
            view.open_backup_bar(true, cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert!(!view.search.backup.open);
        assert!(view.status.contains("PostgreSQL"), "{}", view.status);
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn refreshing_reloads_the_table_but_never_throws_pending_edits_away(cx: &mut TestAppContext) {
    let path = fixture_copy("refresh-table");
    let (view, cx) = saving_view(cx, &path);
    queue_edit(&view, cx, "pending");
    cx.update(|_, cx| view.update(cx, |view, cx| view.refresh_table(cx)));
    view.read_with(cx, |view, _| {
        assert!(view.status.contains("pending edits"), "{}", view.status);
        assert_eq!(view.cell_edit.edits.len(), 1, "the edit is kept");
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.clear();
            view.refresh_table(cx);
        })
    });
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Refreshed the table");
        assert_eq!(view.dataset_count(), 6, "the six fixture rows are loaded again");
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_sql_file_opens_into_the_editor_and_the_editor_text_saves_to_a_new_file(cx: &mut TestAppContext) {
    let path = fixture_copy("sql-files");
    let (view, cx) = saving_view(cx, &path);
    let directory = std::env::temp_dir().join(format!("bookie-sqlfiles-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let script = directory.join("in.sql");
    std::fs::write(&script, "SELECT 'from file';\n").unwrap();

    let type_path = |view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, saving: bool, file: &std::path::Path| {
        let text = file.display().to_string();
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.open_sql_file_bar(saving, cx);
                let input = view.search.sql_file.input.clone().expect("the path field");
                input.update(cx, |input, cx| input.set_content(&text, cx));
                view.run_sql_file(cx);
            })
        });
        settle_saving(cx);
    };
    type_path(&view, cx, false, &script);
    cx.update(|window, cx| view.update(cx, |view, cx| view.apply_opened_sql(window, cx)));
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sql_input.read(cx).value().to_string(), "SELECT 'from file';\n", "{}", view.status);
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input.update(cx, |input, cx| input.set_value("SELECT 'edited';", window, cx));
        })
    });
    let saved = directory.join("out");
    type_path(&view, cx, true, &saved);
    let out = directory.join("out.sql");
    assert_eq!(read_when_written(cx, &out, |text| !text.is_empty()), "SELECT 'edited';");
    type_path(&view, cx, true, &directory.join("out.sql"));
    view.read_with(cx, |view, _| assert!(view.status.contains("already exists"), "{}", view.status));
    std::fs::remove_dir_all(&directory).ok();
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_closed_tab_can_be_reopened(cx: &mut TestAppContext) {
    let path = fixture_copy("reopen-tab");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.closed_tab_actions().iter().all(|action| !action.enabled));
            view.tabs.open_query();
            let index = view.tabs.active;
            view.close_tab(index);
            assert!(view.closed_tab_actions().iter().all(|action| action.enabled));
            assert_ne!(*view.tabs.current(), bookie_gpui::shell::Tab::Query);
            cx.notify();
        })
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.reopen_closed_tab(window, cx)));
    view.read_with(cx, |view, _| {
        assert_eq!(*view.tabs.current(), bookie_gpui::shell::Tab::Query);
        assert!(view.closed_tab_actions().iter().all(|action| !action.enabled), "the entry is used up");
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn closing_a_table_tab_loads_the_table_that_becomes_active(cx: &mut TestAppContext) {
    let path = fixture_copy("close-loads");
    let (view, cx) = saving_view(cx, &path);
    let customers = TableInfo {
        schema: None,
        name: "customers".into(),
    };
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_table_named(&customers, window, cx)));
    settle_saving(cx);
    view.read_with(cx, |view, _| assert_eq!(view.catalog.grid_table, customers, "{}", view.status));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let index = view.tabs.active;
            view.close_tab(index);
            view.show_current_table(window, cx);
        })
    });
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let bookie_gpui::shell::Tab::Table(shown) = view.tabs.current().clone() else {
            panic!("a table tab is active");
        };
        assert_eq!(shown.name, "orders");
        assert_eq!(view.catalog.grid_table, shown, "the grid now holds the table whose tab is shown");
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn exporting_every_table_is_refused_without_a_consistent_snapshot(cx: &mut TestAppContext) {
    let path = fixture_copy("tables-export");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.tables_export_actions().iter().all(|action| !action.enabled));
            view.open_tables_export_bar(cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert!(!view.search.tables_export.open);
        assert!(view.status.contains("consistent snapshot"), "{}", view.status);
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn reconnecting_refreshes_the_table_list_and_never_with_pending_edits(cx: &mut TestAppContext) {
    let path = fixture_copy("reconnect");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.reconnect_actions().iter().all(|action| !action.enabled), "no origin is known yet");
            view.search.origin = Some(bookie_gpui::live::Origin::File(path.clone()));
            assert!(view.reconnect_actions().iter().all(|action| action.enabled));
        })
    });
    queue_edit(&view, cx, "pending");
    cx.update(|_, cx| view.update(cx, |view, cx| view.reconnect_now(cx)));
    view.read_with(cx, |view, _| {
        assert!(view.status.contains("pending edits"), "{}", view.status);
        assert_eq!(view.cell_edit.edits.len(), 1);
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.clear();
            view.tabs.open_query();
            view.reconnect_now(cx);
        })
    });
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.status, "Connected", "{}", view.status);
        assert_eq!(view.tabs.items.len(), 2, "the extra tab was closed by the reconnect");
        assert_eq!(view.catalog.tables.len(), 2);
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn disconnecting_is_refused_while_edits_are_pending_and_offered_only_when_connected(cx: &mut TestAppContext) {
    let path = fixture_copy("disconnect");
    let (view, cx) = saving_view(cx, &path);
    view.read_with(cx, |view, _| {
        assert!(view.disconnect_actions().iter().all(|action| action.enabled), "a connection is open");
    });
    queue_edit(&view, cx, "pending");
    cx.update(|_, cx| view.update(cx, |view, cx| view.disconnect(cx)));
    view.read_with(cx, |view, _| {
        assert!(view.status.contains("pending edits"), "{}", view.status);
        assert_eq!(view.cell_edit.edits.len(), 1, "the edit is kept and the app is still running");
    });
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_decision_made_elsewhere_is_reported_as_such_and_other_failures_keep_their_text() {
    use bookie_app_services::unknown_outcomes::{OutcomeDecision, UnknownOutcomeError};
    let status = |result: Result<(), UnknownOutcomeError>, decision| crate::resolve_ui::decision_status(&result, decision);
    assert_eq!(
        status(Err(UnknownOutcomeError::NoLongerUnresolved), OutcomeDecision::Committed),
        "That write was already decided elsewhere; the list was reloaded"
    );
    assert_eq!(
        status(Err(UnknownOutcomeError::Rejected("a reason is required".into())), OutcomeDecision::Committed),
        "Could not record the decision: a reason is required"
    );
    assert!(status(Ok(()), OutcomeDecision::StillUnknown).contains("still unknown"));
    assert!(status(Ok(()), OutcomeDecision::RolledBack).contains("accepts writes again"));
}
