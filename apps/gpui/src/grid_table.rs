use gpui::{App, Context, IntoElement, Window, div, prelude::*, px};
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableState};

use crate::GridView;
use bookie_gpui::preview::cell_preview;

#[derive(Default)]
pub struct GridDelegate {
    columns: Vec<(String, f32)>,
    rows: Vec<Vec<String>>,
    editor: Option<(usize, usize, gpui::AnyView)>,
}

impl TableDelegate for GridDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
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
            _ => div().child(self.rows[row_ix][col_ix].clone()),
        }
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
        let rows: Vec<Vec<String>> = if self.compare.is_some() {
            Vec::new()
        } else {
            (0..self.visible_count())
                .map(|position| {
                    let source = self.source_row(self.base() + position as u64);
                    self.source.row(source).iter().map(cell_preview).collect()
                })
                .collect()
        };
        let editor = self.editing.as_ref().and_then(|editing| {
            (0..self.visible_count())
                .find(|position| self.source_row(self.base() + *position as u64) == editing.row)
                .map(|position| (position, editing.column, gpui::AnyView::from(self.editor.clone())))
        });
        self.table.update(cx, |table, cx| {
            let delegate = table.delegate();
            let changed = delegate.columns != columns
                || delegate.rows != rows
                || delegate.editor.as_ref().map(|(r, c, _)| (*r, *c)) != editor.as_ref().map(|(r, c, _)| (*r, *c));
            if !changed {
                return;
            }
            let delegate = table.delegate_mut();
            delegate.columns = columns;
            delegate.rows = rows;
            delegate.editor = editor;
            table.refresh(cx);
        });
    }

    pub(crate) fn data_table(&self) -> DataTable<GridDelegate> {
        DataTable::new(&self.table).stripe(true)
    }
}
