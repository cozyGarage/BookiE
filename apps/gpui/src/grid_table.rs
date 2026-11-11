use std::sync::Arc;

use gpui::{App, Context, IntoElement, Window, div, prelude::*, px};
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableState};

use crate::GridView;
use crate::grid_edit::Edits;
use bookie_gpui::model::{Cell, Diff};
use bookie_gpui::preview::cell_preview;
use bookie_gpui::source::Source;

#[derive(Default)]
pub struct RowMap {
    base: u64,
    count: usize,
    kept: Option<Arc<Vec<u32>>>,
    compare: Option<Arc<Diff>>,
}

impl PartialEq for RowMap {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.count == other.count
            && same_arc(&self.kept, &other.kept)
            && same_arc(&self.compare, &other.compare)
    }
}

fn same_arc<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

impl RowMap {
    fn source_row(&self, row: usize) -> Option<u64> {
        let dataset = self.base + row as u64;
        match &self.kept {
            None => Some(dataset),
            Some(kept) => kept.get(dataset as usize).map(|index| u64::from(*index)),
        }
    }
}

#[derive(Default)]
pub struct GridDelegate {
    columns: Vec<(String, f32)>,
    numeric: Vec<bool>,
    source: Option<Source>,
    map: RowMap,
    cached: Option<(usize, Vec<(String, bool)>)>,
    edits: Edits,
    editor: Option<(usize, usize, gpui::AnyView)>,
}

impl TableDelegate for GridDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.map.count
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
            _ => {
                let (text, right) = self.label_of(row_ix, col_ix);
                div().w_full().flex().when(right, |cell| cell.justify_end()).child(text)
            }
        }
    }
}

impl GridDelegate {
    fn is_changed(
        &self,
        source: &Source,
        columns: &[(String, f32)],
        numeric: &[bool],
        map: &RowMap,
        edits: &Edits,
        editor: &Option<(usize, usize, gpui::AnyView)>,
    ) -> bool {
        let same_source = self.source.as_ref().is_some_and(|current| Arc::ptr_eq(current, source));
        !same_source
            || self.columns != columns
            || self.numeric != numeric
            || self.map != *map
            || self.edits != *edits
            || self.editor.as_ref().map(|(row, column, _)| (*row, *column))
                != editor.as_ref().map(|(row, column, _)| (*row, *column))
    }

    pub(crate) fn label_of(&mut self, row_ix: usize, col_ix: usize) -> (String, bool) {
        if self.cached.as_ref().is_none_or(|(row, _)| *row != row_ix) {
            let numeric = &self.numeric;
            let labels = match (&self.map.compare, &self.source) {
                (Some(diff), _) => compare_labels(diff, row_ix, numeric),
                (None, Some(source)) => match self.map.source_row(row_ix) {
                    Some(index) => {
                        let mut cells = source.row(index);
                        for (column, cell) in cells.iter_mut().enumerate() {
                            if let Some(edit) = self.edits.get(&(index, column)) {
                                *cell = edit.clone();
                            }
                        }
                        labels(&cells, numeric)
                    }
                    None => Vec::new(),
                },
                (None, None) => Vec::new(),
            };
            self.cached = Some((row_ix, labels));
        }
        self.cached
            .as_ref()
            .and_then(|(_, labels)| labels.get(col_ix))
            .cloned()
            .unwrap_or_default()
    }
}

impl GridView {
    fn row_map(&self) -> RowMap {
        if let Some(diff) = &self.compare {
            return RowMap {
                base: 0,
                count: diff.rows.len(),
                kept: None,
                compare: Some(diff.clone()),
            };
        }
        RowMap {
            base: self.base(),
            count: self.visible_count(),
            kept: match &self.mode {
                crate::Mode::All => None,
                crate::Mode::Filtered { kept, .. } => Some(kept.clone()),
            },
            compare: None,
        }
    }

    pub(crate) fn sync_table(&self, cx: &mut Context<Self>) {
        let columns: Vec<(String, f32)> = self
            .source
            .columns()
            .iter()
            .map(|column| (column.name.clone(), column.width))
            .collect();
        let numeric: Vec<bool> = self.source.columns().iter().map(|column| column.numeric).collect();
        let numeric = if self.compare.is_some() {
            std::iter::once(false).chain(numeric).collect()
        } else {
            numeric
        };
        let columns = if self.compare.is_some() {
            std::iter::once(("Change".to_string(), COMPARE_CHANGE_WIDTH))
                .chain(columns)
                .collect()
        } else {
            columns
        };
        let map = self.row_map();
        let editor = self.editing.as_ref().and_then(|editing| {
            let dataset = self.dataset_index_of(editing.row)?.checked_sub(map.base)?;
            (dataset < map.count as u64).then(|| {
                (
                    dataset as usize,
                    editing.column,
                    gpui::AnyView::from(self.editor.clone()),
                )
            })
        });
        self.table.update(cx, |table, cx| {
            let cell = table_cell(self.cell_cursor).or((map.count > 0).then_some((0, 0)));
            if table.selected_cell() != cell {
                match cell {
                    Some((row, column)) => table.set_selected_cell(row, column, cx),
                    None => table.clear_selection(cx),
                }
            }
            let delegate = table.delegate_mut();
            delegate.cached = None;
            if !delegate.is_changed(&self.source, &columns, &numeric, &map, &self.edits, &editor) {
                return;
            }
            delegate.columns = columns;
            delegate.numeric = numeric;
            delegate.source = Some(self.source.clone());
            delegate.map = map;
            delegate.edits = self.edits.clone();
            delegate.editor = editor;
            table.refresh(cx);
        });
    }

    pub(crate) fn data_table(&self) -> DataTable<GridDelegate> {
        DataTable::new(&self.table).stripe(true)
    }
}

const COMPARE_CHANGE_WIDTH: f32 = 70.0;

fn labels(cells: &[Cell], numeric: &[bool]) -> Vec<(String, bool)> {
    cells
        .iter()
        .enumerate()
        .map(|(column, cell)| {
            let right = crate::right_aligned(numeric.get(column).copied().unwrap_or(false), cell);
            (cell_preview(cell), right)
        })
        .collect()
}

fn compare_labels(diff: &Diff, row: usize, numeric: &[bool]) -> Vec<(String, bool)> {
    let Some(row) = diff.rows.get(row) else {
        return Vec::new();
    };
    std::iter::once((row.change.to_string(), false))
        .chain(labels(&row.cells, numeric.get(1..).unwrap_or(&[])))
        .collect()
}

pub fn table_cell(cursor: Option<bookie_results::grid_cursor::Cursor>) -> Option<(usize, usize)> {
    cursor.map(|cursor| (cursor.row, cursor.column))
}

#[cfg(test)]
pub fn cursor_at(cell: Option<(usize, usize)>) -> Option<bookie_results::grid_cursor::Cursor> {
    cell.map(|(row, column)| bookie_results::grid_cursor::Cursor { row, column })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_gpui::model::Cell;
    use bookie_gpui::source::{ResultSource, Synthetic};
    use bookie_results::grid_cursor::Cursor;

    #[test]
    fn a_cell_reads_the_source_row_it_maps_to_and_keeps_that_row_cached() {
        let source: Source = Arc::new(Synthetic::new(10));
        let mut delegate = GridDelegate {
            source: Some(source),
            map: RowMap {
                base: 0,
                count: 2,
                kept: Some(Arc::new(vec![3, 7])),
                compare: None,
            },
            ..Default::default()
        };
        let expected = cell_preview(&Synthetic::new(10).row(7)[1]);
        assert_eq!(delegate.label_of(1, 1).0, expected);
        assert_eq!(delegate.cached.as_ref().map(|(row, _)| *row), Some(1));
        assert_eq!(delegate.label_of(1, 1).0, expected);
        assert_eq!(delegate.label_of(9, 0).0, "");
    }

    #[test]
    fn a_map_covers_only_its_own_page() {
        let plain = RowMap {
            base: 5_000,
            count: 2,
            kept: None,
            compare: None,
        };
        assert_eq!(plain.source_row(1), Some(5_001));
        let filtered = RowMap {
            base: 0,
            count: 2,
            kept: Some(Arc::new(vec![9, 4])),
            compare: None,
        };
        assert_eq!(filtered.source_row(1), Some(4));
        assert_eq!(filtered.source_row(2), None);
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

    #[test]
    fn a_compare_row_shows_its_change_then_its_cells() {
        use bookie_gpui::model::{Diff, DiffRow, DiffSummary};
        let diff = Diff {
            summary: DiffSummary {
                keyed: false,
                equal: 0,
                added: 0,
                removed: 0,
                changed: 1,
                unknown: 0,
            },
            rows: vec![DiffRow {
                change: "changed",
                cells: vec![Cell::Text("a".into()), Cell::Int(7)],
            }],
            capped: false,
        };
        let mut delegate = GridDelegate {
            map: RowMap {
                base: 0,
                count: 1,
                kept: None,
                compare: Some(Arc::new(diff)),
            },
            ..Default::default()
        };
        assert_eq!(delegate.label_of(0, 0).0, "changed");
        assert_eq!(delegate.label_of(0, 2).0, "7");
    }

    #[test]
    fn numbers_and_nulls_in_numeric_columns_are_right_aligned() {
        let flags: Vec<bool> = labels(
            &[Cell::Int(7), Cell::Null, Cell::Text("x".into())],
            &[true, true, false],
        )
        .into_iter()
        .map(|(_, right)| right)
        .collect();
        assert_eq!(flags, vec![true, true, false]);
    }
}
