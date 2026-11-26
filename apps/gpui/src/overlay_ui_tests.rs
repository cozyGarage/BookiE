use gpui::{TestAppContext, px, size};
use gpui_kit::test::{TestQueryExt, TestWindowExt};

#[gpui::test]
fn clicking_a_palette_action_executes_it_and_closes_the_palette(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    view.read_with(cx, |view, _| assert!(view.palette.open));
    cx.simulate_input("last page");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.palette.open));
    cx.update(|window, cx| window.render_frame(cx));
    view.read_with(cx, |view, cx| {
        let shown = view.palette_shown(cx);
        assert_eq!(shown.len(), 1);
        assert_eq!(view.palette_actions()[shown[0]].label, "Go to last page");
    });
    let action = cx
        .debug_bounds("palette-command-row-0")
        .expect("palette action is visible");
    cx.simulate_click(action.center(), Default::default());
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert!(!view.palette.open);
        assert!(view.pager.page > 0);
    });
}

#[gpui::test]
fn palette_outside_click_dismisses_without_opening_the_covered_dropdown(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.simulate_resize(size(px(900.0), px(760.0)));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.run_until_parked();

    let trigger = cx.update(|window, _| window.find_by_label("Import / Export").bounds());
    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();

    view.read_with(cx, |view, _| assert!(!view.palette.open));
    cx.update(|window, _| assert!(window.try_find_by_label("Import connections…").is_none()));

    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();
    let item = cx.update(|window, _| window.find_by_label("Import connections…").bounds());
    assert!(item.origin.x >= px(0.0));
    assert!(item.origin.x + item.size.width <= px(900.0));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, _| assert!(window.try_find_by_label("Import connections…").is_none()));

    cx.simulate_resize(size(px(800.0), px(760.0)));
    cx.run_until_parked();
    let trigger = cx.update(|window, _| window.find_by_label("Import / Export").bounds());
    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();
    let item = cx.update(|window, _| window.find_by_label("Import connections…").bounds());
    assert!(item.origin.x >= px(0.0));
    assert!(item.origin.x + item.size.width <= px(800.0));
    assert!(item.origin.y >= px(0.0));
    assert!(item.origin.y + item.size.height <= px(760.0));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, _| assert!(window.try_find_by_label("Import connections…").is_none()));
}

#[gpui::test]
fn the_palette_stops_above_the_status_bar_in_a_short_window(cx: &mut TestAppContext) {
    let (_view, cx) = crate::ui_tests::open(cx);
    cx.simulate_resize(size(px(900.0), px(420.0)));
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let surface = cx.debug_bounds("palette-surface").expect("the palette is drawn");
    let limit = px(420.0 - crate::chrome::STATUS_HEIGHT - bookie_gpui::theme::size::PALETTE_TOP / 2.0);
    assert!(
        surface.origin.y + surface.size.height <= limit,
        "palette bottom {:?} passes {limit:?}",
        surface.origin.y + surface.size.height
    );
}

#[gpui::test]
fn window_shortcuts_do_not_run_behind_an_open_dialog(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();
    let tabs = view.read_with(cx, |view, _| view.tabs.items.clone());
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_connection_dialog(cx)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.simulate_keystrokes("ctrl-w ctrl-t ctrl-shift-p");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.connection_form.open);
        assert_eq!(view.tabs.items, tabs, "no tab was closed or opened behind the dialog");
        assert!(!view.palette.open);
    });
}
