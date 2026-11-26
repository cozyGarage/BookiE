use std::sync::{Arc, Weak};

use bookie_app_services::selection_stats::{column_stats, column_stats_in, tooltip};

use crate::*;

#[derive(Default)]
pub(crate) struct SelectionStatsCache {
    key: Option<SelectionStatsKey>,
    items: Vec<(&'static str, String)>,
    #[cfg(test)]
    computations: usize,
}

struct SelectionStatsKey {
    selection: Selection,
    column: usize,
    source: Weak<dyn bookie_gpui::source::ResultSource>,
    source_rows: u64,
    edit_generation: u64,
}

impl SelectionStatsCache {
    fn matches(
        &self,
        selection: Selection,
        column: usize,
        source: &bookie_gpui::source::Source,
        source_rows: u64,
        edit_generation: u64,
    ) -> bool {
        self.key.as_ref().is_some_and(|key| {
            key.selection == selection
                && key.column == column
                && Weak::ptr_eq(&key.source, &Arc::downgrade(source))
                && key.source_rows == source_rows
                && key.edit_generation == edit_generation
        })
    }

    fn store(
        &mut self,
        selection: Selection,
        column: usize,
        source: &bookie_gpui::source::Source,
        source_rows: u64,
        edit_generation: u64,
        items: Vec<(&'static str, String)>,
    ) {
        self.key = Some(SelectionStatsKey {
            selection,
            column,
            source: Arc::downgrade(source),
            source_rows,
            edit_generation,
        });
        self.items = items;
        #[cfg(test)]
        {
            self.computations += 1;
        }
    }

    fn clear(&mut self) {
        self.key = None;
        self.items.clear();
    }
}

pub(crate) fn stats_line(text: &str) -> String {
    text.lines().collect::<Vec<_>>().join("  ·  ")
}

impl GridView {
    pub(crate) fn selection_stats_enabled(&self) -> bool {
        self.selection.is_some()
    }

    pub(crate) fn selection_stat_items(&mut self) -> Vec<(&'static str, String)> {
        let (Some(selection), Some(cursor)) = (self.selection, self.cell_cursor) else {
            self.selection_stats_cache.clear();
            return Vec::new();
        };
        if selection.count() < 2 || !selection.copyable() {
            self.selection_stats_cache.clear();
            return Vec::new();
        }
        let source_rows = self.source.total();
        if !self.selection_stats_cache.matches(selection, cursor.column, &self.source, source_rows, self.save.generation) {
            let items = {
                let rows = self.selected_row_set(selection);
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
            };
            self.selection_stats_cache.store(
                selection,
                cursor.column,
                &self.source,
                source_rows,
                self.save.generation,
                items,
            );
        }
        self.selection_stats_cache.items.clone()
    }

    pub(crate) fn selection_stats_segment(&mut self, theme: Theme) -> Option<Div> {
        let items = self.selection_stat_items();
        (!items.is_empty()).then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .children(items.into_iter().enumerate().map(|(index, (label, value))| {
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
        let stats = {
            let rows = self.selected_row_set(selection);
            column_stats(&column_infos_for(self.source.columns()), &rows)
        };
        self.status = if stats.is_empty() {
            "The selected rows have no numeric columns to summarise".into()
        } else {
            stats_line(&tooltip(&stats))
        };
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn selection_stats_computations(&self) -> usize {
        self.selection_stats_cache.computations
    }
}

#[cfg(test)]
#[path = "selection_stats_ui_tests.rs"]
mod tests;
