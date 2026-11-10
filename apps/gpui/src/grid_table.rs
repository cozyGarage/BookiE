use std::sync::Arc;

use gpui::{App, Context, IntoElement, Window, div, prelude::*, px};
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableState};

use crate::GridView;
use bookie_gpui::preview::cell_preview;
use bookie_gpui::source::Source;

#[derive(Default)]
pub struct GridDelegate {
    columns: Vec<(String, f32)>,
    source: Option<Source>,
    source_rows: Vec<u64>,
    cached: Option<(usize, Vec<String>)>,
    editor: Option<(usize, usize, gpui::AnyView)>,
}

impl TableDelegate for GridDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.source_rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let (name, width) = &self.columns[col_ix];
        Column::new(name.clone(), name.clone())
            .width(px(*width))
            .resizable(true)
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        match &self.editor {
            Some((row, column, view)) if *row == row_ix && *column == col_ix => div().flex_1().child(view.clone()),
            _ => div().child(self.row_cell_text(row_ix, col_ix)),
        }
    }
}

impl GridDelegate {
    // Cells of one row render back to back, so one fetch per row serves all its columns.
    fn row_cell_text(&mut self, row_ix: usize, col_ix: usize) -> String {
        if self.cached.as_ref().is_none_or(|(row, _)| *row != row_ix) {
            let cells = match (&self.source, self.source_rows.get(row_ix)) {
                (Some(source), Some(&index)) => source.row(index).iter().map(cell_preview).collect(),
                _ => Vec::new(),
            };
            self.cached = Some((row_ix, cells));
        }
        self.cached
            .as_ref()
            .and_then(|(_, cells)| cells.get(col_ix))
            .cloned()
            .unwrap_or_default()
    }
}

impl GridView {
    pub(crate) fn sync_table(&self, cx: &mut Context<Self>) {
        let columns: Vec<(String, f32)> = self
            .source
            .columns()
            .iter()
            .map(|column| (column.name.clone(), column.width))
            .collect();
        let source_rows: Vec<u64> = if self.compare.is_some() {
            Vec::new()
        } else {
            (0..self.visible_count())
                .map(|position| self.source_row(self.base() + position as u64))
                .collect()
        };
        let editor = self.editing.as_ref().and_then(|editing| {
            source_rows
                .iter()
                .position(|row| *row == editing.row)
                .map(|position| (position, editing.column, gpui::AnyView::from(self.editor.clone())))
        });
        self.table.update(cx, |table, cx| {
            let cell = table_cell(self.cell_cursor).or((!source_rows.is_empty()).then_some((0, 0)));
            if table.selected_cell() != cell {
                match cell {
                    Some((row, column)) => table.set_selected_cell(row, column, cx),
                    None => table.clear_selection(cx),
                }
            }
            let delegate = table.delegate_mut();
            delegate.cached = None;
            let same_source = delegate
                .source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &self.source));
            let changed = !same_source
                || delegate.columns != columns
                || delegate.source_rows != source_rows
                || delegate.editor.as_ref().map(|(r, c, _)| (*r, *c)) != editor.as_ref().map(|(r, c, _)| (*r, *c));
            if !changed {
                return;
            }
            delegate.columns = columns;
            delegate.source = Some(self.source.clone());
            delegate.source_rows = source_rows;
            delegate.editor = editor;
            table.refresh(cx);
        });
    }

    pub(crate) fn data_table(&self) -> DataTable<GridDelegate> {
        DataTable::new(&self.table).stripe(true)
    }
}

pub fn table_cell(cursor: Option<bookie_gpui::cursor::Cursor>) -> Option<(usize, usize)> {
    cursor.map(|cursor| (cursor.row, cursor.column))
}

pub fn cursor_at(cell: Option<(usize, usize)>) -> Option<bookie_gpui::cursor::Cursor> {
    cell.map(|(row, column)| bookie_gpui::cursor::Cursor { row, column })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_gpui::cursor::Cursor;
    use bookie_gpui::source::{ResultSource, Synthetic};

    #[test]
    fn a_cell_reads_the_source_row_it_maps_to_and_keeps_that_row_cached() {
        let source: Source = Arc::new(Synthetic::new(10));
        let mut delegate = GridDelegate {
            source: Some(source),
            source_rows: vec![3, 7],
            ..Default::default()
        };
        let expected = cell_preview(&Synthetic::new(10).row(7)[1]);
        assert_eq!(delegate.row_cell_text(1, 1), expected);
        assert_eq!(delegate.cached.as_ref().map(|(row, _)| *row), Some(1));
        assert_eq!(delegate.row_cell_text(1, 1), expected);
        assert_eq!(delegate.row_cell_text(9, 0), "");
    }

    #[test]
    fn a_cursor_survives_the_trip_through_the_table_cell() {
        let cursor = Some(Cursor { row: 3, column: 2 });
        assert_eq!(cursor_at(table_cell(cursor)), cursor);
        assert_eq!(cursor_at(table_cell(None)), None);
    }

    #[test]
    fn a_table_cell_survives_the_trip_through_the_cursor() {
        assert_eq!(table_cell(cursor_at(Some((0, 5)))), Some((0, 5)));
        assert_eq!(table_cell(cursor_at(None)), None);
    }
}
