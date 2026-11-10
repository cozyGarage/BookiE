use bookie_gpui::theme::Theme;
use gpui::{App, Global};

struct Active(Theme);

impl Global for Active {}

pub trait ThemeExt {
    fn current(cx: &App) -> Theme;
    fn install(self, cx: &mut App);
}

impl ThemeExt for Theme {
    fn current(cx: &App) -> Theme {
        cx.try_global::<Active>().map(|active| active.0).unwrap_or_default()
    }

    fn install(self, cx: &mut App) {
        cx.set_global(Active(self));
    }
}
