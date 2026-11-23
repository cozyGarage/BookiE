use bookie_app_services::selection_stats::{column_stats, column_stats_in, tooltip};

use super::*;

pub(crate) fn stats_line(text: &str) -> String {
    text.lines().collect::<Vec<_>>().join("  ·  ")
}

impl GridView {
    pub(crate) fn selection_stats_enabled(&self) -> bool {
        self.selection.is_some()
    }

    pub(crate) fn selection_stat_items(&self) -> Vec<(&'static str, String)> {
        let (Some(selection), Some(cursor)) = (self.selection, self.cell_cursor) else {
            return Vec::new();
        };
        if selection.count() < 2 || !selection.copyable() {
            return Vec::new();
        }
        let rows = self.selected_rows(selection);
        let columns = column_infos_for(self.source.columns());
        column_stats_in(&columns, &rows, &[cursor.column])
            .into_iter()
            .next()
            .map(|stats| {
                vec![
                    ("Count", stats.count.to_string()),
                    ("Sum", stats.sum),
                    ("Avg", stats.average),
                    ("Min", stats.minimum),
                    ("Max", stats.maximum),
                ]
            })
            .unwrap_or_default()
    }

    pub(crate) fn selection_stats_segment(&self, theme: Theme) -> Option<Div> {
        let items = self.selection_stat_items();
        (!items.is_empty()).then(|| {
            div().flex().items_center().gap_2().children(items.into_iter().enumerate().map(|(index, (label, value))| {
                let copied = value.clone();
                div()
                    .id(("selection-stat", index))
                    .debug_selector(move || format!("selection-stat-{label}"))
                    .cursor_pointer()
                    .hover(move |style| style.text_color(rgb(theme.foreground)))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(format!("Copy {label}")).build(window, cx)
                    })
                    .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copied.clone())))
                    .child(format!("{label} {value}"))
            }))
        })
    }

    pub(crate) fn show_selection_stats(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else { return };
        let rows = self.selected_rows(selection);
        let stats = column_stats(&column_infos_for(self.source.columns()), &rows);
        self.status = if stats.is_empty() {
            "The selected rows have no numeric columns to summarise".into()
        } else {
            stats_line(&tooltip(&stats))
        };
        cx.notify();
    }
}

#[cfg(test)]
#[path = "selection_stats_ui_tests.rs"]
mod tests;
