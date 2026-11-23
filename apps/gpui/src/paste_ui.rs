use bookie_app_services::clipboard_grid::{parse_clipboard_grid, paste_plan};
use bookie_gpui::edit::{is_editable, parse_edit};
use bookie_gpui::model::{Cell, CellText};

use super::*;

const MAX_LITERAL_PASTE_BYTES: usize = 1024 * 1024;

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
        let enabled = self.cell_cursor.is_some() && self.source.editable() && self.compare.is_none();
        vec![
            Action {
                id: "paste-cells",
                label: "Paste cells from the clipboard at the active cell".to_string(),
                enabled,
            },
            Action {
                id: "paste-literal-cell",
                label: "Paste clipboard text into one cell".to_string(),
                enabled,
            },
        ]
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

    fn literal_paste_target(&self, text: &str, cx: &App) -> Result<PasteTarget, String> {
        if text.len() > MAX_LITERAL_PASTE_BYTES {
            return Err("The clipboard text is larger than 1 MiB".into());
        }
        let cursor = self.cell_cursor.ok_or("Click the cell to paste into first")?;
        let projection = self.table.read(cx).delegate().clone_projection();
        let displayed: Vec<usize> = (0..projection.len())
            .filter_map(|index| projection.source_index(index))
            .collect();
        let row = u32::try_from(cursor.row).map_err(|_| "The selected row is no longer available")?;
        let loaded_rows = u32::try_from(self.visible_count()).unwrap_or(u32::MAX);
        let grid = [vec![Some(text.to_owned())]];
        let cell = paste_plan(row, cursor.column, &grid, loaded_rows, &displayed)?
            .into_iter()
            .next()
            .ok_or("The selected cell is no longer available")?;
        let column = self
            .source
            .columns()
            .get(cell.col)
            .ok_or("The selected column is no longer available")?;
        let source_row = self.source_row(self.base() + u64::from(cell.row));
        let current = self.cell_at(source_row, cell.col);
        let text_column = if column.data_type.trim().is_empty() {
            matches!(&current, Cell::Text(_))
        } else {
            let metadata = bookie_core::ColumnInfo {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
                nullable: true,
                primary_key: column.primary_key,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            };
            matches!(
                bookie_core::filter_value_kind(&metadata),
                bookie_core::FilterValueKind::Text
            )
        };
        if !text_column {
            return Err("Literal paste is available only for text columns".into());
        }
        if !matches!(&current, Cell::Text(_) | Cell::Null) {
            return Err("Literal paste is available only for text cells".into());
        }
        Ok(PasteTarget {
            row: source_row,
            column: cell.col,
            text: cell.text,
            current,
        })
    }

    fn apply_paste(&mut self, outcome: Result<Vec<((u64, usize), Cell)>, String>, cx: &mut Context<Self>) {
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
        self.apply_paste(outcome, cx);
    }

    pub(crate) fn paste_literal_cell(&mut self, cx: &mut Context<Self>) {
        if self.save.pending.is_some() || self.save.busy {
            self.status = "Save or cancel the review before pasting".into();
            return;
        }
        if !self.source.editable() || self.compare.is_some() {
            self.status = "The selected result cannot be edited".into();
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.status = "The clipboard has no text to paste".into();
            return;
        };
        let outcome = self
            .literal_paste_target(&text, cx)
            .and_then(|target| staged_paste(&[target]));
        self.apply_paste(outcome, cx);
    }
}

#[cfg(test)]
#[path = "paste_ui_tests.rs"]
mod tests;
