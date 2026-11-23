use gpui::TestAppContext;

use crate::ui_theme::ThemeExt;

use super::{open, PaletteMode};

#[gpui::test]
fn the_theme_picker_lists_bundled_themes_and_installs_the_chosen_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        crate::ui_theme::load_bundled_themes(cx);
        view.update(cx, |view, cx| view.open_picker(PaletteMode::Themes, window, cx));
    });
    view.read_with(cx, |view, cx| {
        let titles: Vec<_> = view.picker_items(cx).into_iter().map(|item| item.title).collect();
        assert_eq!(titles[..2], ["BookiE Light", "BookiE Dark"]);
        assert!(titles.iter().any(|title| title == "Nord"), "{titles:?}");
    });
    cx.update(|_, cx| assert!(crate::picker::install_theme("Tokyo Night", cx)));
    view.read_with(cx, |_, cx| assert!(crate::Theme::current(cx).dark));
    cx.update(|_, cx| assert!(crate::picker::install_theme("BookiE Light", cx)));
    view.read_with(cx, |_, cx| assert_eq!(crate::Theme::current(cx), crate::Theme::light()));
}
