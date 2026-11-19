use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bookie_core::export::{
    CsvOptions, ExportError, ResultExport, ResultFormat, SqlTarget, cap_pages, write_paged_file,
};
use bookie_core::{ColumnInfo, Connection, FilterSet};
use tokio_util::sync::CancellationToken;

use crate::export_pages::{PagePlan, PageRunner, page_fetcher};

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
    Ok(())
}

fn export_one(
    conn: &Arc<dyn Connection>,
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
    let plan = PagePlan {
        driver_id: request.driver_id.to_string(),
        schema: target.schema.clone(),
        table: target.table.clone(),
        columns: target.columns.clone(),
        filter: FilterSet::default(),
        sort: None,
        row_id_order: false,
        timeout_secs: request.timeout_secs,
    };
    let capped = Arc::new(AtomicBool::new(false));
    let fetch = page_fetcher(conn.clone(), plan, run.clone(), cancel.clone());
    let next = cap_pages(fetch, request.max_rows_per_table, capped.clone());
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
    write_paged_file(
        path,
        &target.columns,
        request.max_rows_per_table,
        &export,
        next,
        || cancel.is_cancelled(),
        |written| {
            rows.set(written);
            progress(written);
        },
    )?;
    Ok((rows.get(), capped.load(Ordering::SeqCst)))
}

pub fn export_tables(
    conn: Arc<dyn Connection>,
    request: &TableExportRequest<'_>,
    run: &PageRunner,
    cancel: &CancellationToken,
    progress: impl Fn(usize, usize),
) -> Result<Vec<TableResult>, String> {
    check_request(request)?;
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
        let outcome = match export_one(&conn, request, target, &path, run, cancel, &|rows| {
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
