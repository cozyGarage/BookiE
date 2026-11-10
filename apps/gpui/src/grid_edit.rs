use std::collections::HashMap;

use bookie_gpui::cursor::Cursor;
use bookie_gpui::edit::{is_editable, parse_edit};
use bookie_gpui::model::Cell;
use gpui::{Context, Div, Focusable, MouseButton, MouseDownEvent, Window, prelude::*, rgb};

use crate::text_input::InputEvent;
use crate::ui_theme::ThemeExt;
use crate::{GridView, display_cell};
use bookie_gpui::theme::Theme;

pub type Edits = HashMap<(u64, usize), Cell>;

pub(super) fn cursor_row(cursor: Option<Cursor>, row: usize) -> bool {
    cursor.is_some_and(|cursor| cursor.row == row)
}

pub(super) fn cursor_column_background(cursor: Option<Cursor>, column: usize, theme: Theme) -> Option<u32> {
    cursor.filter(|cursor| cursor.column == column).map(|_| theme.chrome)
}

fn cursor_cell_background(cursor: Option<Cursor>, row: usize, column: usize, theme: Theme) -> Option<u32> {
    let cursor = cursor?;
    if cursor_row(Some(cursor), row) && cursor.column == column {
        Some(theme.background)
    } else {
        cursor_column_background(Some(cursor), column, theme)
    }
}

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

    pub fn data_cell(&self, source: u64, column: usize, position: usize, cell: &Cell, cx: &mut Context<Self>) -> Div {
        let width = self.source.columns().get(column).map_or(120.0, |column| column.width);
        if self
            .editing
            .as_ref()
            .is_some_and(|e| e.row == source && e.column == column)
        {
            return crate::flex_cell(width).px_0().child(self.editor.clone());
        }
        let theme = Theme::current(cx);
        let cell = self.edits.get(&(source, column)).unwrap_or(cell);
        let mut item = display_cell(width, cell, theme);
        if matches!(cell, Cell::Null) {
            item = item.italic().text_color(rgb(theme.muted_foreground));
        }
        if self.edits.contains_key(&(source, column)) {
            item = item.border_b_2().border_color(rgb(theme.warning));
        }
        if let Some(background) = cursor_cell_background(self.cell_cursor, position, column, theme) {
            item = item.bg(rgb(background));
        }
        if self.cell_cursor == Some(Cursor { row: position, column }) {
            item = item.border_2().border_color(rgb(theme.info));
        }
        item.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                this.cell_cursor = Some(Cursor { row: position, column });
                if event.click_count == 2 {
                    this.start_edit(source, column, window, cx);
                    cx.stop_propagation();
                }
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_cell_highlights_its_row_column_and_intersection() {
        let theme = Theme::dbx_light();
        let cursor = Some(Cursor { row: 4, column: 2 });

        assert!(cursor_row(cursor, 4));
        assert!(!cursor_row(cursor, 5));
        assert_eq!(cursor_column_background(cursor, 2, theme), Some(theme.chrome));
        assert_eq!(cursor_column_background(cursor, 1, theme), None);
        assert_eq!(cursor_cell_background(cursor, 4, 2, theme), Some(theme.background));
        assert_eq!(cursor_cell_background(cursor, 5, 2, theme), Some(theme.chrome));
        assert_eq!(cursor_cell_background(cursor, 4, 1, theme), None);
    }
}
