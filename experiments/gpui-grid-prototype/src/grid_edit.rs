use std::collections::HashMap;

use bookie_gpui_grid_prototype::cursor::Cursor;
use bookie_gpui_grid_prototype::edit::{is_editable, parse_edit};
use bookie_gpui_grid_prototype::model::Cell;
use gpui::{Context, Div, Focusable, MouseButton, MouseDownEvent, Window, div, prelude::*, px, rgb};

use crate::text_input::InputEvent;
use crate::{CELL_WIDTHS, GridView, cell_div};

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
        window.focus(&self.focus);
        cx.notify();
    }

    fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.editing.as_ref() else { return };
        let text = self.editor.read(cx).content().to_string();
        match parse_edit(editing.kind, &text) {
            Ok(cell) => {
                println!(
                    "PROTOTYPE_EDIT applied row={} column={} kind={} value={}",
                    editing.row,
                    editing.column,
                    cell.kind(),
                    cell.display()
                );
                self.edits.insert((editing.row, editing.column), cell);
                self.status = format!("{} edited in this view only; nothing is saved.", self.edits.len());
                self.stop_edit(window, cx);
            }
            Err(reason) => {
                println!("PROTOTYPE_EDIT refused kind={} reason={reason}", editing.kind);
                self.status = format!("Not applied: {reason}");
                cx.notify();
            }
        }
    }

    pub fn start_edit(&mut self, row: u64, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        let cell = self.cell_at(row, column);
        if !is_editable(column, &cell) {
            self.status = "This cell cannot be edited".into();
            cx.notify();
            return;
        }
        let text = if matches!(cell, Cell::Null) {
            String::new()
        } else {
            cell.display()
        };
        self.editing = Some(Editing {
            row,
            column,
            kind: cell.kind(),
        });
        self.editor.update(cx, |editor, cx| editor.set_content(&text, cx));
        window.focus(&self.editor.read(cx).focus_handle(cx));
        cx.notify();
    }

    pub fn cell_at(&self, row: u64, column: usize) -> Cell {
        self.edits
            .get(&(row, column))
            .cloned()
            .unwrap_or_else(|| bookie_gpui_grid_prototype::model::row_at(row).swap_remove(column))
    }

    pub fn data_cell(&self, source: u64, column: usize, position: usize, cx: &mut Context<Self>) -> Div {
        let width = CELL_WIDTHS[column];
        if self
            .editing
            .as_ref()
            .is_some_and(|e| e.row == source && e.column == column)
        {
            return div().w(px(width)).flex_none().child(self.editor.clone());
        }
        let cell = self.cell_at(source, column);
        let mut item = cell_div(width, cell.display());
        if self.edits.contains_key(&(source, column)) {
            item = item.border_b_2().border_color(rgb(0xe0a96d));
        }
        if self.cell_cursor == Some(Cursor { row: position, column }) {
            item = item.border_2().border_color(rgb(0x3584e4));
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
