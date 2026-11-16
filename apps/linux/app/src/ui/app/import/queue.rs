use std::path::Path;
use std::sync::Arc;

use bookie_app_services::import_queue::{FileImporter, FileOutcome, QueueReport, RunBudget, run_queue};
use bookie_core::import::{CsvImportOptions, read_csv};
use bookie_policy::PolicyGuard;
use relm4::ComponentSender;
use tokio_util::sync::CancellationToken;

use super::{ImportRun, plan_for, read_file};
use crate::ui::app::{App, AppMsg};
use crate::ui::import_dialog::ImportChoice;

#[derive(Debug)]
pub struct ImportQueueFileStarted {
    pub index: usize,
    pub count: usize,
    pub name: String,
    pub total: u64,
}

#[derive(Debug)]
pub struct ImportQueueFinished {
    pub schema: Option<String>,
    pub table: String,
    pub updates_existing_rows: bool,
    pub report: QueueReport,
}

struct QueueImporter {
    guard: Arc<PolicyGuard>,
    template: ImportChoice,
    first_bytes: Option<Arc<[u8]>>,
    shape: Option<Vec<String>>,
    token: CancellationToken,
    timeout_secs: u32,
    sender: ComponentSender<App>,
}

impl QueueImporter {
    async fn bytes_for(&mut self, index: usize, path: &Path) -> Result<Arc<[u8]>, String> {
        if let Some(bytes) = self.first_bytes.take().filter(|_| index == 0) {
            return Ok(bytes);
        }
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || read_file(&path).map(Arc::from))
            .await
            .map_err(|_| bookie_app_services::tr!("The import file could not be prepared."))?
    }

    async fn plan_for_file(
        &mut self,
        bytes: Arc<[u8]>,
    ) -> Result<(bookie_core::import::InsertPlan, Vec<String>), String> {
        let driver_id = self.guard.context().driver_id.clone();
        let choice = ImportChoice {
            bytes,
            ..self.template.clone()
        };
        let expected = self.shape.clone();
        tokio::task::spawn_blocking(move || {
            let shape = file_shape(&choice.bytes, &choice.options)?;
            if expected.as_ref().is_some_and(|expected| *expected != shape) {
                return Err(bookie_app_services::tr!(
                    "The columns differ from the first file, so the same choices cannot be applied."
                ));
            }
            plan_for(&driver_id, &choice).map(|plan| (plan, shape))
        })
        .await
        .map_err(|_| bookie_app_services::tr!("The import plan could not be prepared."))?
    }

    async fn import_file(&mut self, index: usize, count: usize, path: &Path, budget: RunBudget) -> FileOutcome {
        let failed = |message: String| FileOutcome::Failed { rows: 0, message };
        let bytes = match self.bytes_for(index, path).await {
            Ok(bytes) => bytes,
            Err(message) => return failed(message),
        };
        let (plan, shape) = match self.plan_for_file(bytes).await {
            Ok(planned) => planned,
            Err(message) => return failed(message),
        };
        self.shape = Some(shape);
        let total = plan.row_count();
        if total > budget.remaining_rows {
            return failed(
                bookie_app_services::tr!("The run would pass its limit of {limit} rows.")
                    .replace("{limit}", &bookie_app_services::import_queue::MAX_RUN_ROWS.to_string()),
            );
        }
        self.sender
            .input(AppMsg::ImportQueueFileStarted(Box::new(ImportQueueFileStarted {
                index,
                count,
                name: file_label(path),
                total,
            })));
        let run = ImportRun {
            guard: self.guard.clone(),
            plan,
            create_sql: None,
            schema: self.template.schema.clone(),
            table: self.template.table.clone(),
            token: self.token.clone(),
            timeout_secs: self.timeout_secs,
            skip_rejected_rows: self.template.skip_rejected_rows,
            updates_existing_rows: self.template.write.updates_existing_rows(),
        };
        let report = run.execute(&self.sender).await;
        outcome_of(report, &self.token)
    }
}

impl FileImporter for QueueImporter {
    fn import(
        &mut self,
        index: usize,
        count: usize,
        path: &Path,
        budget: RunBudget,
    ) -> impl std::future::Future<Output = FileOutcome> + Send {
        self.import_file(index, count, path, budget)
    }
}

fn outcome_of(report: super::CsvImportReport, token: &CancellationToken) -> FileOutcome {
    let rows = report.committed;
    match report.error {
        None => FileOutcome::Imported {
            rows,
            skipped: report.skipped,
        },
        Some(_) if report.cancelled || token.is_cancelled() => FileOutcome::Cancelled { rows },
        Some(message) => FileOutcome::Failed { rows, message },
    }
}

fn file_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn file_shape(bytes: &[u8], options: &CsvImportOptions) -> Result<Vec<String>, String> {
    let sheet = read_csv(bytes, options, Some(1)).map_err(|error| error.to_string())?;
    if options.has_header {
        return Ok(sheet.headers);
    }
    let fields = sheet.rows.first().map_or(0, Vec::len);
    Ok(vec![String::new(); fields])
}

impl App {
    pub(in crate::ui::app) fn start_import_queue(
        &mut self,
        choice: ImportChoice,
        connection_id: uuid::Uuid,
        sender: ComponentSender<Self>,
    ) {
        let Some(guard) = self
            .database
            .guard(connection_id, bookie_policy::Principal::human_gui())
        else {
            self.close_csv_import_progress();
            self.show_error_alert(
                &bookie_app_services::tr!("Can't import"),
                &bookie_app_services::tr!("No active connection."),
            );
            return;
        };
        let Some(token) = self
            .csv_import_progress
            .as_ref()
            .map(crate::ui::import_dialog::ImportProgress::cancel_token)
        else {
            return;
        };
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let mut paths = vec![choice.source.clone()];
        paths.extend(choice.more_files.iter().cloned());
        let finished = (
            choice.schema.clone(),
            choice.table.clone(),
            choice.write.updates_existing_rows(),
        );
        let first_bytes = Some(choice.bytes.clone());
        let template = ImportChoice {
            bytes: Arc::from(Vec::new()),
            more_files: Vec::new(),
            ..choice
        };
        let for_progress = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let mut importer = QueueImporter {
                        guard,
                        template,
                        first_bytes,
                        shape: None,
                        token: token.clone(),
                        timeout_secs,
                        sender: for_progress.clone(),
                    };
                    let report = run_queue(paths, &token, &mut importer).await;
                    for_progress.input(AppMsg::ImportQueueFinished(Box::new(ImportQueueFinished {
                        schema: finished.0,
                        table: finished.1,
                        updates_existing_rows: finished.2,
                        report,
                    })));
                })
                .drop_on_shutdown()
        });
    }

    pub(in crate::ui::app) fn on_import_queue_file_started(&mut self, started: ImportQueueFileStarted) {
        let Some(progress) = self.csv_import_progress.as_mut() else {
            return;
        };
        let note = bookie_app_services::tr!("File {index} of {count}: {name}")
            .replace("{index}", &(started.index + 1).to_string())
            .replace("{count}", &started.count.to_string())
            .replace("{name}", &started.name);
        progress.begin_file(&note, started.total);
    }

    pub(in crate::ui::app) fn on_import_queue_finished(&mut self, finished: ImportQueueFinished) {
        self.close_csv_import_progress();
        self.refresh_import_targets(finished.schema.as_deref(), &finished.table);
        let report = finished.report;
        if report.succeeded() {
            self.show_toast(&queue_toast(&report, &finished.table, finished.updates_existing_rows));
            return;
        }
        let title = if report.was_cancelled() {
            bookie_app_services::tr!("Import cancelled")
        } else {
            bookie_app_services::tr!("Import stopped")
        };
        self.show_error_alert(&title, &report.summary());
    }
}

fn queue_toast(report: &QueueReport, table: &str, updates_existing_rows: bool) -> String {
    let template = if updates_existing_rows {
        bookie_app_services::tr!("Imported or updated {rows} rows from {files} files in {table}")
    } else {
        bookie_app_services::tr!("Imported {rows} rows from {files} files into {table}")
    };
    template
        .replace("{rows}", &report.rows_written().to_string())
        .replace("{files}", &report.files.len().to_string())
        .replace("{table}", table)
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
