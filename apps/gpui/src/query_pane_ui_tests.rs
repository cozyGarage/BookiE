use gpui::TestAppContext;

#[gpui::test]
fn hiding_results_cancels_explain_and_drops_its_late_result(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.panel.explaining = true;
            view.begin_background(crate::Work::Load, cx, |_| (), |view, ()| view.status = "stale plan".into());
            let cancel = view.loads.cancel.clone();
            view.toggle_results();
            assert!(cancel.is_cancelled());
            assert!(!view.panel.explaining);
            assert_eq!(view.status, "Explain cancelled");
            assert!(!view.panel.open);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.status, "Explain cancelled"));
}
