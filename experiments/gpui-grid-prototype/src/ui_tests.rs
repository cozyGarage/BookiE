use std::time::Instant;

use bookie_gpui_grid_prototype::model::Cell;
use gpui::{TestAppContext, VisualTestContext};

use crate::GridView;
use crate::text_input;

fn open(cx: &mut TestAppContext) -> (gpui::Entity<GridView>, &mut VisualTestContext) {
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::new(150_000, Instant::now(), window, cx));
    cx.run_until_parked();
    (view, cx)
}

#[gpui::test]
fn a_valid_edit_is_applied_to_the_typed_cell_and_marked(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(2, 3, window, cx)));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("2026-02-03");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.edits.get(&(2, 3)), Some(&Cell::Date("2026-02-03".into())));
        assert!(view.editing.is_none());
        assert!(view.status.contains("edited in this view only"));
    });
}

#[gpui::test]
fn an_invalid_edit_is_refused_and_keeps_the_editor_open(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(2, 3, window, cx)));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("2026-99-99");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.edits.is_empty());
        assert!(view.editing.is_some());
        assert_eq!(view.status, "Not applied: not a date");
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.editing.is_none() && view.edits.is_empty()));
}

#[gpui::test]
fn the_undecodable_cell_cannot_be_edited(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(10, 3, window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.editing.is_none());
        assert_eq!(view.status, "This cell cannot be edited");
    });
}

#[gpui::test]
fn the_find_field_takes_typed_text_and_enter_runs_the_search(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        let handle = view.read(cx).find.read(cx).focus_handle_for_tests();
        window.focus(&handle);
    });
    cx.simulate_input("sample 149999");
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.find.read(cx).content(), "sample 149999");
        assert_eq!(view.cursor.matches, vec![149_999]);
    });
}
