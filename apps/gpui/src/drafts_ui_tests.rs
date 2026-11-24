use gpui::TestAppContext;
use gpui_kit::component::input::Position;

use crate::live_ui_tests::open_live;

#[gpui::test]
fn a_closed_query_comes_back_from_the_draft_store_when_no_closed_tab_is_left(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.sql_input.update(cx, |input, cx| input.set_value("SELECT 'kept' AS draft", window, cx));
            let index = view.tabs.active;
            view.close_tab(index, cx);
            assert_eq!(view.closed_drafts.as_ref().map(|drafts| drafts.len()), Some(1));
            view.search.closed = Default::default();
            view.sql_input.update(cx, |input, cx| input.set_value("", window, cx));
            view.reopen_closed_tab(window, cx);
            assert_eq!(view.tabs.current(), &bookie_gpui::shell::Tab::Query);
            assert_eq!(view.sql_input.read(cx).value().to_string(), "SELECT 'kept' AS draft");
            assert_eq!(view.closed_drafts.as_ref().map(|drafts| drafts.len()), Some(0));
        })
    });
}

#[gpui::test]
fn go_to_last_edit_returns_the_caret_to_where_the_query_was_last_changed(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.sql_input.update(cx, |input, cx| {
                input.set_value("SELECT 1;\nSELECT 2;\nSELECT 3;", window, cx);
                input.set_cursor_position(Position::new(1, 6), window, cx);
            });
        })
    });
    cx.simulate_input("x");
    cx.run_until_parked();
    let edited = view.read_with(cx, |view, cx| view.sql_input.read(cx).cursor_position());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input.update(cx, |input, cx| input.set_cursor_position(Position::new(0, 0), window, cx));
            view.run_palette_id("go-to-last-edit", window, cx);
        })
    });
    cx.run_until_parked();
    let now = view.read_with(cx, |view, cx| view.sql_input.read(cx).cursor_position());
    assert_eq!(now, edited);
    assert_eq!(edited, Position::new(1, 7), "the caret sits after the typed character");
}
