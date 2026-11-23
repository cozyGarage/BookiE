use bookie_app_services::clipboard_grid::{parse_clipboard_grid, paste_plan};
use bookie_gpui::edit::{is_editable, parse_edit};
use bookie_gpui::model::{Cell, CellText};

use super::*;

pub(crate) struct PasteTarget {
    pub row: u64,
    pub column: usize,
    pub text: Option<String>,
    pub current: Cell,
}

pub(crate) fn staged_paste(targets: &[PasteTarget]) -> Result<Vec<((u64, usize), Cell)>, String> {
    let mut staged = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        let value = match &target.text {
            None => Cell::Null,
            Some(text) => {
                parse_edit(target.current.kind(), text).map_err(|reason| format!("row {}: {reason}", index + 1))?
            }
        };
        if value != target.current {
            staged.push(((target.row, target.column), value));
        }
    }
    Ok(staged)
}

impl GridView {
    pub(crate) fn paste_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "paste-cells",
            label: "Paste cells from the clipboard at the active cell".to_string(),
            enabled: self.cell_cursor.is_some() && self.source.editable() && self.compare.is_none(),
        }]
    }

    fn paste_targets(&self, text: &str, cx: &App) -> Result<Vec<PasteTarget>, String> {
        let cursor = self.cell_cursor.ok_or("Click the cell to paste into first")?;
        let projection = self.table.read(cx).delegate().clone_projection();
        let displayed: Vec<usize> = (0..projection.len())
            .filter_map(|index| projection.source_index(index))
            .collect();
        let plan = paste_plan(
            cursor.row as u32,
            cursor.column,
            &parse_clipboard_grid(text),
            self.visible_count() as u32,
            &displayed,
        )?;
        let allowed = |column: usize, cell: &Cell| {
            if self.live.is_some() {
                !matches!(cell, Cell::Undecodable(_))
            } else {
                is_editable(column, cell)
            }
        };
        plan.into_iter()
            .map(|cell| {
                let row = self.source_row(self.base() + u64::from(cell.row));
                let current = self.cell_at(row, cell.col);
                if !allowed(cell.col, &current) {
                    return Err(format!(
                        "A cell in column {} cannot be edited, so nothing was pasted",
                        cell.col + 1
                    ));
                }
                Ok(PasteTarget {
                    row,
                    column: cell.col,
                    text: cell.text,
                    current,
                })
            })
            .collect()
    }

    pub(crate) fn paste_cells(&mut self, cx: &mut Context<Self>) {
        if self.save.pending.is_some() || self.save.busy {
            self.status = "Save or cancel the review before pasting".into();
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.status = "The clipboard has no text to paste".into();
            return;
        };
        let outcome = self.paste_targets(&text, cx).and_then(|targets| staged_paste(&targets));
        match outcome {
            Ok(staged) if staged.is_empty() => self.status = "The pasted values equal the current ones".into(),
            Ok(staged) => {
                let count = staged.len();
                for (key, value) in staged {
                    self.record_edit(key, value);
                }
                self.edits_changed();
                self.status = format!("{count} cell(s) pasted; review and save to apply");
            }
            Err(reason) => self.status = format!("Nothing was pasted: {reason}"),
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "paste_ui_tests.rs"]
mod tests;
