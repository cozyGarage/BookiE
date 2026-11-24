use gpui::TestAppContext;

#[gpui::test]
fn settings_show_mcp_controls_and_refuse_start_without_a_saved_connection(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.open_settings_page(crate::keyboard_settings_ui::SettingsPage::Mcp);
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
