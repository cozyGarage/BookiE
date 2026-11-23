use bookie_gpui::messages::MessageKind;
use gpui::{Context, Div, Stateful, div, prelude::*, px, rgb};

use crate::assets::icon;
use crate::chrome::{STATUS_HEIGHT, ghost, ghost_icon, one_line};
use crate::{GridView, thousands};
use bookie_gpui::shell::table_label;
use bookie_gpui::theme::{Theme, size};

pub(crate) fn card(theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(size::SURFACE_RADIUS))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.background))
}

pub(crate) fn card_title(text: &str, theme: Theme) -> Div {
    div()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(theme.border))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(text.to_string())
}

pub(crate) fn home_stat_card(label: &str, icon_name: &'static str, value: &str, theme: Theme) -> Div {
    card(theme)
        .flex_1()
        .p_4()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_color(rgb(theme.muted_foreground))
                .child(icon(icon_name, theme.muted_foreground))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(size::STAT_TEXT))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(value.to_string()),
        )
}

pub(crate) fn home_shortcut(
    id: &'static str,
    icon_name: &'static str,
    label: &'static str,
    theme: Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .h(px(size::SMALL_CONTROL))
        .px_2()
        .rounded(px(theme.radius))
        .cursor_pointer()
        .text_color(rgb(theme.foreground))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(icon(icon_name, theme.muted_foreground))
        .child(label)
}

impl GridView {
    pub(crate) fn home_stats_row(&self, theme: Theme) -> Div {
        let unique_drivers = self
            .connections
            .iter()
            .map(|c| c.driver_id.as_str())
            .chain(self.live.as_ref().map(|l| l.driver_id.as_str()))
            .collect::<std::collections::HashSet<_>>()
            .len()
            .max(1);
        div()
            .flex()
            .gap_4()
            .child(home_stat_card(
                "Connections",
                "database",
                &self.connections.len().max(1).to_string(),
                theme,
            ))
            .child(home_stat_card(
                "Connected",
                "play",
                &usize::from(self.live.is_some() || self.connections.is_empty()).to_string(),
                theme,
            ))
            .child(home_stat_card(
                "Database Types",
                "package",
                &unique_drivers.to_string(),
                theme,
            ))
    }

    pub(crate) fn home_quick_start_card(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        card(theme).flex_1().child(card_title("Quick Start", theme)).child(
            div()
                .p_2()
                .flex()
                .flex_col()
                .children(self.connections.iter().enumerate().map(|(index, saved)| {
                    let (name, id) = (saved.name.clone(), saved.id);
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div().flex_1().child(
                                ghost(
                                    ("saved-connection", index),
                                    format!("{} · {}", saved.name, saved.driver_id),
                                    theme,
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.connect(bookie_gpui::live::Origin::Saved(name.clone()), cx);
                                    cx.notify();
                                })),
                            ),
                        )
                        .child(
                            ghost_icon(
                                ("edit-connection", index),
                                "sliders-horizontal",
                                "Edit connection",
                                theme,
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_connection(id, cx);
                            })),
                        )
                }))
                .when(!self.catalog.tables.is_empty(), |section| {
                    section.child(
                        ghost(
                            "quick-open",
                            format!(
                                "{} → {}",
                                self.catalog.connection,
                                table_label(&self.catalog.grid_table)
                            ),
                            theme,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            let name = this.catalog.grid_table.clone();
                            this.open_table_named(&name, window, cx);
                            cx.notify();
                        })),
                    )
                }),
        )
    }

    pub(crate) fn home_shortcuts_card(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        card(theme)
            .w(px(size::SIDE_PANEL_WIDTH))
            .child(card_title("Shortcuts", theme))
            .child(
                div()
                    .p_2()
                    .flex()
                    .flex_col()
                    .child(home_shortcut("s-new", "plus", "New Query", theme).on_click(cx.listener(
                        |this, _, window, cx| {
                            this.tabs.open_query();
                            this.sync_grid_owner();
                            if this.live.is_some() {
                                this.focus_editor = true;
                            } else {
                                window.focus(&this.focus, cx);
                            }
                            cx.notify();
                        },
                    )))
                    .child(
                        home_shortcut("s-status", "play", "Connection Status", theme).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.show_status();
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        home_shortcut("s-palette", "command", "Command Palette", theme).on_click(cx.listener(
                            |this, _, window, cx| {
                                this.open_palette(window, cx);
                            },
                        )),
                    ),
            )
    }

    pub(crate) fn home_query_history_card(&self, theme: Theme) -> Div {
        let entries = self.panel.messages.entries();
        let rows: Vec<(String, bool)> = if self.history.is_empty() {
            entries
                .iter()
                .rev()
                .take(5)
                .map(|entry| (entry.text.clone(), !matches!(entry.kind, MessageKind::Error)))
                .collect()
        } else {
            self.history
                .iter()
                .map(|row| {
                    (
                        format!("{} · {}", row.connection_name, one_line(&row.query)),
                        row.succeeded,
                    )
                })
                .collect()
        };
        let content = if rows.is_empty() {
            div()
                .p_4()
                .text_color(rgb(theme.muted_foreground))
                .child("No recent statements executed yet")
        } else {
            div()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .children(rows.into_iter().map(|(text, ok)| {
                    div()
                        .px_2()
                        .py_1()
                        .rounded(px(theme.radius))
                        .text_size(px(size::TEXT_SMALL))
                        .font_family("monospace")
                        .text_color(rgb(if ok { theme.foreground } else { theme.warning }))
                        .child(text)
                }))
        };
        card(theme).child(card_title("Query History", theme)).child(content)
    }

    pub fn placeholder_page(&self, name: &str, theme: Theme) -> Div {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(theme.muted_foreground))
            .child(format!(
                "No data loaded for {name}. Select an active table or run a query."
            ))
    }

    pub fn status_bar(&self, theme: Theme) -> Div {
        let right = if self.tabs.shows_grid_for(&self.catalog.grid_table) {
            format!(
                "{} rows · {}",
                thousands(self.dataset_count()),
                if theme.dark { "dark" } else { "light" }
            )
        } else {
            if theme.dark { "dark".into() } else { "light".into() }
        };
        let sql = self.tabs.shows_grid_for(&self.catalog.grid_table)
            .then(|| self.source.executed_sql()).flatten().map(str::to_owned);
        let elapsed = self.tabs.shows_grid_for(&self.catalog.grid_table)
            .then(|| self.source.load_elapsed()).flatten()
            .map(bookie_gpui::messages::format_elapsed);
        let details = sql.map(|sql| {
            use gpui_component::button::{Button, ButtonVariants};
            let full = sql.clone();
            div().flex().items_center().min_w_0().gap_1()
                .child(div().id("result-sql-preview").min_w_0().max_w(px(size::HOME_WIDTH / 2.0))
                    .overflow_hidden().whitespace_nowrap().text_ellipsis()
                    .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(full.clone()).build(window, cx))
                    .child(sql.clone()))
                .child(Button::new("copy-result-sql").ghost().compact().label("Copy SQL")
                    .on_click(move |_, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(sql.clone()))))
        });
        div()
            .flex()
            .items_center()
            .justify_between()
            .flex_none()
            .h(px(STATUS_HEIGHT))
            .px_3()
            .bg(rgb(theme.chrome))
            .border_t_1()
            .border_color(rgb(theme.border))
            .text_size(px(size::TEXT_SMALL))
            .text_color(rgb(theme.muted_foreground))
            .child(div().flex().min_w_0().overflow_hidden().items_center().gap_2().child(self.source.origin()).when(
                !self.status.is_empty(),
                |row| {
                    row.child(div().child("·"))
                        .child(div().text_color(rgb(theme.foreground)).child(self.status.clone()))
                },
            ))
            .children(details)
            .children(self.selection_stats_segment(theme))
            .children(elapsed.map(|time| div().child(time)))
            .when(self.tabs.shows_grid_for(&self.catalog.grid_table), |bar| bar.child(format!("Page: {} rows", self.pager.rows)))
            .child(right)
    }
}
