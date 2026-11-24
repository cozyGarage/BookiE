use gpui::TestAppContext;
use gpui_kit::test::TestWindowExt;

#[gpui::test]
fn token_buttons_fit_inside_settings_at_narrow_width(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.simulate_resize(gpui::size(gpui::px(800.0), gpui::px(900.0)));
    view.update(cx, |view, cx| {
        view.tabs.open_settings();
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let card = cx.debug_bounds("settings-card").expect("settings card is visible");
    for id in ["mcp-issue-read", "mcp-issue-write"] {
        let button = cx.debug_bounds(id).expect("token action is visible");
        assert!(button.origin.x + button.size.width <= card.origin.x + card.size.width);
    }
}

#[gpui::test]
fn settings_show_mcp_controls_and_refuse_start_without_a_saved_connection(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_settings();
            view.sync_grid_owner();
            view.set_mcp_enabled(true, cx);
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("mcp-issue-read").is_some(),
        "read-only token action is visible"
    );
    assert!(
        cx.debug_bounds("mcp-issue-write").is_some(),
        "read-write token action is visible"
    );
    view.read_with(cx, |view, _| {
        assert!(!view.mcp.enabled());
        assert_eq!(
            view.mcp.status.as_deref(),
            Some("Connect to a saved database before enabling MCP.")
        );
    });
}

#[gpui::test]
fn token_issue_requires_an_enabled_server_and_does_not_retain_plaintext(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.issue_mcp_token(bookie_app_services::mcp_service::TokenPermissions::ReadOnly, cx);
            cx.notify();
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.mcp.status.as_deref(),
            Some("Enable the MCP server before issuing a token.")
        );
        assert!(view.mcp.plaintext.is_none());
    });
}
