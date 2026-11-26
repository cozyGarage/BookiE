use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bookie_core::export::{
    CsvOptions, ExportError, ResultExport, ResultFormat, SqlTarget, cap_pages, write_paged_file_no_replace,
};
use bookie_core::{ColumnInfo, Connection, DatabaseDriver, DriverError, FilterSet, OperationControl, TableInfo};
use bookie_policy::PolicyGuard;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::export_pages::{PageFuture, PagePlan, PageRunner};
use crate::snapshot_export::SnapshotPages;

const MAX_FILE_STEM_CHARS: usize = 120;
pub const MAX_TABLES_PER_EXPORT: usize = 256;

#[derive(Clone)]
pub struct TableTarget {
    pub schema: Option<String>,
    pub table: String,
    pub columns: Vec<ColumnInfo>,
}

#[derive(Debug, Clone)]
pub struct TablesExportJob {
    pub directory: PathBuf,
    pub max_rows_per_table: Option<usize>,
    pub timeout_secs: u32,
}

pub struct TableExportRequest<'a> {
    pub driver_id: &'a str,
    pub tables: &'a [TableTarget],
    pub directory: &'a Path,
    pub format: ResultFormat,
    pub extension: &'a str,
    pub csv: &'a CsvOptions,
    pub max_rows_per_table: Option<usize>,
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
    if request.tables.len() > MAX_TABLES_PER_EXPORT {
        return Err("choose at most 256 tables per export".into());
    }
    if request.max_rows_per_table == Some(0) {
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
        request.max_rows_per_table.unwrap_or(usize::MAX),
        capped.clone(),
    );
    let result = write_paged_file_no_replace(
        path,
        &target.columns,
        request.max_rows_per_table.unwrap_or(0),
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

fn driver_message(error: &DriverError) -> String {
    crate::driver_failure_message::driver_failure_message(&crate::driver_error_presentation::classify(error))
}

pub async fn prepare_targets(
    connection: &dyn Connection,
    tables: Vec<TableInfo>,
    control: &OperationControl,
) -> Result<Vec<TableTarget>, String> {
    let mut targets = Vec::with_capacity(tables.len());
    for table in tables {
        if control.cancellation_token().is_cancelled() {
            return Err(driver_message(&DriverError::Cancelled));
        }
        let columns = connection
            .fetch_columns_controlled(table.schema.as_deref(), &table.name, control)
            .await
            .map_err(|error| {
                let name = table
                    .schema
                    .as_ref()
                    .map_or_else(|| table.name.clone(), |schema| format!("{schema}.{}", table.name));
                format!("{name}: {}", driver_message(&error))
            })?;
        if control.cancellation_token().is_cancelled() {
            return Err(driver_message(&DriverError::Cancelled));
        }
        targets.push(TableTarget {
            schema: table.schema,
            table: table.name,
            columns,
        });
    }
    Ok(targets)
}

pub(crate) async fn export_tables_to_csv(
    guard: Arc<PolicyGuard>,
    driver: Arc<dyn DatabaseDriver>,
    targets: Vec<TableTarget>,
    job: TablesExportJob,
    cancel: CancellationToken,
) -> Result<Vec<TableResult>, String> {
    export_with_async_pages(move |run| {
        let csv = CsvOptions::default();
        let request = TableExportRequest {
            driver_id: driver.id(),
            tables: &targets,
            directory: &job.directory,
            format: ResultFormat::Csv,
            extension: "csv",
            csv: &csv,
            max_rows_per_table: job.max_rows_per_table,
            timeout_secs: job.timeout_secs,
        };
        export_tables(guard, driver.as_ref(), &request, &run, &cancel, |_, _| {})
    })
    .await
}

async fn export_with_async_pages(
    write: impl FnOnce(PageRunner) -> Result<Vec<TableResult>, String> + Send + 'static,
) -> Result<Vec<TableResult>, String> {
    let (send, mut receive) = mpsc::channel::<(PageFuture, oneshot::Sender<_>)>(1);
    let pages = tokio::spawn(async move {
        while let Some((fetch, reply)) = receive.recv().await {
            let result = fetch.await;
            let _ = reply.send(result);
        }
    });
    let written = tokio::task::spawn_blocking(move || {
        let run: PageRunner = Arc::new(move |fetch| {
            let (reply, result) = oneshot::channel();
            let stopped = || ExportError::Source("The export page task stopped unexpectedly.".into());
            send.blocking_send((fetch, reply)).map_err(|_| stopped())?;
            result.blocking_recv().map_err(|_| stopped())?
        });
        write(run)
    })
    .await
    .map_err(|_| "The export task stopped unexpectedly.".to_string())?;
    pages
        .await
        .map_err(|_| "The export page task stopped unexpectedly.".to_string())?;
    written
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
