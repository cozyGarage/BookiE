use super::*;

impl GridView {
    pub(crate) fn selected_rows(&self, selection: Selection) -> Vec<Vec<Cell>> {
        (selection.first()..=selection.last())
            .map(|index| {
                let source = self.source_row(index);
                let mut row = self.source.row(source);
                for (column, cell) in row.iter_mut().enumerate() {
                    if let Some(edited) = self.edits.get(&(source, column)) {
                        *cell = edited.clone();
                    }
                }
                row
            })
            .collect()
    }

    pub(crate) fn copy_selection(&mut self, with_headers: bool, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else { return };
        if !selection.copyable() {
            self.status = selection.label();
            return;
        }
        let columns = column_infos_for(self.source.columns());
        let text = result_copy::tsv(&columns, self.selected_rows(selection), with_headers);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let noun = if selection.count() == 1 { "row" } else { "rows" };
        self.status = format!(
            "Copied {} {noun}{}",
            selection.count(),
            if with_headers { " with headers" } else { "" }
        );
    }

    pub(crate) fn copy_selection_as(&mut self, format: CopyFormat, cx: &mut Context<Self>) {
        if format == CopyFormat::Insert && (self.grid_is_query || self.catalog.tables.is_empty()) {
            self.status = "Copy as INSERT is available for table rows only.".into();
            return;
        }
        let Some(selection) = self.selection else { return };
        if !selection.copyable() {
            self.status = selection.label();
            return;
        }
        let rows = self.selected_rows(selection);
        let driver_id = self
            .live
            .as_ref()
            .map_or_else(|| "postgres".to_string(), |session| session.driver_id.clone());
        let columns = column_infos_for(self.source.columns());
        let context = CopyContext {
            columns: &columns,
            driver_id: &driver_id,
            target: (!self.grid_is_query).then(|| self.catalog.grid_table.clone()),
            column: self.cell_cursor.map_or(0, |cursor| cursor.column),
        };
        match result_copy::render(format, &context, &rows) {
            Ok(copied) => {
                cx.write_to_clipboard(ClipboardItem::new_string(copied.text));
                let noun = if selection.count() == 1 { "row" } else { "rows" };
                self.status = match copied.note {
                    Some(note) => format!("Copied {} {noun} ({note})", selection.count()),
                    None => format!("Copied {} {noun}", selection.count()),
                };
            }
            Err(message) => self.status = message,
        }
    }

    pub(crate) fn export_selection_or_page(&mut self, cx: &mut Context<Self>) {
        let (rows, count) = if let Some(selection) = self.selection {
            if !selection.copyable() {
                self.status = selection.label();
                return;
            }
            (self.selected_rows(selection), selection.count())
        } else {
            let (first, last) = self.pager.range();
            if self.dataset_count() == 0 || first > last {
                self.status = "No rows to export".into();
                return;
            }
            let rows: Vec<Vec<Cell>> = (first..=last)
                .map(|idx| {
                    let source = self.source_row(idx);
                    let mut row = self.source.row(source);
                    for (col, cell) in row.iter_mut().enumerate() {
                        if let Some(edited) = self.edits.get(&(source, col)) {
                            *cell = edited.clone();
                        }
                    }
                    row
                })
                .collect();
            let count = rows.len() as u64;
            (rows, count)
        };
        let driver_id = self
            .live
            .as_ref()
            .map_or_else(|| "postgres".to_string(), |session| session.driver_id.clone());
        let columns = column_infos_for(self.source.columns());
        let context = CopyContext {
            columns: &columns,
            driver_id: &driver_id,
            target: (!self.grid_is_query).then(|| self.catalog.grid_table.clone()),
            column: self.cell_cursor.map_or(0, |cursor| cursor.column),
        };
        match result_copy::render(CopyFormat::Csv, &context, &rows) {
            Ok(copied) => {
                cx.write_to_clipboard(ClipboardItem::new_string(copied.text));
                let noun = if count == 1 { "row" } else { "rows" };
                self.status = format!("Exported {count} {noun} as CSV to clipboard");
            }
            Err(message) => self.status = message,
        }
    }
}
