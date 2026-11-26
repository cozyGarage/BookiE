use std::time::Instant;

use gpui::TestAppContext;
use gpui_kit::test::TestWindowExt;

use crate::GridView;
use crate::live_ui_tests::{SqliteFixture, sqlite_fixture};
use bookie_gpui::source::ResultSource;

fn orders_with_views() -> SqliteFixture {
    sqlite_fixture(&[
        "CREATE TABLE orders (id INTEGER PRIMARY KEY, customer TEXT)",
        "CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT)",
        "INSERT INTO orders VALUES (1, 'Ada'), (2, 'Grace')",
        "INSERT INTO customers VALUES (1, 'Ada Lovelace'), (2, 'Grace Hopper')",
        "CREATE VIEW order_names AS SELECT id, customer FROM orders",
        "CREATE VIEW order_customers AS SELECT o.id, c.name FROM orders o JOIN customers c ON c.id = o.id",
    ])
}

fn writable_relation(fixture: &SqliteFixture, name: &str) -> std::sync::Arc<bookie_gpui::live::LiveTable> {
    use bookie_gpui::live::{LiveSession, Origin};
    let mut session = LiveSession::open(&Origin::File(fixture.0.clone())).expect("open");
    std::sync::Arc::get_mut(&mut session).expect("not shared").writable = Some(uuid::Uuid::new_v4());
    let table = bookie_core::TableInfo {
        schema: None,
        name: name.into(),
    };
    std::sync::Arc::new(
        session
            .load_table_controlled(&table, 100, &tokio_util::sync::CancellationToken::new())
            .expect("load"),
    )
}

fn relation_view(
    cx: &mut TestAppContext,
    table: std::sync::Arc<bookie_gpui::live::LiveTable>,
) -> (gpui::Entity<GridView>, &mut gpui::VisualTestContext) {
    cx.update(crate::text_input::bind_keys);
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

#[test]
fn a_table_stays_editable_and_sqlite_views_are_read_only_on_a_writable_connection() {
    let fixture = orders_with_views();
    assert!(writable_relation(&fixture, "orders").editable());
    for name in ["order_names", "order_customers"] {
        let view = writable_relation(&fixture, name);
        assert!(!view.editable(), "{name} must not offer edits");
        assert!(view.origin().contains("read-only"), "{}", view.origin());
    }
}

#[gpui::test]
fn a_join_view_offers_no_save_and_says_why(cx: &mut TestAppContext) {
    let fixture = orders_with_views();
    let (view, cx) = relation_view(cx, writable_relation(&fixture, "order_customers"));
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cell_edit
                .edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("Changed".into()));
            view.edits_changed();
        })
    });
    cx.update(|window, cx| window.render_frame(cx));
    assert!(
        cx.debug_bounds("save-pending-edits").is_none(),
        "a read-only view offers no Save"
    );
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.can_save_edits());
        assert!(view.save.pending.is_none(), "no review opens");
        assert!(
            view.status.starts_with("This view cannot be edited: "),
            "{}",
            view.status
        );
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.request_delete(cx)));
    view.read_with(cx, |view, _| {
        assert!(!view.save.busy && view.save.pending.is_none());
        assert!(
            view.status.starts_with("This view cannot be edited: "),
            "{}",
            view.status
        );
    });
}

#[gpui::test]
fn a_single_table_sqlite_view_cannot_add_rows(cx: &mut TestAppContext) {
    let fixture = orders_with_views();
    let (view, cx) = relation_view(cx, writable_relation(&fixture, "order_names"));
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_insert_form(cx)));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.insert.open && !view.insert.loading);
        assert!(
            view.status.starts_with("This view cannot be edited: "),
            "{}",
            view.status
        );
    });
}
