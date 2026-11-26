use gpui::TestAppContext;

#[gpui::test]
fn hiding_results_cancels_explain_and_drops_its_late_result(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.begin_background(
                crate::Work::Query,
                cx,
                |_| (),
                |view, ()| view.status = "stale plan".into(),
            );
            view.panel.explaining = true;
            let cancel = view.queries.cancel.clone();
            view.toggle_results();
            assert!(cancel.is_cancelled());
            assert!(!view.panel.explaining);
            assert_eq!(view.status, "Canceled the explain");
            assert!(!view.panel.open);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.status, "Canceled the explain"));
}

#[gpui::test]
fn format_rewrites_the_editor_with_the_shared_formatter_and_leaves_empty_text_alone(cx: &mut TestAppContext) {
    let (view, cx) = crate::live_ui_tests::open_live(cx);
    let original = "select id, customer from orders where total > 1 order by id";
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.sql_input
                .update(cx, |input, cx| input.set_value(original, window, cx));
            view.format_query(window, cx);
        })
    });
    let grammar = bookie_app_services::result_editability::grammar_for_driver("sqlite").expect("sqlite grammar");
    let expected = bookie_core::sql_format::format_script(
        original,
        grammar,
        bookie_core::sql_syntax::script::LexicalSettings::default_for(grammar),
    );
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sql_input.read(cx).value().to_string(), expected);
        assert_ne!(expected, original, "the formatter changes this statement");
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input.update(cx, |input, cx| input.set_value("  ", window, cx));
            view.status.clear();
            view.format_query(window, cx);
            assert!(view.status.is_empty(), "blank text is not formatted");
        })
    });
}

#[gpui::test]
fn a_blocked_explain_cannot_cancel_the_query_already_running(cx: &mut TestAppContext) {
    let (view, cx) = crate::live_ui_tests::open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input
                .update(cx, |input, cx| input.set_value("SELECT id FROM orders", window, cx));
            view.run_query(cx);
            let query = view.queries.cancel.clone();
            view.save.busy = true;
            view.explain_query(cx);
            assert!(view.status.contains("save is running"));
            view.toggle_results();
            assert!(!query.is_cancelled());
            assert!(!view.panel.explaining);
        });
    });
}

#[gpui::test]
fn replacement_load_does_not_inherit_explain_cancellation(cx: &mut TestAppContext) {
    let (view, cx) = crate::live_ui_tests::open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input
                .update(cx, |input, cx| input.set_value("SELECT id FROM orders", window, cx));
            view.explain_query(cx);
            assert!(view.panel.explaining);
            let explain = view.queries.cancel.clone();
            view.run_query(cx);
            let query = view.queries.cancel.clone();
            view.toggle_results();
            assert!(explain.is_cancelled());
            assert!(!query.is_cancelled());
            assert!(!view.panel.explaining);
        });
    });
}
