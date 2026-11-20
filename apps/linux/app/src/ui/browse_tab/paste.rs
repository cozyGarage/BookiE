use relm4::ComponentSender;
use relm4::gtk::{self, gdk, glib};

pub(super) use bookie_app_services::clipboard_grid::parse_clipboard_grid;
use bookie_app_services::clipboard_grid::{PasteCell, paste_plan};

use super::value_parse::parse_pasted_cell_with_style;
use super::*;

const MAX_PASTE_BYTES: usize = bookie_core::import::MAX_FIELD_BYTES;

fn paste_size_allowed(size: usize) -> bool {
    size <= MAX_PASTE_BYTES
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct PasteSnapshot {
    page_request_id: Option<uuid::Uuid>,
    row_keys: Vec<Option<crate::services::change_tracker::RowKey>>,
}

#[derive(Debug)]
pub(in crate::ui) struct PasteAnchor {
    pub(in crate::ui) row: u32,
    pub(in crate::ui) col: usize,
    pub(in crate::ui) row_key: Vec<Value>,
    pub(in crate::ui) displayed: Option<Vec<usize>>,
}

#[derive(Debug)]
pub(in crate::ui) struct PasteTextInput {
    pub(in crate::ui) row: u32,
    pub(in crate::ui) col: usize,
    pub(in crate::ui) row_key: Vec<Value>,
    pub(in crate::ui) text: String,
    pub(in crate::ui) displayed: Option<Vec<usize>>,
    pub(in crate::ui) snapshot: PasteSnapshot,
}

impl PasteSnapshot {
    fn new(
        page_request_id: Option<uuid::Uuid>,
        row_keys: Vec<Option<crate::services::change_tracker::RowKey>>,
    ) -> Self {
        Self {
            page_request_id,
            row_keys,
        }
    }

    fn matches(
        &self,
        current_page_request_id: Option<uuid::Uuid>,
        target_rows: &[u32],
        mut row_key_at: impl FnMut(u32) -> Option<crate::services::change_tracker::RowKey>,
    ) -> bool {
        if self.page_request_id != current_page_request_id {
            return false;
        }
        target_rows.iter().all(|row| {
            let Some(expected) = self.row_keys.get(*row as usize).and_then(Option::as_ref) else {
                return false;
            };
            row_key_at(*row).as_ref() == Some(expected)
        })
    }
}

pub(super) fn paste_shortcut(sender: ComponentSender<BrowseTab>) -> gtk::Shortcut {
    gtk::Shortcut::builder()
        .trigger(&crate::ui::shortcut::parse("<Primary>v"))
        .action(&gtk::CallbackAction::new(move |widget, _| {
            match crate::ui::grid::focused_cell_identity(widget).or_else(|| crate::ui::grid::last_clicked_cell(widget))
            {
                Some((row, col, row_key)) => sender.input(BrowseTabInput::Paste(PasteAnchor {
                    row,
                    col,
                    row_key,
                    displayed: crate::ui::grid::displayed_column_order(widget),
                })),
                None => sender.input(BrowseTabInput::PasteNotSupported),
            }
            glib::Propagation::Stop
        }))
        .build()
}

impl BrowseTab {
    pub(super) fn handle_paste(&mut self, anchor: PasteAnchor, sender: ComponentSender<Self>) {
        let PasteAnchor {
            row,
            col,
            row_key,
            displayed,
        } = anchor;
        if !self.row_position_still_holds(row, &row_key) {
            let _ = sender.output(BrowseTabOutput::ShowToast(bookie_app_services::tr!(
                "The rows changed since you clicked the cell, so nothing was pasted"
            )));
            return;
        }
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let clipboard = display.clipboard();
        let snapshot = self.capture_paste_snapshot();
        glib::spawn_future_local(async move {
            if let Ok(Some(text)) = clipboard.read_text_future().await {
                if !paste_size_allowed(text.len()) {
                    let _ = sender.output(BrowseTabOutput::ShowToast(bookie_app_services::tr!(
                        "Clipboard text is too large to paste; nothing was pasted"
                    )));
                    return;
                }
                sender.input(BrowseTabInput::PasteText(PasteTextInput {
                    row,
                    col,
                    row_key,
                    text: text.to_string(),
                    displayed,
                    snapshot,
                }));
            }
        });
    }

    fn check_paste_cell(&self, cell: &PasteCell) -> Result<Value, String> {
        let column = self.current_columns.get(cell.col);
        let row = self.row_object_at(cell.row);
        let allowed = column
            .zip(row.as_ref())
            .is_some_and(|(column, row)| crate::ui::grid::cell_allows_inline_edit(column, &row.cell_value(cell.col)));
        let name = column.map(|column| column.name.as_str()).unwrap_or_default();
        if !allowed {
            return Err(
                bookie_app_services::tr!("{column} on row {row} can't be edited, so nothing was pasted")
                    .replace("{column}", name)
                    .replace("{row}", &(cell.row + 1).to_string()),
            );
        }
        let current = row.as_ref().map(|row| row.cell_value(cell.col));
        parse_pasted_cell_with_style(cell.text.as_deref(), column, &self.parse_hooks, current.as_ref()).map_err(
            |message| {
                bookie_app_services::tr!("{column} on row {row}: {message}. Nothing was pasted.")
                    .replace("{column}", name)
                    .replace("{row}", &(cell.row + 1).to_string())
                    .replace("{message}", &message)
            },
        )
    }

    pub(super) fn handle_paste_text(&mut self, input: PasteTextInput, sender: ComponentSender<Self>) {
        let PasteTextInput {
            row,
            col,
            row_key,
            text,
            displayed,
            snapshot,
        } = input;
        if snapshot.page_request_id != self.page_requests.current_id() || !self.row_position_still_holds(row, &row_key)
        {
            let _ = sender.output(BrowseTabOutput::ShowToast(bookie_app_services::tr!(
                "The rows changed since you clicked the cell, so nothing was pasted"
            )));
            return;
        }
        let loaded = self.row_count_loaded();
        let displayed = displayed.unwrap_or_else(|| (0..self.current_columns.len()).collect());
        let cells = match paste_plan(row, col, &parse_clipboard_grid(&text), loaded, &displayed) {
            Ok(cells) => cells,
            Err(message) => {
                let _ = sender.output(BrowseTabOutput::ShowToast(message));
                return;
            }
        };
        let target_rows: Vec<u32> = cells.iter().map(|cell| cell.row).collect();
        if !snapshot.matches(self.page_requests.current_id(), &target_rows, |position| {
            self.paste_row_key_at(position)
        }) {
            let _ = sender.output(BrowseTabOutput::ShowToast(bookie_app_services::tr!(
                "That row moved while you were editing. The change was not applied."
            )));
            return;
        }
        let values = match cells
            .iter()
            .map(|cell| self.check_paste_cell(cell))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(values) => values,
            Err(message) => {
                let _ = sender.output(BrowseTabOutput::ShowToast(message));
                return;
            }
        };
        for (cell, value) in cells.iter().zip(values) {
            self.stage_grid_cell_value(cell.row, cell.col, value, &sender);
        }
    }

    pub(super) fn capture_paste_snapshot(&self) -> PasteSnapshot {
        let store = self.row_store();
        let pk_indices = self.pk_indices();
        let row_keys = (0..self.row_count_loaded())
            .map(|position| store.as_ref()?.row_key_at(position, &pk_indices))
            .collect();
        PasteSnapshot::new(self.page_requests.current_id(), row_keys)
    }

    fn paste_row_key_at(&self, position: u32) -> Option<crate::services::change_tracker::RowKey> {
        self.row_store()?.row_key_at(position, &self.pk_indices())
    }

    pub(super) fn row_count_loaded(&self) -> u32 {
        self.current_selection
            .as_ref()
            .map_or(0, |selection| selection.n_items())
    }
}

#[cfg(test)]
#[path = "paste_tests.rs"]
mod tests;
