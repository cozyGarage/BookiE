use gpui::TestAppContext;
use gpui_kit::test::TestWindowExt;

use crate::live_ui_tests::open_live;

fn bounds(cx: &mut gpui::VisualTestContext, selector: &'static str) -> gpui::Bounds<gpui::Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is rendered"))
}

#[gpui::test]
fn a_long_status_message_never_overlaps_the_status_chips(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    for width in [1500.0, 760.0] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(800.0)));
        view.update(cx, |view, cx| {
            view.status = "The statement failed: ".to_string() + &"a very long database message ".repeat(40);
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
        let bar = bounds(cx, "status-bar");
        let connection = bounds(cx, "status-connection");
        let text = bounds(cx, "status-text");
        let rows = bounds(cx, "status-count");
        assert!(
            connection.origin.x + connection.size.width <= text.origin.x,
            "{width}: {connection:?} {text:?}"
        );
        assert!(
            text.origin.x + text.size.width <= rows.origin.x,
            "{width}: {text:?} {rows:?}"
        );
        assert!(
            rows.origin.x + rows.size.width <= bar.origin.x + bar.size.width,
            "{width}: {rows:?} {bar:?}"
        );
    }
}

#[gpui::test]
fn the_connection_chip_shows_the_server_version_after_it_loads(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    view.update(cx, |view, cx| view.request_connection_info(cx));
    cx.run_until_parked();
    let chip = view.read_with(cx, |view, _| view.connection_chip_text());
    assert!(
        chip.starts_with("SQLite 3."),
        "the version leads so truncation cannot hide it: {chip}"
    );
    assert!(!chip.contains("sqlite ·"), "{chip}");
    let tooltip = view.read_with(cx, |view, _| view.connection_tooltip());
    assert!(tooltip.contains("Round trip:"), "{tooltip}");
}

#[gpui::test]
fn sidebar_tables_show_their_size_when_the_database_reports_one(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.0), gpui::px(900.0)));
    view.update(cx, |view, cx| {
        view.table_sizes = bookie_app_services::table_sizes::SizeIndex::new(vec![bookie_core::TableSize {
            schema: None,
            name: "orders".into(),
            bytes: 5 * 1024 * 1024,
        }]);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    assert!(
        cx.debug_bounds("sidebar-size-orders").is_some(),
        "orders shows its size"
    );
}
