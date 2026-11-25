use gpui::{TestAppContext, VisualTestContext};

use crate::ui_tests::open;

fn assert_tooltips(cx: &mut VisualTestContext, controls: &[(&'static str, &'static str)]) {
    for (id, marker) in controls {
        let control = cx.debug_bounds(id).unwrap_or_else(|| panic!("{id} is not drawn"));
        let tooltip = cx
            .debug_bounds(marker)
            .unwrap_or_else(|| panic!("{id} has no tooltip ({marker})"));
        assert!(
            control.contains(&tooltip.center()),
            "the tooltip of {id} belongs to another control"
        );
    }
}

#[gpui::test]
fn every_icon_only_control_in_the_window_and_sidebar_header_has_a_tooltip(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(760.0), gpui::px(800.0)));
    view.update(cx, |view, cx| {
        view.tabs.open_query();
        view.sync_grid_owner();
        view.tabs.active = 0;
        cx.notify();
    });
    cx.run_until_parked();
    assert_tooltips(
        cx,
        &[
            ("new-connection", "new-connection tooltip: New connection"),
            ("new-query", "new-query tooltip: New query"),
            ("palette", "palette tooltip: Command palette (Ctrl+Shift+P)"),
            ("header-collapse-all", "header-collapse-all tooltip: Collapse all"),
            ("header-new-folder", "header-new-folder tooltip: New folder"),
            ("header-refresh", "header-refresh tooltip: Refresh"),
            ("collapse-sidebar", "collapse-sidebar tooltip: Collapse sidebar"),
            ("sidebar-locate", "sidebar-locate tooltip: Locate the open table"),
            ("sidebar-filter", "sidebar-filter tooltip: Regular expression"),
            (
                "sidebar-active-only",
                "sidebar-active-only tooltip: Show only the active table",
            ),
            ("close-1", "close-1 tooltip: Close tab"),
        ],
    );
    view.update(cx, |view, cx| {
        view.sidebar.open = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert_tooltips(
        cx,
        &[("sidebar-open-top-left", "sidebar-open-top-left tooltip: Show sidebar")],
    );
}

#[gpui::test]
fn every_icon_only_control_in_the_query_toolbar_and_results_has_a_tooltip(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.tabs.open_query();
        view.sync_grid_owner();
        cx.notify();
    });
    cx.run_until_parked();
    assert_tooltips(cx, &[("toggle-results", "toggle-results tooltip: Hide results")]);
    view.update(cx, |view, cx| {
        view.toggle_results();
        cx.notify();
    });
    cx.run_until_parked();
    assert_tooltips(cx, &[("toggle-results", "toggle-results tooltip: Show results")]);
}

#[gpui::test]
fn every_icon_only_control_in_the_grid_bar_has_a_tooltip(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.0), gpui::px(800.0)));
    cx.run_until_parked();
    assert_tooltips(
        cx,
        &[
            ("prev", "prev tooltip: Previous page"),
            ("next", "next tooltip: Next page"),
            (
                "load-more",
                "load-more tooltip: Append the next page instead of replacing the current one",
            ),
            ("find-prev", "find-prev tooltip: Previous match"),
            ("find-next", "find-next tooltip: Next match"),
            ("show", "show tooltip: Show matching rows only"),
        ],
    );
}
