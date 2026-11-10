use std::collections::HashMap;

use bookie_gpui::edit::{is_editable, parse_edit};
use bookie_gpui::model::Cell;
use gpui::{Context, Focusable, Window};

use crate::GridView;
use crate::text_input::InputEvent;

pub type Edits = HashMap<(u64, usize), Cell>;

pub struct Editing {
    pub row: u64,
    pub column: usize,
    pub kind: &'static str,
}

impl GridView {
    pub fn on_editor_event(&mut self, event: &InputEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            InputEvent::Submit => self.commit_edit(window, cx),
            InputEvent::Cancel => self.stop_edit(window, cx),
            InputEvent::Changed => {}
        }
    }

    fn stop_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.editing.as_ref() else { return };
        let text = self.editor.read(cx).content().to_string();
        match parse_edit(editing.kind, &text) {
            Ok(cell) => {
                self.edits.insert((editing.row, editing.column), cell);
                self.status = format!("{} edited in this view only; nothing is saved.", self.edits.len());
                self.stop_edit(window, cx);
            }
            Err(reason) => {
                self.status = format!("Not applied: {reason}");
                cx.notify();
            }
        }
    }

    pub fn start_edit(&mut self, row: u64, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        let cell = self.cell_at(row, column);
        if !self.source.editable() || !is_editable(column, &cell) {
            self.status = "This cell cannot be edited".into();
            cx.notify();
            return;
        }
        let text = if matches!(cell, Cell::Null) {
            String::new()
        } else {
            cell.display()
        };
        if text.contains('\n') {
            self.status = "Multiline values cannot be edited in this single-line prototype".into();
            cx.notify();
            return;
        }
        self.editing = Some(Editing {
            row,
            column,
            kind: cell.kind(),
        });
        self.editor.update(cx, |editor, cx| editor.set_content(&text, cx));
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub fn cell_at(&self, row: u64, column: usize) -> Cell {
        self.edits
            .get(&(row, column))
            .cloned()
            .unwrap_or_else(|| self.source.row(row).swap_remove(column))
    }

    pub(crate) fn edit_clicked_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.compare.is_some() || row >= self.visible_count() {
            return;
        }
        let column = self.cell_cursor.map_or(0, |cursor| cursor.column);
        let source = self.source_row(self.base() + row as u64);
        self.cell_cursor = Some(bookie_gpui::cursor::Cursor { row, column });
        self.start_edit(source, column, window, cx);
    }
}
