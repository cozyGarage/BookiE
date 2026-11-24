use bookie_gpui::theme::Theme;
use gpui::{App, Entity, div, prelude::*, px, rgb};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem};

use crate::GridView;
use crate::picker::{BUILT_IN_DARK, BUILT_IN_LIGHT};
use crate::ui_theme::ThemeExt;

impl GridView {
    pub(crate) fn current_theme_name(&self, cx: &App) -> String {
        if !self.fonts.settings.theme.is_empty() {
            return self.fonts.settings.theme.clone();
        }
        if Theme::current(cx).dark {
            BUILT_IN_DARK
        } else {
            BUILT_IN_LIGHT
        }
        .to_string()
    }

    pub(crate) fn theme_names(cx: &App) -> Vec<String> {
        [BUILT_IN_LIGHT.to_string(), BUILT_IN_DARK.to_string()]
            .into_iter()
            .chain(
                crate::ui_theme::bundled_theme_names(cx)
                    .into_iter()
                    .map(|name| name.to_string()),
            )
            .collect()
    }

    pub(crate) fn theme_group(&self, view: Entity<Self>, theme: Theme, cx: &App) -> SettingGroup {
        let current = self.current_theme_name(cx);
        Self::theme_names(cx).into_iter().enumerate().fold(
            SettingGroup::new().title("Color themes"),
            |group, (index, name)| {
                let in_use = name == current;
                let view = view.clone();
                let option = name.clone();
                group.item(SettingItem::new(
                    name,
                    SettingField::render(move |_, _, _| {
                        let selector = format!("theme-option-{option}");
                        if in_use {
                            return div()
                                .debug_selector(move || selector.clone())
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(px(bookie_gpui::theme::size::TEXT_SMALL))
                                .text_color(rgb(theme.muted_foreground))
                                .child(crate::assets::icon("check", theme.foreground))
                                .child("In use")
                                .into_any_element();
                        }
                        let view = view.clone();
                        let option = option.clone();
                        crate::chrome::ghost(("use-theme", index), "Use", theme)
                            .debug_selector(move || selector.clone())
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.choose_theme(&option, window, cx);
                                    cx.notify();
                                })
                            })
                            .into_any_element()
                    }),
                ))
            },
        )
    }
}
