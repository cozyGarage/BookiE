use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bookie_core::export::{
    CsvOptions, ExportError, ResultExport, ResultFormat, SqlTarget, cap_pages, write_paged_file_no_replace,
};
use bookie_core::{ColumnInfo, DatabaseDriver, FilterSet};
use bookie_policy::PolicyGuard;
use tokio_util::sync::CancellationToken;

use crate::export_pages::{PagePlan, PageRunner};
use crate::snapshot_export::SnapshotPages;

const MAX_FILE_STEM_CHARS: usize = 120;

#[derive(Clone)]
pub struct TableTarget {
    pub schema: Option<String>,
    pub table: String,
    pub columns: Vec<ColumnInfo>,
}

pub struct TableExportRequest<'a> {
    pub driver_id: &'a str,
    pub tables: &'a [TableTarget],
    pub directory: &'a Path,
    pub format: ResultFormat,
    pub extension: &'a str,
    pub csv: &'a CsvOptions,
    pub max_rows_per_table: usize,
    pub timeout_secs: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TableOutcome {
    Written { rows: usize, capped: bool },
    Failed(String),
    NotStarted,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TableResult {
    pub table: String,
    pub path: Option<PathBuf>,
    pub outcome: TableOutcome,
}

fn label(target: &TableTarget) -> String {
    match &target.schema {
        Some(schema) => format!("{schema}.{}", target.table),
        None => target.table.clone(),
    }
}

fn file_stem(label: &str, used: &mut HashSet<String>) -> String {
    let mut stem: String = label
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '_' | '-' | '.') {
                character
            } else {
                '_'
            }
        })
        .take(MAX_FILE_STEM_CHARS)
        .collect();
    if stem.starts_with('.') || stem.is_empty() {
        stem.insert(0, '_');
    }
    let mut candidate = stem.clone();
    let mut number = 2;
    while !used.insert(candidate.to_lowercase()) {
        candidate = format!("{stem}-{number}");
        number += 1;
    }
    candidate
}

fn check_request(request: &TableExportRequest<'_>) -> Result<(), String> {
    if request.tables.is_empty() {
        return Err("choose at least one table".into());
    }
    if request.max_rows_per_table == 0 {
        return Err("a row limit of at least 1 is required for a multi-table export".into());
    }
    if !request.directory.is_dir() {
        return Err("the export folder does not exist".into());
    }
    if request.extension.is_empty() || !request.extension.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err("the export extension is invalid".into());
    }
    Ok(())
}

fn page_plan(request: &TableExportRequest<'_>, target: &TableTarget) -> PagePlan {
    PagePlan {
        driver_id: request.driver_id.to_string(),
        schema: target.schema.clone(),
        table: target.table.clone(),
        columns: target.columns.clone(),
        filter: FilterSet::default(),
        sort: None,
        row_id_order: false,
        timeout_secs: request.timeout_secs,
    }
}

fn export_one(
    guard: &Arc<PolicyGuard>,
    request: &TableExportRequest<'_>,
    target: &TableTarget,
    path: &Path,
    run: &PageRunner,
    cancel: &CancellationToken,
    progress: &dyn Fn(usize),
) -> Result<(usize, bool), ExportError> {
    if path.exists() {
        return Err(ExportError::Source(format!(
            "{} already exists and was not replaced",
            path.display()
        )));
    }
    let plan = page_plan(request, target);
    let capped = Arc::new(AtomicBool::new(false));
    let pages = std::cell::RefCell::new(Some(SnapshotPages::new(
        guard.clone(),
        plan,
        run.clone(),
        cancel.clone(),
    )));
    let export = ResultExport {
        format: request.format,
        csv: request.csv,
        sql: Some(SqlTarget {
            driver_id: request.driver_id,
            schema: target.schema.as_deref(),
            table: &target.table,
        }),
    };
    let rows = Cell::new(0usize);
    let mut next = cap_pages(
        || {
            pages
                .borrow_mut()
                .as_mut()
                .ok_or_else(|| ExportError::Source("snapshot is closed".into()))?
                .next_page()
        },
        request.max_rows_per_table,
        capped.clone(),
    );
    let result = write_paged_file_no_replace(
        path,
        &target.columns,
        request.max_rows_per_table,
        &export,
        || match next()? {
            Some(page) => Ok(Some(page)),
            None => {
                if let Some(pages) = pages.borrow_mut().take() {
                    pages.finish()?;
                }
                Ok(None)
            }
        },
        || cancel.is_cancelled(),
        |written| {
            rows.set(written);
            progress(written);
        },
    );
    drop(next);
    let finish = pages.into_inner().map_or(Ok(()), SnapshotPages::finish);
    result?;
    finish?;
    Ok((rows.get(), capped.load(Ordering::SeqCst)))
}

pub fn export_tables(
    guard: Arc<PolicyGuard>,
    driver: &dyn DatabaseDriver,
    request: &TableExportRequest<'_>,
    run: &PageRunner,
    cancel: &CancellationToken,
    progress: impl Fn(usize, usize),
) -> Result<Vec<TableResult>, String> {
    check_request(request)?;
    if driver.id() != request.driver_id
        || guard.context().driver_id != request.driver_id
        || !driver.capabilities().supports_snapshot_export
    {
        return Err("multi-table export requires a supported snapshot for the active driver".into());
    }
    let mut used = HashSet::new();
    let mut results = Vec::with_capacity(request.tables.len());
    for (index, target) in request.tables.iter().enumerate() {
        let table = label(target);
        let stem = file_stem(&table, &mut used);
        let path = request.directory.join(format!("{stem}.{}", request.extension));
        if cancel.is_cancelled() {
            results.push(TableResult {
                table,
                path: None,
                outcome: TableOutcome::NotStarted,
            });
            continue;
        }
        let outcome = match export_one(&guard, request, target, &path, run, cancel, &|rows| {
            progress(index, rows)
        }) {
            Ok((rows, capped)) => TableOutcome::Written { rows, capped },
            Err(error) => TableOutcome::Failed(error.to_string()),
        };
        let written = matches!(outcome, TableOutcome::Written { .. });
        results.push(TableResult {
            table,
            path: written.then_some(path),
            outcome,
        });
    }
    Ok(results)
}

#[cfg(test)]
#[path = "export_tables_tests.rs"]
mod tests;
