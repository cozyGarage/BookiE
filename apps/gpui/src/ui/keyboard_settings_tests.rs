use bookie_gpui::keymap::{Chord, Command, Keymap};
use gpui::TestAppContext;
use gpui_kit::test::TestWindowExt;

use crate::ui_tests::open;

fn record(view: &gpui::Entity<crate::GridView>, cx: &mut gpui::VisualTestContext, command: Command, keys: &str) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_recording_shortcut(command, window, cx)));
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

#[gpui::test]
fn a_recorded_shortcut_replaces_the_old_one_and_names_the_command_it_takes_from(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    record(&view, cx, Command::Filter, "ctrl-g");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.keymap.binding(Command::Filter),
            Some(&Chord::parse("ctrl-g").unwrap())
        );
        assert_eq!(view.recording_shortcut, None);
    });

    record(&view, cx, Command::Filter, "ctrl-b");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.keymap.binding(Command::Filter),
            Some(&Chord::parse("ctrl-b").unwrap())
        );
        assert_eq!(view.keymap.binding(Command::ToggleSidebar), None);
        assert!(view.status.contains(Command::ToggleSidebar.title()), "{}", view.status);
    });

    record(&view, cx, Command::Filter, "escape");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.keymap.binding(Command::Filter),
            Some(&Chord::parse("ctrl-b").unwrap())
        );
        assert_eq!(view.status, "Shortcut unchanged");
    });

    view.update(cx, |view, cx| {
        view.remove_shortcut(Command::Filter, cx);
        assert_eq!(view.keymap.binding(Command::Filter), None);
        view.reset_shortcut(Command::Filter, cx);
        view.reset_shortcut(Command::ToggleSidebar, cx);
        assert_eq!(view.keymap, Keymap::default());
    });
}

#[gpui::test]
fn the_palette_opens_settings_on_the_keyboard_page(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.0), gpui::px(900.0)));
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("keyboard-shortcuts", window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    assert!(cx.debug_bounds("settings-card").is_some(), "the settings page is shown");
    assert!(
        cx.debug_bounds("shortcut-filter").is_some(),
        "the keyboard page lists every shortcut"
    );
    assert!(cx.debug_bounds("shortcut-palette").is_some());
}

#[gpui::test]
fn the_appearance_page_lists_every_theme_and_applies_one_on_click(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.0), gpui::px(2400.0)));
    cx.update(|_, cx| crate::ui_theme::load_bundled_themes(cx));
    view.update(cx, |view, cx| {
        view.open_settings_page(crate::keyboard_settings_ui::SettingsPage::Appearance);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let names = cx.update(|_, cx| crate::GridView::theme_names(cx));
    assert_eq!(names.len(), 20, "two built-in themes and 18 bundled ones: {names:?}");
    assert!(
        cx.debug_bounds("theme-option-BookiE Light").is_some(),
        "the current theme is listed"
    );
    let nord = cx
        .debug_bounds("theme-option-Nord")
        .expect("Nord is listed with a Use button");
    cx.simulate_click(nord.center(), Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.fonts.settings.theme, "Nord");
        assert!(<bookie_gpui::theme::Theme as crate::ui_theme::ThemeExt>::current(cx).dark);
    });
}
