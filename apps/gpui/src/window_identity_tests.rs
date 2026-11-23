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

#[gpui::test]
fn editing_connection_tab_uses_saved_name(cx: &mut TestAppContext) {
    let (view, cx) = ui_tests::open(cx);
    view.update(cx, |view, cx| {
        view.connection_form.editing = Some(uuid::Uuid::new_v4());
        view.connection_form
            .name
            .update(cx, |input, cx| input.set_content("minipc Redis", cx));
        view.tabs.open_new_connection();
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.tab_title(view.tabs.current(), cx), "minipc Redis");
    });
}

#[gpui::test]
fn unnamed_connection_edit_tab_identifies_edit_mode(cx: &mut TestAppContext) {
    let (view, cx) = ui_tests::open(cx);
    view.update(cx, |view, _| {
        view.connection_form.editing = Some(uuid::Uuid::new_v4());
        view.tabs.open_new_connection();
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.tab_title(view.tabs.current(), cx), "Edit connection");
    });
}
