use bookie_gpui::messages::MessageKind;
use gpui::{Context, Div, Stateful, div, prelude::*, px, rgb};

use crate::assets::icon;
use crate::chrome::{STATUS_HEIGHT, one_line, tip};
use crate::surfaces::{ButtonKind, button, compact_button, icon_button, section_header};
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
    section_header(text.to_string(), theme)
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(theme.border))
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
    pub(crate) fn home_counts(&self) -> (usize, usize, usize) {
        let unique_drivers = self
            .connections
            .iter()
            .map(|c| c.driver_id.as_str())
            .chain(self.live.as_ref().map(|l| l.driver_id.as_str()))
            .collect::<std::collections::HashSet<_>>()
            .len();
        (self.connections.len(), usize::from(self.live.is_some()), unique_drivers)
    }

    pub(crate) fn home_stats_row(&self, theme: Theme) -> Div {
        let (connections, connected, unique_drivers) = self.home_counts();
        div()
            .flex()
            .gap_4()
            .child(home_stat_card(
                "Connections",
                "database",
                &connections.to_string(),
                theme,
            ))
            .child(home_stat_card("Connected", "play", &connected.to_string(), theme))
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
                                button(
                                    ButtonKind::Ghost,
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
                            icon_button(
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
                        button(
                            ButtonKind::Ghost,
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

    pub(crate) fn status_shows_grid(&self) -> bool {
        self.on_table_tab()
            || (self.tabs.current() == &bookie_gpui::shell::Tab::Query && !self.run_tabs.slots.is_empty())
    }

    pub(crate) fn status_count_text(&self, dark: bool, compact: bool) -> String {
        match (self.status_shows_grid(), compact) {
            (true, false) => footer_counts(self.dataset_count(), self.source.columns().len(), dark),
            (true, true) => format!("{} rows", thousands(self.dataset_count())),
            (false, _) => String::new(),
        }
    }

    pub fn status_bar(&self, theme: Theme, window_width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let compact = window_width < size::STATUS_COMPACT_WIDTH;
        let table_tab = self.on_table_tab();
        let grid = self.status_shows_grid();
        let rows = self.status_count_text(theme.dark, compact);
        let sql = (grid && !compact)
            .then(|| self.source.executed_sql())
            .flatten()
            .map(str::to_owned);
        let elapsed = grid
            .then(|| self.source.load_elapsed())
            .flatten()
            .map(bookie_gpui::messages::format_elapsed);
        let count_rows = (grid && !self.grid_is_query && self.source.is_partial()).then(|| {
            compact_button(ButtonKind::Ghost, "count-rows", "Count rows", theme)
                .tooltip(tip("Count the rows of this table in the database"))
                .on_click(cx.listener(|this, _, _, cx| this.count_table_rows(cx)))
        });
        let details = sql.map(|sql| {
            let full = sql.clone();
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .id("result-sql-preview")
                        .max_w(px(size::STATUS_SQL_PREVIEW_WIDTH))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(full.clone()).build(window, cx)
                        })
                        .child(sql.clone()),
                )
                .child(
                    compact_button(ButtonKind::Ghost, "copy-result-sql", "Copy SQL", theme)
                        .on_click(move |_, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(sql.clone()))),
                )
        });
        let connection_tooltip = self.connection_tooltip();
        let connection = div()
            .id("status-connection")
            .debug_selector(|| "status-connection".into())
            .min_w_0()
            .max_w(px(size::STATUS_CONNECTION_WIDTH))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(connection_tooltip.clone()).build(window, cx)
            })
            .child(self.connection_chip_text());
        let full_status = self.status.clone();
        let status = div()
            .id("status-text")
            .debug_selector(|| "status-text".into())
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_color(rgb(theme.foreground))
            .when(!self.status.is_empty(), |status| {
                status.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(full_status.clone()).build(window, cx)
                })
            })
            .child(self.status.clone());
        div()
            .id("status-bar")
            .debug_selector(|| "status-bar".into())
            .flex()
            .items_center()
            .gap_2()
            .flex_none()
            .h(px(STATUS_HEIGHT))
            .px_3()
            .overflow_hidden()
            .bg(rgb(theme.chrome))
            .border_t_1()
            .border_color(rgb(theme.border))
            .text_size(px(size::TEXT_SMALL))
            .text_color(rgb(theme.muted_foreground))
            .child(connection)
            .child(status)
            .children(details)
            .children(count_rows)
            .when(!compact, |bar| bar.children(self.selection_stats_segment(theme)))
            .children(elapsed.map(|time| div().flex_none().child(time)))
            .when(table_tab, |bar| {
                bar.child(
                    div()
                        .debug_selector(|| "status-page".into())
                        .flex_none()
                        .child(format!("Page: {} rows", self.pager.rows)),
                )
            })
            .child(
                div()
                    .id("status-count")
                    .debug_selector(|| "status-count".into())
                    .flex_none()
                    .child(rows),
            )
    }
}

fn footer_counts(rows: u64, columns: usize, dark: bool) -> String {
    format!(
        "{} rows · {} {} · {}",
        thousands(rows),
        columns,
        if columns == 1 { "column" } else { "columns" },
        if dark { "dark" } else { "light" }
    )
}

#[cfg(test)]
mod footer_tests {
    use super::footer_counts;

    #[test]
    fn the_footer_counts_rows_and_columns_and_names_the_theme() {
        assert_eq!(footer_counts(1234, 8, false), "1,234 rows · 8 columns · light");
        assert_eq!(footer_counts(0, 1, true), "0 rows · 1 column · dark");
    }
}
