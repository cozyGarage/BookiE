use bookie_app_services::selection_stats::{column_stats, tooltip};

use super::*;

pub(crate) fn stats_line(text: &str) -> String {
    text.lines().collect::<Vec<_>>().join("  ·  ")
}

impl GridView {
    pub(crate) fn selection_stats_enabled(&self) -> bool {
        self.selection.is_some()
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
