use std::collections::HashMap;

use bookie_gpui::edit::{is_editable, parse_edit};
use bookie_gpui::model::{Cell, CellText};
use gpui::{Context, Focusable, Window};

use crate::GridView;
use crate::text_input::InputEvent;

pub type Edits = HashMap<(u64, usize), Cell>;

pub struct Editing {
    pub row: u64,
    pub column: usize,
    pub kind: &'static str,
    pub full_value: bool,
}

const MAX_FULL_VALUE_EDIT_BYTES: usize = 1024 * 1024;

impl GridView {
    pub(crate) fn apply_active_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
    }

    pub(crate) fn cancel_active_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_edit(window, cx);
    }

    pub fn on_editor_event(&mut self, event: &InputEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            InputEvent::Submit => self.commit_edit(window, cx),
            InputEvent::Cancel => self.stop_edit(window, cx),
            InputEvent::Changed => {}
        }
    }

    fn stop_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cell_edit.active = None;
        self.cell_edit.editor.update(cx, |editor, cx| {
            editor.set_text_align(gpui::TextAlign::Left, cx);
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.cell_edit.active.as_ref() else { return };
        if self.save.pending.is_some() || self.save.busy {
            self.status = "Save or cancel the review before editing more cells".into();
            self.stop_edit(window, cx);
            return;
        }
        let full_value = editing.full_value;
        let text = if full_value {
            self.cell_edit.value_editor.read(cx).value().to_string()
        } else {
            self.cell_edit.editor.read(cx).content().to_string()
        };
        if editing.kind == "null" && text.is_empty() {
            self.stop_edit(window, cx);
            return;
        }
        match parse_edit(editing.kind, &text) {
            Ok(cell) => {
                let key = (editing.row, editing.column);
                self.record_edit(key, cell);
                self.edits_changed();
                let count = self.cell_edit.edits.len();
                self.status = if self.can_save_edits() {
                    format!("{count} change(s) pending; press Ctrl+S to save")
                } else {
                    format!("{count} edited in this view only; nothing is saved.")
                };
                self.stop_edit(window, cx);
            }
            Err(reason) => {
                self.status = format!("Not applied: {reason}");
                cx.notify();
            }
        }
    }

    pub fn start_edit(&mut self, row: u64, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.form_open() {
            self.status = "Finish or cancel the open form before editing cells".into();
            cx.notify();
            return;
        }
        if self.save.pending.is_some() || self.save.busy {
            self.status = "Save or cancel the review before editing more cells".into();
            cx.notify();
            return;
        }
        let cell = self.cell_at(row, column);
        let allowed = if self.live.is_some() {
            !matches!(cell, Cell::Undecodable(_))
        } else {
            is_editable(column, &cell)
        };
        if !self.source.editable() || !allowed {
            self.status = "This cell cannot be edited".into();
            cx.notify();
            return;
        }
        let text = if matches!(cell, Cell::Null) {
            String::new()
        } else {
            cell.display()
        };
        let full_value = matches!(cell, Cell::Text(_) | Cell::JsonText(_) | Cell::Json(_))
            && (matches!(cell, Cell::JsonText(_) | Cell::Json(_))
                || text.len() > 512
                || text.contains(['\n', '\r', '\t', '\0']));
        if full_value && text.len() > MAX_FULL_VALUE_EDIT_BYTES {
            self.status = "This value exceeds the 1 MiB full-value editor limit".into();
            cx.notify();
            return;
        }
        let is_numeric = crate::right_aligned(
            self.source.columns().get(column).map_or(false, |c| c.numeric),
            &cell,
        );
        self.cell_edit.active = Some(Editing {
            row,
            column,
            kind: cell.kind(),
            full_value,
        });
        if full_value {
            self.cell_edit
                .value_editor
                .update(cx, |editor, cx| editor.set_value(&text, window, cx));
            window.focus(&self.cell_edit.value_editor.read(cx).focus_handle(cx), cx);
        } else {
            self.cell_edit.editor.update(cx, |editor, cx| {
                editor.set_text_align(
                    if is_numeric { gpui::TextAlign::Right } else { gpui::TextAlign::Left },
                    cx,
                );
                editor.set_content(&text, cx);
            });
            window.focus(&self.cell_edit.editor.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }

    pub fn cell_at(&self, row: u64, column: usize) -> Cell {
        self.cell_edit.edits
            .get(&(row, column))
            .cloned()
            .unwrap_or_else(|| self.source.row(row).swap_remove(column))
    }

    pub(crate) fn edit_clicked_row(
        &mut self,
        row: usize,
        clicked_column: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.compare.is_some() || row >= self.visible_count() {
            return;
        }
        let column = clicked_column.unwrap_or_else(|| self.cell_cursor.map_or(0, |cursor| cursor.column));
        let source = self.source_row(self.base() + row as u64);
        self.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row, column });
        self.start_edit(source, column, window, cx);
    }
}
