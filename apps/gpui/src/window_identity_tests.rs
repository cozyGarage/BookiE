use gpui::TestAppContext;

use crate::{Tab, ui_tests};

#[gpui::test]
fn title_tracks_the_active_tab(cx: &mut TestAppContext) {
    let (view, cx) = ui_tests::open(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.window_title_tab.as_ref(), Some(view.tabs.current()));
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            cx.notify();
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.window_title_tab, Some(Tab::Query));
        assert_eq!(view.tabs.current().title(), "Query 1");
    });
}
