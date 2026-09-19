use std::path::PathBuf;
use std::sync::Arc;

use relm4::ComponentSender;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::gtk::gio;
use tablepro_core::import::{
    CsvFormat, DEFAULT_IMPORT_BATCH_ROWS, ImportTarget, InsertPlan, MAX_FILE_BYTES, PlanError, build_insert_plan,
    detect_format, read_csv,
};
use tablepro_core::{ColumnInfo, DriverError, OperationControl};
use tablepro_policy::{BulkBatch, BulkInsertEnd, BulkInsertRequest, PolicyGuard};
use tokio_util::sync::CancellationToken;

use crate::ui::error_text;
use crate::ui::import_dialog::{ImportChoice, ImportRequest};

use super::{App, AppMsg};

/// Everything the file dialog collected, handed back to the GTK thread.
#[derive(Debug)]
pub struct CsvImportPreparation {
    pub schema: Option<String>,
    pub table: String,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub format: CsvFormat,
    pub columns: Vec<ColumnInfo>,
}

/// How an import ended, as the GTK thread hears it.
#[derive(Debug)]
pub struct CsvImportReport {
    pub table: String,
    pub committed: u64,
    pub total: u64,
    pub error: Option<String>,
}

impl App {
    pub(super) fn on_import_csv_into_table(
        &self,
        schema: Option<String>,
        table: String,
        sender: ComponentSender<Self>,
    ) {
        if let Some(message) = self.import_refusal() {
            self.show_error_alert(&crate::tr!("Can't import"), &message);
            return;
        }
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&crate::tr!("CSV files")));
        filter.add_mime_type("text/csv");
        filter.add_suffix("csv");
        filter.add_suffix("tsv");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(crate::tr!("Import CSV"))
            .modal(true)
            .default_filter(&filter)
            .filters(&filters)
            .build();
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |outcome| {
            let Some(path) = outcome.ok().and_then(|file| file.path()) else {
                return;
            };
            sender.input(AppMsg::CsvFileChosen {
                schema: schema.clone(),
                table: table.clone(),
                path,
            });
        });
    }

    /// Why an import cannot start at all. Checked before the user picks a
    /// file, so a refusal never arrives after the work looks under way.
    fn import_refusal(&self) -> Option<String> {
        if self.connection_id.is_none() || self.current_driver_id.is_none() {
            return Some(crate::tr!("No active connection."));
        }
        if self.read_only {
            return Some(crate::tr!("This connection is open read-only."));
        }
        if !self.database.policy_available() {
            return Some(crate::tr!(
                "The policy rules could not be loaded, so writes are not permitted."
            ));
        }
        if self.database.governed_writes_disabled() {
            return Some(crate::tr!(
                "A previous operation's audit record could not be written, so writes are not permitted."
            ));
        }
        None
    }

    pub(super) fn on_csv_file_chosen(
        &self,
        schema: Option<String>,
        table: String,
        path: PathBuf,
        sender: ComponentSender<Self>,
    ) {
        let Some(connection_id) = self.connection_id else {
            return;
        };
        let Some(conn) = self.database.get(connection_id) else {
            return;
        };
        let timeout_secs = crate::services::operation_control::configured_timeout_secs(&self.preferences);
        sender.clone().command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    match prepare_import(conn, &control, schema, table, path).await {
                        Ok(preparation) => sender.input(AppMsg::CsvImportPrepared(Box::new(preparation))),
                        Err(message) => sender.input(AppMsg::ShowAlert {
                            title: crate::tr!("Can't import"),
                            body: message,
                        }),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_csv_import_prepared(&self, preparation: CsvImportPreparation, sender: ComponentSender<Self>) {
        let request = ImportRequest {
            schema: preparation.schema,
            table: preparation.table,
            path: preparation.path,
            bytes: preparation.bytes,
            format: preparation.format,
            columns: preparation.columns,
        };
        crate::ui::import_dialog::present(&self.window, request, move |choice| {
            sender.input(AppMsg::StartCsvImport(Box::new(choice)));
        });
    }

    pub(super) fn on_start_csv_import(&mut self, choice: ImportChoice, sender: ComponentSender<Self>) {
        if let Some(message) = self.import_refusal() {
            self.show_error_alert(&crate::tr!("Can't import"), &message);
            return;
        }
        let (Some(connection_id), Some(driver_id)) = (self.connection_id, self.current_driver_id.clone()) else {
            return;
        };
        let plan = match plan_for(&driver_id, &choice) {
            Ok(plan) => plan,
            Err(message) => {
                self.show_error_alert(&crate::tr!("Can't import"), &message);
                return;
            }
        };
        let Some(guard) = self
            .database
            .guard(connection_id, tablepro_policy::Principal::human_gui())
        else {
            self.show_error_alert(&crate::tr!("Can't import"), &crate::tr!("No active connection."));
            return;
        };
        let progress = crate::ui::import_dialog::present_progress(&self.window, &choice.table, plan.row_count());
        let token = progress.cancel_token();
        self.csv_import_progress = Some(progress);
        self.run_import(guard, plan, choice, token, sender);
    }

    fn run_import(
        &self,
        guard: Arc<PolicyGuard>,
        plan: InsertPlan,
        choice: ImportChoice,
        token: CancellationToken,
        sender: ComponentSender<Self>,
    ) {
        let timeout_secs = crate::services::operation_control::configured_timeout_secs(&self.preferences);
        let sender_for_cmd = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let run = ImportRun {
                        guard,
                        plan,
                        schema: choice.schema,
                        table: choice.table,
                        token,
                        timeout_secs,
                    };
                    let report = run.execute(&sender_for_cmd).await;
                    sender_for_cmd.input(AppMsg::CsvImportFinished(Box::new(report)));
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_csv_import_progress(&self, committed: u64) {
        if let Some(progress) = &self.csv_import_progress {
            progress.set_committed(committed);
        }
    }

    pub(super) fn on_csv_import_finished(&mut self, report: CsvImportReport, sender: ComponentSender<Self>) {
        if let Some(progress) = self.csv_import_progress.take() {
            progress.close();
        }
        sender.input(AppMsg::RefreshPage);
        let Some(error) = report.error else {
            self.show_toast(
                &crate::tr!("Imported {done} rows into {table}")
                    .replace("{done}", &report.committed.to_string())
                    .replace("{table}", &report.table),
            );
            return;
        };
        self.show_error_alert(
            &crate::tr!("Import stopped"),
            &crate::tr!("{done} of {total} rows were written to {table} before the import stopped. {error}")
                .replace("{done}", &report.committed.to_string())
                .replace("{total}", &report.total.to_string())
                .replace("{table}", &report.table)
                .replace("{error}", &error),
        );
    }
}

struct ImportRun {
    guard: Arc<PolicyGuard>,
    plan: InsertPlan,
    schema: Option<String>,
    table: String,
    token: CancellationToken,
    timeout_secs: u32,
}

impl ImportRun {
    async fn execute(self, sender: &ComponentSender<App>) -> CsvImportReport {
        let total = self.plan.row_count();
        let request = BulkInsertRequest {
            schema: self.schema.clone(),
            table: self.table.clone(),
            statement: self.plan.statement.clone(),
            row_budget: total,
        };
        let mut scope = match self.guard.begin_bulk_insert(request).await {
            Ok(scope) => scope,
            Err(error) => {
                return CsvImportReport {
                    table: self.table,
                    committed: 0,
                    total,
                    error: Some(error_text::driver_message(&error)),
                };
            }
        };
        let outcome = self.run_batches(&mut scope, sender).await;
        let end = match &outcome {
            Ok(()) => BulkInsertEnd::Completed,
            Err(BatchStop::Cancelled) => BulkInsertEnd::Cancelled,
            Err(BatchStop::Failed(error)) => BulkInsertEnd::Failed(error),
        };
        let committed = self
            .guard
            .finish_bulk_insert(&mut scope, end)
            .await
            .unwrap_or_else(|_| scope.committed_rows());
        CsvImportReport {
            table: self.table,
            committed,
            total,
            error: stop_message(outcome),
        }
    }

    async fn run_batches(
        &self,
        scope: &mut tablepro_policy::BulkInsertScope,
        sender: &ComponentSender<App>,
    ) -> Result<(), BatchStop> {
        let mut committed = 0;
        for rows in self.plan.batches(DEFAULT_IMPORT_BATCH_ROWS) {
            if self.token.is_cancelled() {
                return Err(BatchStop::Cancelled);
            }
            let control = self.control();
            let batch = BulkBatch {
                schema: self.schema.as_deref(),
                table: &self.table,
                statement: &self.plan.statement,
                rows,
            };
            match self.guard.execute_bulk_batch(scope, batch, &control).await {
                Ok(written) => committed += written,
                Err(DriverError::Cancelled) => return Err(BatchStop::Cancelled),
                Err(error) => return Err(BatchStop::Failed(error)),
            }
            sender.input(AppMsg::CsvImportProgress(committed));
        }
        Ok(())
    }

    fn control(&self) -> OperationControl {
        crate::services::operation_control::bounded_with(self.timeout_secs, self.token.clone())
    }
}

enum BatchStop {
    Cancelled,
    Failed(DriverError),
}

fn stop_message(outcome: Result<(), BatchStop>) -> Option<String> {
    match outcome {
        Ok(()) => None,
        Err(BatchStop::Cancelled) => Some(crate::tr!("The import was cancelled.")),
        Err(BatchStop::Failed(error)) => Some(error_text::driver_message(&error)),
    }
}

async fn prepare_import(
    conn: Arc<dyn tablepro_core::Connection>,
    control: &OperationControl,
    schema: Option<String>,
    table: String,
    path: PathBuf,
) -> Result<CsvImportPreparation, String> {
    let bytes = read_file(&path)?;
    let format = detect_format(&bytes);
    let columns = conn
        .fetch_columns_controlled(schema.as_deref(), &table, control)
        .await
        .map_err(|error| error_text::driver_message(&error))?;
    if columns.is_empty() {
        return Err(crate::tr!("The table has no columns to import into."));
    }
    Ok(CsvImportPreparation {
        schema,
        table,
        path,
        bytes,
        format,
        columns,
    })
}

fn read_file(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let size = std::fs::metadata(path)
        .map_err(|_| crate::tr!("The file could not be opened."))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(crate::tr!("The file is larger than the {limit} MB import limit.")
            .replace("{limit}", &(MAX_FILE_BYTES / (1024 * 1024)).to_string()));
    }
    std::fs::read(path).map_err(|_| crate::tr!("The file could not be read."))
}

fn plan_for(driver_id: &str, choice: &ImportChoice) -> Result<InsertPlan, String> {
    let sheet = read_csv(&choice.bytes, &choice.options, None).map_err(|error| error.to_string())?;
    let target = ImportTarget {
        driver_id,
        schema: choice.schema.as_deref(),
        table: &choice.table,
        columns: &choice.columns,
        mapping: &choice.mapping,
    };
    build_insert_plan(&target, &sheet, &choice.options).map_err(plan_error_text)
}

/// A plan failure names the rows it could not read, never their contents.
fn plan_error_text(error: PlanError) -> String {
    let PlanError::Rows { first, .. } = &error else {
        return error.to_string();
    };
    let lines: Vec<String> = first
        .iter()
        .map(|row| {
            crate::tr!("Line {line}, column {column}: {reason}")
                .replace("{line}", &row.line.to_string())
                .replace("{column}", &row.column)
                .replace("{reason}", &row.reason.to_string())
        })
        .collect();
    format!("{}\n\n{}", error, lines.join("\n"))
}
