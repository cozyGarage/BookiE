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

    let trigger = cx.update(|window, _| window.find_by_label("Import / Export connections").bounds());
    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();

    view.read_with(cx, |view, _| assert!(!view.palette.open));
    cx.update(|window, _| assert!(window.try_find_by_label("Import Connections…").is_none()));

    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();
    let item = cx.update(|window, _| window.find_by_label("Import Connections…").bounds());
    assert!(item.origin.x >= px(0.0));
    assert!(item.origin.x + item.size.width <= px(900.0));
    cx.simulate_click(item.center(), Default::default());
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert!(!view.palette.open);
    });

    cx.simulate_resize(size(px(800.0), px(760.0)));
    cx.run_until_parked();
    let trigger = cx.update(|window, _| window.find_by_label("Import / Export connections").bounds());
    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();
    let item = cx.update(|window, _| window.find_by_label("Import Connections…").bounds());
    assert!(item.origin.x >= px(0.0));
    assert!(item.origin.x + item.size.width <= px(800.0));
    assert!(item.origin.y >= px(0.0));
    assert!(item.origin.y + item.size.height <= px(760.0));
}
