use bookie_gpui::theme::Theme;
use gpui::{App, Global};

struct Active(Theme);

impl Global for Active {}

struct EditorSize(f32);

impl Global for EditorSize {}

pub fn apply_editor_size(size: f32, cx: &mut App) {
    gpui_kit::component::theme::Theme::global_mut(cx).mono_font_size = gpui::px(size);
    cx.set_global(EditorSize(size));
}

pub trait ThemeExt {
    fn current(cx: &App) -> Theme;
    fn install(self, cx: &mut App);
}

impl ThemeExt for Theme {
    fn current(cx: &App) -> Theme {
        cx.try_global::<Active>().map(|active| active.0).unwrap_or_default()
    }

    fn install(self, cx: &mut App) {
        let mode = if self.dark {
            gpui_kit::component::theme::ThemeMode::Dark
        } else {
            gpui_kit::component::theme::ThemeMode::Light
        };
        gpui_kit::component::theme::Theme::change(mode, None, cx);
        map_component_colors(&self, cx);
        if let Some(EditorSize(size)) = cx.try_global::<EditorSize>() {
            let size = *size;
            gpui_kit::component::theme::Theme::global_mut(cx).mono_font_size = gpui::px(size);
        }
        cx.set_global(Active(self));
    }
}

fn map_component_colors(theme: &Theme, cx: &mut App) {
    let color = |value: u32| gpui::Hsla::from(gpui::rgb(value));
    let colors = &mut gpui_kit::component::theme::Theme::global_mut(cx).colors;
    colors.table_active = color(theme.active_row);
    colors.table_active_border = color(theme.focus_ring);
    colors.table_hover = color(theme.accent);
    colors.list_active = color(theme.active_row);
    colors.list_active_border = color(theme.focus_ring);
    colors.list_hover = color(theme.accent);
    colors.selection = color(theme.active_row);
    colors.ring = color(theme.focus_ring);
    colors.primary = color(theme.foreground);
    colors.primary_foreground = color(theme.background);
}
