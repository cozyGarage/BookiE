use std::time::Instant;

use gpui::{TestAppContext, VisualTestContext};

use crate::GridView;
use crate::text_input;
use bookie_core::TableInfo;
use bookie_gpui::live::{LiveTable, Origin};
use bookie_gpui::model::Cell;

fn orders_fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/orders.sqlite")
}

fn open_first_two_orders(cx: &mut TestAppContext) -> (gpui::Entity<GridView>, &mut VisualTestContext) {
    let table = std::sync::Arc::new(
        LiveTable::load(
            &Origin::File(orders_fixture()),
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            2,
        )
        .expect("load"),
    );
    cx.update(text_input::bind_keys);
    cx.add_window_view(|window, cx| {
        let mut view = GridView::with_source(table.clone(), Instant::now(), window, cx);
        view.attach_live(table.clone());
        view
    })
}

fn submit_where(view: &gpui::Entity<GridView>, cx: &mut VisualTestContext, text: &str) {
    cx.run_until_parked();
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("where", window, cx)));
    cx.simulate_keystrokes("ctrl-a backspace");
    if !text.is_empty() {
        cx.simulate_input(text);
    }
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
}

#[gpui::test]
fn a_where_filter_runs_on_the_server_not_on_the_loaded_rows(cx: &mut TestAppContext) {
    let (view, cx) = open_first_two_orders(cx);
    view.read_with(cx, |view, _| assert_eq!(view.source.total(), 2));

    submit_where(&view, cx, "id = 6");
    view.read_with(cx, |view, _| {
        assert_eq!(view.source.total(), 1, "{}", view.status);
        assert_eq!(view.source.row(0)[0], Cell::Int(6));
        assert!(matches!(view.tabs.current(), bookie_gpui::shell::Tab::Table(_)));
    });

    submit_where(&view, cx, "id = = 1");
    view.read_with(cx, |view, _| {
        assert_eq!(view.source.total(), 1, "a failed WHERE keeps the previous grid");
        assert_eq!(view.source.row(0)[0], Cell::Int(6));
        assert!(!view.status.is_empty(), "the database error is reported");
    });

    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("clear-where", window, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.source.total(), 6, "{}", view.status);
        assert_eq!(view.source.row(0)[0], Cell::Int(1));
    });
}

#[gpui::test]
fn the_row_count_respects_the_where_filter(cx: &mut TestAppContext) {
    let (view, cx) = open_first_two_orders(cx);
    submit_where(&view, cx, "customer = 'Ada'");
    view.read_with(cx, |view, _| assert_eq!(view.source.total(), 2, "{}", view.status));
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("count-table-rows", window, cx)));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.status.contains(" 2 rows"), "{}", view.status);
    });
}
