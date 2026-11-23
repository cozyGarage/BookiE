use std::{ops::Range, path::PathBuf, sync::Arc};

use bookie_core::TableInfo;
use bookie_core::export::ExportError;
use bookie_core::export::{ResultExport, ResultFormat, SqlTarget, offered_formats, write_paged_file_no_replace};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::chrome::ghost;

const EXPORT_PAGE_ROWS: u64 = 128;

pub(crate) struct ExportFileState {
    pub open: bool,
    pub input: Option<Entity<TextInput>>,
    pub scope: ExportScope,
    pub format: ResultFormat,
    generation: u64,
    cancel: Option<CancellationToken>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ExportScope {
    Selected,
    #[default]
    CurrentPage,
    Loaded,
    Full,
}

impl Default for ExportFileState {
    fn default() -> Self {
        Self {
            open: false,
            input: None,
            scope: ExportScope::default(),
            format: ResultFormat::Csv,
            generation: 0,
            cancel: None,
        }
    }
}

impl GridView {
    pub(crate) fn export_file_enabled(&self) -> bool {
        !self.export.file.open && self.export.file.cancel.is_none() && self.dataset_count() > 0
    }

    pub(crate) fn close_export_file_bar(&mut self) {
        if let Some(cancel) = self.export.file.cancel.as_ref() {
            cancel.cancel();
            self.status = "Cancelling export…".into();
        }
        self.export.file.open = false;
    }

    pub(crate) fn open_export_file_bar(&mut self, cx: &mut Context<Self>) {
        if !self.export_file_enabled() {
            return;
        }
        let input = self.export.file.input.get_or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new("File name; selected format adds its extension", cx));
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submit) {
                    this.write_export_file(cx);
                }
            })
            .detach();
            input
        });
        input.update(cx, |input, cx| input.set_content("", cx));
        self.export.file.scope = if self.export_range(ExportScope::Selected).is_some() {
            ExportScope::Selected
        } else if self.export_range(ExportScope::CurrentPage).is_some() {
            ExportScope::CurrentPage
        } else {
            ExportScope::Loaded
        };
        self.export.file.format = ResultFormat::Csv;
        self.export.file.open = true;
        self.status = "Choose which rows and format to export".into();
        cx.notify();
    }

    fn export_scope_options(&self) -> Vec<(ExportScope, String)> {
        let mut options = Vec::new();
        if let Some(range) = self.export_range(ExportScope::Selected) {
            options.push((
                ExportScope::Selected,
                row_scope_label("Selected rows", range_count(&range), false, self.export.limit),
            ));
        }
        let page = self.export_range(ExportScope::CurrentPage);
        if let Some(range) = page.filter(|range| range.start < range.end) {
            options.push((
                ExportScope::CurrentPage,
                row_scope_label("Current page", range_count(&range), false, self.export.limit),
            ));
        }
        let loaded = self.dataset_count();
        let loaded_label = row_scope_label(
            "Filtered / loaded rows",
            loaded,
            self.source.is_partial(),
            self.export.limit,
        );
        options.push((ExportScope::Loaded, loaded_label));
        if full_result_available(
            self.grid_is_query,
            self.source.is_partial(),
            self.source.snapshot_consistent(),
        ) {
            options.push((
                ExportScope::Full,
                row_scope_label("Full result", self.source.total(), false, self.export.limit),
            ));
        }
        options
    }

    pub(crate) fn write_export_file(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self
            .export
            .file
            .input
            .clone()
            .filter(|_| self.export.file.open && self.export.file.cancel.is_none())
        else {
            return;
        };
        let typed = input.read(cx).content().trim().to_string();
        let format = self.export.file.format;
        let Some((path, target)) = self.export_destination(&typed, format) else {
            return;
        };
        let scope = self.export.file.scope;
        let Some(range) = self.export_range(scope) else {
            self.status = "That row scope is no longer available".into();
            return;
        };
        let full_count = range_count(&range);
        let count = self.export.limit.map_or(full_count, |limit| {
            full_count.min(u64::try_from(limit).unwrap_or(u64::MAX))
        });
        if count == 0 {
            self.status = "No rows to export".into();
            return;
        }
        let formats = self.formats_for(count, target.is_some());
        if !formats.contains(&format) {
            self.status = "That format is not available for this result and row count".into();
            return;
        }
        let Some(row_count) = usize::try_from(count).ok() else {
            self.status = "The selected row count is too large to export".into();
            return;
        };
        let driver_id = self
            .live
            .as_ref()
            .map_or_else(|| "postgres".to_string(), |session| session.driver_id.clone());
        let columns = column_infos_for(self.source.columns());
        let source = self.source.clone();
        let index_map = if scope == ExportScope::Full {
            None
        } else {
            self.local_order.clone().or_else(|| match &self.mode {
                Mode::All => None,
                Mode::Filtered { kept, .. } => Some(kept.clone()),
            })
        };
        let edits = self.cell_edit.edits.clone();
        let cancel = CancellationToken::new();
        self.export.file.generation = self.export.file.generation.wrapping_add(1);
        let generation = self.export.file.generation;
        self.export.file.cancel = Some(cancel.clone());
        let csv = self.csv_options();
        let cut = count < full_count;
        let end = range.start.saturating_add(count).min(range.end);
        self.status = if cut {
            format!("Writing the first {count} of {full_count} row(s)…")
        } else {
            format!("Writing {count} row(s)…")
        };
        let shown = path.display().to_string();
        cx.spawn(async move |this, cx| {
            let worker_cancel = cancel.clone();
            let written = cx
                .background_executor()
                .spawn(async move {
                    let sql = target.as_ref().map(|table| SqlTarget {
                        driver_id: &driver_id,
                        schema: table.schema.as_deref(),
                        table: &table.name,
                    });
                    let export = ResultExport { format, csv: &csv, sql };
                    let mut next = range.start;
                    write_paged_file_no_replace(
                        &path,
                        &columns,
                        row_count,
                        &export,
                        || next_export_page(&source, &index_map, &edits, &worker_cancel, &mut next, end),
                        || worker_cancel.is_cancelled(),
                        |_| {},
                    )
                    .map_err(|error| error.to_string())
                })
                .await;
            this.update(cx, |this, cx| {
                if this.export.file.generation != generation {
                    return;
                }
                this.export.file.cancel = None;
                match written {
                    Ok(()) => {
                        this.export.file.open = false;
                        this.status = if cut {
                            format!("Wrote the first {count} of {full_count} row(s) to {shown}")
                        } else {
                            format!("Wrote {count} row(s) to {shown}")
                        };
                    }
                    Err(_) if cancel.is_cancelled() => this.status = "Export cancelled; no file was created".into(),
                    Err(message) => this.status = format!("Could not export: {message}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn export_range(&self, scope: ExportScope) -> Option<Range<u64>> {
        let selected = self.selection;
        let full_allowed = full_result_available(
            self.grid_is_query,
            self.source.is_partial(),
            self.source.snapshot_consistent(),
        );
        scope_range(
            scope,
            self.dataset_count(),
            self.source.total(),
            self.pager.page,
            self.pager.rows,
            selected,
            !self.continuous,
            full_allowed,
        )
    }

    fn export_destination(&mut self, typed: &str, format: ResultFormat) -> Option<(PathBuf, Option<TableInfo>)> {
        if typed.is_empty() {
            self.status = "Choose a file name".into();
            return None;
        }
        let mut path = bookie_core::user_path::expand_tilde(std::path::Path::new(typed));
        match path.extension().and_then(|extension| extension.to_str()) {
            None => {
                path.set_extension(format.extension());
            }
            Some(extension) if ResultFormat::from_extension(extension) == Some(format) => {}
            Some(_) => {
                self.status = format!("Choose a .{} file name for {}", format.extension(), format.label());
                return None;
            }
        }
        if path.exists() {
            self.status = "That file already exists; choose a new file name".into();
            return None;
        }
        Some((path, (!self.grid_is_query).then(|| self.catalog.grid_table.clone())))
    }

    fn formats_for(&self, rows: u64, has_table: bool) -> Vec<ResultFormat> {
        let driver_id = self
            .live
            .as_ref()
            .map_or("postgres", |session| session.driver_id.as_str());
        export_formats(driver_id, rows, has_table)
    }

    pub(crate) fn export_file_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.export.file.input.clone()?;
        if !self.export.file.open {
            return None;
        }
        use gpui_component::button::{Button, ButtonVariants};
        use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};

        let scope_owner = cx.entity().downgrade();
        let format_owner = cx.entity().downgrade();
        let scope_options = self.export_scope_options();
        let scope_label = scope_options
            .iter()
            .find(|(scope, _)| *scope == self.export.file.scope)
            .map_or("Choose rows", |(_, label)| label.as_str())
            .to_string();
        let has_table = !self.grid_is_query;
        let formats = self.formats_for(self.export_count(self.export.file.scope), has_table);
        let format_label = self.export.file.format.label();
        let source_label = if self.grid_is_query {
            "query result".to_string()
        } else {
            format!("table {}", table_label(&self.catalog.grid_table))
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .p_2()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(format!("Export {source_label}")),
                )
                .child(
                    Button::new("export-scope")
                        .ghost()
                        .compact()
                        .accessibility_label("Export row scope")
                        .child(scope_label)
                        .dropdown_menu(move |mut menu, _, _| {
                            for (scope, label) in scope_options.iter().cloned() {
                                let owner = scope_owner.clone();
                                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        if this.export.file.cancel.is_none() {
                                            this.set_export_scope(scope);
                                            cx.notify();
                                        }
                                    });
                                }));
                            }
                            menu
                        }),
                )
                .child(
                    Button::new("export-format")
                        .ghost()
                        .compact()
                        .accessibility_label("Export file format")
                        .child(format_label)
                        .dropdown_menu(move |mut menu, _, _| {
                            for format in formats.iter().copied() {
                                let owner = format_owner.clone();
                                menu = menu.item(PopupMenuItem::new(format.label()).on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        if this.export.file.cancel.is_none() {
                                            this.export.file.format = format;
                                            cx.notify();
                                        }
                                    });
                                }));
                            }
                            menu
                        }),
                )
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    ghost(
                        "write-export-file",
                        if self.export.file.cancel.is_some() {
                            "Writing…"
                        } else {
                            "Export"
                        },
                        theme,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.write_export_file(cx))),
                )
                .child(
                    ghost("cancel-export-file", "Cancel", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.close_export_file_bar();
                        cx.notify();
                    })),
                ),
        )
    }

    fn export_count(&self, scope: ExportScope) -> u64 {
        let count = self.export_range(scope).map_or(0, |range| range_count(&range));
        self.export
            .limit
            .map_or(count, |limit| count.min(u64::try_from(limit).unwrap_or(u64::MAX)))
    }

    fn set_export_scope(&mut self, scope: ExportScope) {
        self.export.file.scope = scope;
        let formats = self.formats_for(self.export_count(scope), !self.grid_is_query);
        if !formats.contains(&self.export.file.format) {
            self.export.file.format = formats.first().copied().unwrap_or(ResultFormat::Csv);
        }
    }
}

fn full_result_available(is_query: bool, partial: bool, snapshot_consistent: bool) -> bool {
    !partial && (is_query || snapshot_consistent)
}

fn range_count(range: &Range<u64>) -> u64 {
    range.end.saturating_sub(range.start)
}

fn row_scope_label(label: &str, count: u64, partial: bool, limit: Option<usize>) -> String {
    let capped = limit
        .map(|limit| count.min(u64::try_from(limit).unwrap_or(u64::MAX)))
        .unwrap_or(count);
    let count_label = if capped < count {
        format!("first {capped} of {count}")
    } else {
        count.to_string()
    };
    format!(
        "{label} ({count_label} rows{})",
        if partial { "; more may exist" } else { "" }
    )
}

fn export_formats(driver_id: &str, rows: u64, has_table: bool) -> Vec<ResultFormat> {
    offered_formats(
        driver_id,
        usize::try_from(rows).is_ok_and(|rows| rows <= bookie_core::export::MAX_WORKBOOK_ROWS),
    )
    .into_iter()
    .filter(|format| has_table || *format != ResultFormat::Sql)
    .collect()
}

fn scope_range(
    scope: ExportScope,
    dataset_count: u64,
    source_count: u64,
    page: u64,
    page_rows: u64,
    selection: Option<crate::Selection>,
    page_available: bool,
    full_allowed: bool,
) -> Option<Range<u64>> {
    match scope {
        ExportScope::Selected => selection.and_then(|selection| {
            (selection.last() < dataset_count).then(|| selection.first()..selection.last().saturating_add(1))
        }),
        ExportScope::CurrentPage if page_available && page_rows > 0 => {
            let start = page.saturating_mul(page_rows).min(dataset_count);
            (start < dataset_count).then(|| start..start.saturating_add(page_rows).min(dataset_count))
        }
        ExportScope::Loaded => Some(0..dataset_count),
        ExportScope::Full if full_allowed => Some(0..source_count),
        ExportScope::CurrentPage | ExportScope::Full => None,
    }
}

fn mapped_source_row(index: u64, map: &Option<Arc<Vec<u32>>>) -> Option<u64> {
    match map {
        Some(map) => usize::try_from(index)
            .ok()
            .and_then(|index| map.get(index))
            .map(|source| u64::from(*source)),
        None => Some(index),
    }
}

fn next_export_page(
    source: &bookie_gpui::source::Source,
    map: &Option<Arc<Vec<u32>>>,
    edits: &crate::grid_edit::Edits,
    cancellation: &CancellationToken,
    next: &mut u64,
    end: u64,
) -> Result<Option<bookie_core::export::RowPage>, ExportError> {
    if *next >= end {
        return Ok(None);
    }
    let page_end = next.saturating_add(EXPORT_PAGE_ROWS).min(end);
    let rows = (*next..page_end)
        .map(|index| {
            if cancellation.is_cancelled() {
                return Err(ExportError::Cancelled);
            }
            let source_row = mapped_source_row(index, map)
                .ok_or_else(|| ExportError::Source("the filtered row range changed during export".into()))?;
            let mut row = source.row(source_row);
            for (column, value) in row.iter_mut().enumerate() {
                if let Some(edited) = edits.get(&(source_row, column)) {
                    *value = edited.clone();
                }
            }
            Ok(row)
        })
        .collect::<Result<_, ExportError>>()?;
    *next = page_end;
    Ok(Some(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_page_and_selection_have_distinct_ranges() {
        let selection = crate::Selection { anchor: 3, focus: 5 };
        assert_eq!(
            scope_range(ExportScope::CurrentPage, 30, 30, 2, 10, Some(selection), true, true),
            Some(20..30)
        );
        assert_eq!(
            scope_range(ExportScope::Selected, 30, 30, 2, 10, Some(selection), true, true),
            Some(3..6)
        );
        assert_eq!(
            scope_range(
                ExportScope::Selected,
                5,
                30,
                2,
                10,
                Some(crate::Selection { anchor: 4, focus: 6 }),
                true,
                true,
            ),
            None,
            "stale selections are not offered as export scopes"
        );
        assert_eq!(
            scope_range(ExportScope::CurrentPage, 0, 0, 0, 10, None, true, true),
            None
        );
        assert_eq!(
            scope_range(ExportScope::CurrentPage, 30, 30, 2, 10, None, false, true),
            None
        );
    }

    #[test]
    fn filtered_loaded_rows_use_dataset_indices_and_report_capped_results_as_partial() {
        let mapping = Some(Arc::new(vec![4, 8, 12]));
        let rows = (0..3)
            .map(|index| mapped_source_row(index, &mapping))
            .collect::<Vec<_>>();
        assert_eq!(rows, vec![Some(4), Some(8), Some(12)]);
        assert_eq!(mapped_source_row(3, &mapping), None);
        assert_eq!(
            scope_range(ExportScope::Loaded, 3, 20, 0, 10, None, true, false),
            Some(0..3)
        );
        assert_eq!(
            row_scope_label("Filtered / loaded rows", 3, true, Some(2)),
            "Filtered / loaded rows (first 2 of 3 rows; more may exist)"
        );
        assert!(!full_result_available(true, true, true));
    }

    #[test]
    fn full_table_export_requires_a_complete_snapshot() {
        assert!(full_result_available(true, false, false));
        assert!(!full_result_available(false, false, false));
        assert!(full_result_available(false, false, true));
        assert!(!full_result_available(false, true, true));
    }

    #[test]
    fn formats_follow_source_and_workbook_limits() {
        let query = export_formats("postgres", 3, false);
        assert!(!query.contains(&ResultFormat::Sql));
        assert!(query.contains(&ResultFormat::Xlsx));
        let table = export_formats("postgres", 3, true);
        assert!(table.contains(&ResultFormat::Sql));
        assert!(
            !export_formats("postgres", bookie_core::export::MAX_WORKBOOK_ROWS as u64 + 1, true)
                .contains(&ResultFormat::Xlsx)
        );
    }

    #[test]
    fn streaming_export_preserves_typed_json_and_cancellation_publishes_nothing() {
        use bookie_core::export::{CsvOptions, ResultExport, write_paged_file_no_replace};
        use bookie_gpui::source::{ResultSource, Source, Synthetic};

        let source: Source = Arc::new(Synthetic::new(12));
        let columns = bookie_gpui::core_bridge::column_infos_for(source.columns());
        let directory = tempfile::tempdir().expect("temporary export directory");
        let path = directory.path().join("typed.jsonl");
        let csv = CsvOptions::default();
        let export = ResultExport {
            format: ResultFormat::JsonLines,
            csv: &csv,
            sql: None,
        };
        let mut next = 0;
        write_paged_file_no_replace(
            &path,
            &columns,
            3,
            &export,
            || {
                next_export_page(
                    &source,
                    &None,
                    &Default::default(),
                    &CancellationToken::new(),
                    &mut next,
                    3,
                )
            },
            || false,
            |_| {},
        )
        .expect("write typed JSON lines");
        let first = std::fs::read_to_string(&path)
            .expect("read exported values")
            .lines()
            .next()
            .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .expect("parse exported JSON line");
        assert!(first["value"].is_number());
        assert!(first["typed"].is_number());

        let cancelled_path = directory.path().join("cancelled.jsonl");
        let cancellation = CancellationToken::new();
        let cancel_after_first_row = cancellation.clone();
        let mut next = 0;
        let result = write_paged_file_no_replace(
            &cancelled_path,
            &columns,
            3,
            &export,
            || next_export_page(&source, &None, &Default::default(), &cancellation, &mut next, 3),
            || cancellation.is_cancelled(),
            |rows| {
                if rows == 1 {
                    cancel_after_first_row.cancel();
                }
            },
        );
        assert!(result.is_err());
        assert!(!cancelled_path.exists());
    }
}
