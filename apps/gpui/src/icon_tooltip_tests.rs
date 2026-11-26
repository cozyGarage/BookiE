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
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        view.sidebar.open = true;
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
            ("palette", "palette tooltip: Command palette · Ctrl+Shift+P"),
            ("header-collapse-all", "header-collapse-all tooltip: Collapse all"),
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
            ("pager-prev", "pager-prev tooltip: Previous page"),
            ("next", "next tooltip: Next page"),
            ("find-prev", "find-prev tooltip: Previous match"),
            ("find-next", "find-next tooltip: Next match"),
            ("show", "show tooltip: Show matching rows only"),
        ],
    );
}

const KEYBOARD_PATHS: &[(&str, &str)] = &[
    ("new-connection", "new-connection"),
    ("new-query", "query"),
    ("header-collapse-all", "collapse-sidebar-sections"),
    ("collapse-sidebar", "sidebar"),
    ("sidebar-locate", "reveal-sidebar"),
    ("sidebar-filter", "sidebar-regex"),
    ("sidebar-active-only", "sidebar-active-only"),
    ("pager-prev", "previous"),
    ("next", "next"),
    ("find-prev", "find"),
    ("find-next", "find"),
    ("show", "filter"),
    ("toggle-results", "results"),
];

#[gpui::test]
fn every_toolbar_and_sidebar_icon_action_can_be_reached_from_the_palette(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(1500.0), gpui::px(800.0)));
    view.update(cx, |view, cx| {
        view.sidebar.open = true;
        cx.notify();
    });
    cx.run_until_parked();
    let palette: Vec<&str> = view.read_with(cx, |view, _| {
        view.palette_actions().iter().map(|action| action.id).collect()
    });
    let mut drawn = 0;
    for (control, action) in KEYBOARD_PATHS {
        assert!(palette.contains(action), "{control} has no palette action {action}");
        if cx.debug_bounds(control).is_some() {
            drawn += 1;
        }
    }
    assert!(
        drawn >= KEYBOARD_PATHS.len() / 2,
        "only {drawn} of the listed controls are drawn"
    );
    view.update(cx, |view, cx| {
        view.tabs.open_query();
        view.sync_grid_owner();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("toggle-results").is_some());
}

#[gpui::test]
fn the_palette_toggles_the_sidebar_and_paging_options_like_their_icons(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let state = |view: &crate::GridView| {
        (
            view.sidebar.active_only,
            view.pager.append,
            view.sidebar.connection_open || view.sidebar.database_open,
        )
    };
    let before = view.read_with(cx, |view, _| state(view));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            for id in ["sidebar-active-only", "append-pages", "collapse-sidebar-sections"] {
                view.run_palette_id(id, window, cx);
            }
        })
    });
    let after = view.read_with(cx, |view, _| state(view));
    assert_eq!(after, (!before.0, !before.1, !before.2));
}
