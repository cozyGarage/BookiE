use std::rc::Rc;

use bookie_gpui::theme::Theme;
use gpui::{App, Global, Hsla, SharedString};
use gpui_kit::component::theme::{Theme as ComponentTheme, ThemeConfig, ThemeMode, ThemeRegistry};

const THEME_FILES: &[&str] = &[
    include_str!("../themes/catppuccin.json"),
    include_str!("../themes/dracula.json"),
    include_str!("../themes/gruvbox.json"),
    include_str!("../themes/nord.json"),
    include_str!("../themes/one.json"),
    include_str!("../themes/solarized.json"),
    include_str!("../themes/tokyo-night.json"),
    include_str!("../themes/vscode.json"),
];

struct Active(Theme);

impl Global for Active {}

struct EditorSize(f32);

impl Global for EditorSize {}

pub fn apply_editor_size(size: f32, cx: &mut App) {
    ComponentTheme::global_mut(cx).mono_font_size = gpui::px(size);
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
        let mode = if self.dark { ThemeMode::Dark } else { ThemeMode::Light };
        if cx.has_global::<ComponentTheme>() {
            let registry = ThemeRegistry::global(cx);
            let (light, dark) = (
                registry.default_light_theme().clone(),
                registry.default_dark_theme().clone(),
            );
            let theme = ComponentTheme::global_mut(cx);
            theme.light_theme = light;
            theme.dark_theme = dark;
        }
        ComponentTheme::change(mode, None, cx);
        map_component_colors(&self, cx);
        if let Some(EditorSize(size)) = cx.try_global::<EditorSize>() {
            let size = *size;
            ComponentTheme::global_mut(cx).mono_font_size = gpui::px(size);
        }
        cx.set_global(Active(self));
    }
}

fn map_component_colors(theme: &Theme, cx: &mut App) {
    let color = |value: u32| gpui::Hsla::from(gpui::rgb(value));
    let colors = &mut ComponentTheme::global_mut(cx).colors;
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

pub fn load_bundled_themes(cx: &mut App) {
    let registry = ThemeRegistry::global_mut(cx);
    for file in THEME_FILES {
        if let Err(error) = registry.load_themes_from_str(file) {
            tracing::warn!(%error, "a bundled theme did not load");
        }
    }
}

pub fn bundled_theme_names(cx: &App) -> Vec<SharedString> {
    let defaults: Vec<SharedString> = ThemeRegistry::global(cx)
        .default_themes()
        .values()
        .map(|config| config.name.clone())
        .collect();
    ThemeRegistry::global(cx)
        .sorted_themes()
        .into_iter()
        .map(|config| config.name.clone())
        .filter(|name| !defaults.contains(name))
        .collect()
}

pub fn install_named(name: &str, cx: &mut App) -> bool {
    let Some(config) = ThemeRegistry::global(cx).themes().get(name).cloned() else {
        return false;
    };
    let dark = config.mode.is_dark();
    ComponentTheme::change(config.mode, None, cx);
    apply_config(&config, cx);
    let theme = from_component(dark, cx);
    if let Some(EditorSize(size)) = cx.try_global::<EditorSize>() {
        let size = *size;
        ComponentTheme::global_mut(cx).mono_font_size = gpui::px(size);
    }
    cx.set_global(Active(theme));
    true
}

fn apply_config(config: &Rc<ThemeConfig>, cx: &mut App) {
    ComponentTheme::global_mut(cx).apply_config(config);
}

fn from_component(dark: bool, cx: &App) -> Theme {
    let base = if dark { Theme::dark() } else { Theme::light() };
    let colors = &ComponentTheme::global(cx).colors;
    Theme {
        background: hex(colors.background),
        foreground: hex(colors.foreground),
        muted: hex(colors.muted),
        muted_foreground: hex(colors.muted_foreground),
        chrome: hex(colors.title_bar),
        sidebar: hex(colors.sidebar),
        popover: hex(colors.popover),
        border: hex(colors.border),
        focus_ring: hex(colors.ring),
        accent: hex(colors.accent),
        stripe: hex(colors.table_even),
        active_row: hex(colors.table_active),
        info: hex(colors.info),
        success: hex(colors.success),
        warning: hex(colors.warning),
        destructive: hex(colors.danger),
        ..base
    }
}

fn hex(color: Hsla) -> u32 {
    let rgb = color.to_rgb();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(rgb.r) << 16) | (channel(rgb.g) << 8) | channel(rgb.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn every_bundled_theme_loads_and_one_can_be_installed_and_left_again(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            load_bundled_themes(cx);
            let names = bundled_theme_names(cx);
            assert_eq!(names.len(), 18, "{names:?}");
            assert!(install_named("Nord", cx));
            let nord = Theme::current(cx);
            assert!(nord.dark);
            assert_eq!(nord.background, 0x2e3440);
            assert!(!install_named("No such theme", cx));
            Theme::light().install(cx);
            assert_eq!(Theme::current(cx), Theme::light());
            assert!(!ComponentTheme::global(cx).mode.is_dark());
        });
    }

    #[test]
    fn hex_round_trips_an_rgb_colour() {
        assert_eq!(hex(Hsla::from(gpui::rgb(0x2e3440))), 0x2e3440);
    }
}
