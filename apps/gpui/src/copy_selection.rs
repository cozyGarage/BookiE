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
        match result_copy::render_with(format, &context, &rows, &self.csv_options()) {
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

    fn csv_options(&self) -> bookie_core::export::CsvOptions {
        bookie_core::export::CsvOptions {
            custom_delimiter: self.export.csv_delimiter.clone(),
            ..Default::default()
        }
    }

    pub(crate) fn set_csv_delimiter(&mut self) {
        match self.palette.delimiter.clone() {
            Some((_, Ok(delimiter))) => self.set_csv_delimiter_to(Some(delimiter)),
            Some((_, Err(error))) => {
                self.status = format!("CSV delimiter refused: {}", bookie_gpui::csv_delimiter::error_label(error));
            }
            None => {}
        }
    }

    pub(crate) fn set_csv_delimiter_to(&mut self, delimiter: Option<bookie_core::export::CustomDelimiter>) {
        self.status = match &delimiter {
            Some(delimiter) => format!("CSV delimiter set to \"{}\"", delimiter.as_str()),
            None => "CSV delimiter reset to the comma".into(),
        };
        self.export.csv_delimiter = delimiter;
    }

    pub(crate) fn set_export_limit(&mut self) {
        match self.palette.limit.clone() {
            Some((_, Ok(limit))) => self.set_export_limit_to(Some(limit)),
            Some((_, Err(()))) => {
                self.status = format!("Export limit refused: {}", bookie_gpui::export_limit::error_label());
            }
            None => {}
        }
    }

    pub(crate) fn set_export_limit_to(&mut self, limit: Option<usize>) {
        self.status = match limit {
            Some(limit) => format!("Export limit set to {limit} rows"),
            None => "Export limit removed".into(),
        };
        self.export.limit = limit;
    }

    pub(crate) fn export_selection_or_page(&mut self, cx: &mut Context<Self>) {
        let (mut rows, count) = if let Some(selection) = self.selection {
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
            let rows: Vec<Vec<Cell>> = (first.saturating_sub(1)..last)
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
        let cut = self.export.limit.is_some_and(|limit| rows.len() > limit);
        if let Some(limit) = self.export.limit {
            rows.truncate(limit);
        }
        let count = if cut { rows.len() as u64 } else { count };
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
        match result_copy::render_with(CopyFormat::Csv, &context, &rows, &self.csv_options()) {
            Ok(copied) => {
                cx.write_to_clipboard(ClipboardItem::new_string(copied.text));
                let noun = if count == 1 { "row" } else { "rows" };
                self.status = if cut {
                    format!("Exported the first {count} {noun} as CSV to clipboard (export limit reached, more rows were not exported)")
                } else {
                    format!("Exported {count} {noun} as CSV to clipboard")
                };
            }
            Err(message) => self.status = message,
        }
    }
}
