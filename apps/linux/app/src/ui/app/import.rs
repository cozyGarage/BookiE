use std::path::PathBuf;
use std::sync::Arc;

use bookie_core::import::{
    ConflictKey, CsvFormat, DEFAULT_IMPORT_BATCH_ROWS, ImportError, ImportTarget, InsertPlan, MAX_FILE_BYTES,
    PlanError, build_insert_plan, conflict_key_candidates, detect_format, read_csv, read_import_file,
};
use bookie_core::sql_ddl::build_create_table;
use bookie_core::{ColumnInfo, Connection, DriverCapabilities, DriverError, OperationControl};
use bookie_policy::{BulkBatch, BulkInsertEnd, BulkInsertRequest, PolicyGuard};
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::gtk::gio;
use relm4::{ComponentController, ComponentSender};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::services::csv_import_mappings::CsvMappingScope;
use crate::services::error_text;
use crate::ui::import_dialog::{ImportChoice, ImportMode, ImportRequest};

mod queue;
mod skips;

pub use queue::{ImportQueueFileStarted, ImportQueueFinished};

use super::browse::{RefreshDecision, refresh_decision};
use super::types::WorkspaceTab;
use super::{App, AppMsg, BrowseTabInput};

/// Everything the file dialog collected, handed back to the GTK thread.
#[derive(Debug)]
pub struct CsvImportPreparation {
    pub connection_id: uuid::Uuid,
    pub database: String,
    pub schema: Option<String>,
    /// The existing table, or `None` when the import is creating one.
    pub table: Option<String>,
    pub path: PathBuf,
    pub bytes: Arc<[u8]>,
    pub format: CsvFormat,
    pub columns: Vec<ColumnInfo>,
    pub keys: Vec<ConflictKey>,
    pub more_files: Vec<PathBuf>,
    pub driver_id: String,
    pub capabilities: DriverCapabilities,
}

/// How an import ended, as the GTK thread hears it.
#[derive(Debug)]
pub struct CsvImportReport {
    pub schema: Option<String>,
    pub table: String,
    pub committed: u64,
    pub total: u64,
    pub error: Option<String>,
    pub skipped: u64,
    pub skipped_summary: String,
    pub updates_existing_rows: bool,
    pub cancelled: bool,
}

impl CsvImportReport {
    fn stopped(schema: Option<String>, table: String, total: u64, error: String) -> Self {
        Self {
            schema,
            table,
            committed: 0,
            total,
            error: Some(error),
            skipped: 0,
            skipped_summary: String::new(),
            updates_existing_rows: false,
            cancelled: false,
        }
    }
}

#[derive(Debug)]
pub struct CsvImportPreparationFailure {
    pub connection_id: Uuid,
    pub driver_id: String,
    pub message: String,
}

pub struct CsvImportPlanPreparation {
    connection_id: Uuid,
    driver_id: String,
    choice: Option<ImportChoice>,
    result: Result<(InsertPlan, Option<String>), String>,
}

impl std::fmt::Debug for CsvImportPlanPreparation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CsvImportPlanPreparation")
            .field("connection_id", &self.connection_id)
            .field("driver_id", &self.driver_id)
            .field("result_ready", &self.result.is_ok())
            .finish()
    }
}

impl App {
    fn begin_csv_import_preparation(&mut self, label: &str) -> Option<CancellationToken> {
        if self.csv_import_progress.is_some() {
            return None;
        }
        self.csv_import_progress = Some(crate::ui::import_dialog::present_preparing(&self.window, label));
        self.csv_import_progress
            .as_ref()
            .map(crate::ui::import_dialog::ImportProgress::cancel_token)
    }

    fn close_csv_import_progress(&mut self) {
        if let Some(progress) = self.csv_import_progress.take() {
            progress.close();
        }
    }

    pub(super) fn on_import_csv_into_table(
        &self,
        schema: Option<String>,
        table: String,
        sender: ComponentSender<Self>,
    ) {
        self.choose_csv_file(schema, Some(table), sender);
    }

    pub(super) fn on_create_table_from_csv(&self, schema: Option<String>, sender: ComponentSender<Self>) {
        if !bookie_core::import::supports_table_creation(self.driver_id()) {
            self.show_error_alert(
                &bookie_app_services::tr!("Can't create a table"),
                &bookie_app_services::tr!(
                    "This connection type cannot create a table from a file. Import into an existing table instead."
                ),
            );
            return;
        }
        self.choose_csv_file(schema, None, sender);
    }

    fn choose_csv_file(&self, schema: Option<String>, table: Option<String>, sender: ComponentSender<Self>) {
        if let Some(message) = self.import_refusal() {
            self.show_error_alert(&bookie_app_services::tr!("Can't import"), &message);
            return;
        }
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&bookie_app_services::tr!("CSV and Excel files")));
        filter.add_mime_type("text/csv");
        filter.add_suffix("csv");
        filter.add_suffix("tsv");
        filter.add_suffix("xlsx");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(bookie_app_services::tr!("Import CSV"))
            .modal(true)
            .default_filter(&filter)
            .filters(&filters)
            .build();
        if table.is_none() {
            dialog.open(Some(&self.window), gio::Cancellable::NONE, move |outcome| {
                let Some(path) = outcome.ok().and_then(|file| file.path()) else {
                    return;
                };
                sender.input(AppMsg::CsvFileChosen {
                    schema: schema.clone(),
                    table: table.clone(),
                    path,
                    more_files: Vec::new(),
                });
            });
            return;
        }
        dialog.open_multiple(Some(&self.window), gio::Cancellable::NONE, move |outcome| {
            let Ok(files) = outcome else {
                return;
            };
            let mut paths = chosen_paths(&files).into_iter();
            let Some(path) = paths.next() else {
                return;
            };
            sender.input(AppMsg::CsvFileChosen {
                schema: schema.clone(),
                table: table.clone(),
                path,
                more_files: paths.collect(),
            });
        });
    }

    /// Why an import cannot start at all. Checked before the user picks a
    /// file, so a refusal never arrives after the work looks under way.
    fn import_refusal(&self) -> Option<String> {
        if self.connection_id.is_none() || self.current_driver_id.is_none() {
            return Some(bookie_app_services::tr!("No active connection."));
        }
        if self.read_only {
            return Some(bookie_app_services::tr!("This connection is open read-only."));
        }
        if !self.database.policy_available() {
            return Some(bookie_app_services::tr!(
                "The policy rules could not be loaded, so writes are not permitted."
            ));
        }
        if self
            .connection_id
            .is_some_and(|id| self.database.governed_writes_disabled(id))
        {
            return Some(bookie_app_services::tr!(
                "A previous operation's audit record could not be written, so writes are not permitted."
            ));
        }
        None
    }

    pub(super) fn on_csv_file_chosen(
        &mut self,
        schema: Option<String>,
        table: Option<String>,
        path: PathBuf,
        more_files: Vec<PathBuf>,
        sender: ComponentSender<Self>,
    ) {
        let (Some(connection_id), Some(driver_id)) = (self.connection_id, self.current_driver_id.clone()) else {
            return;
        };
        let Some(capabilities) = self.registry.get(&driver_id).map(|driver| driver.capabilities()) else {
            self.show_error_alert(
                &bookie_app_services::tr!("Can't import"),
                &bookie_app_services::tr!("This connection type is not available in this build."),
            );
            return;
        };
        let Some(conn) = self.database.get(connection_id) else {
            return;
        };
        let Some(cancellation) = self.begin_csv_import_preparation("CSV file") else {
            return;
        };
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let database = self
            .saved_connections
            .iter()
            .find(|saved| saved.id == connection_id)
            .map(|saved| saved.database.clone())
            .unwrap_or_default();
        let failure_driver_id = driver_id.clone();
        sender.clone().command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, cancellation);
                    match prepare_import(
                        conn,
                        &control,
                        Prepare {
                            schema,
                            table,
                            path,
                            more_files,
                            driver_id,
                            capabilities,
                            connection_id,
                            database,
                        },
                    )
                    .await
                    {
                        Ok(preparation) => sender.input(AppMsg::CsvImportPrepared(Box::new(preparation))),
                        Err(message) => sender.input(AppMsg::CsvImportPreparationFailed(Box::new(
                            CsvImportPreparationFailure {
                                connection_id,
                                driver_id: failure_driver_id,
                                message,
                            },
                        ))),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_csv_import_prepared(&mut self, preparation: CsvImportPreparation, sender: ComponentSender<Self>) {
        if self.connection_id != Some(preparation.connection_id)
            || self.current_driver_id.as_deref() != Some(preparation.driver_id.as_str())
            || self
                .saved_connections
                .iter()
                .find(|saved| saved.id == preparation.connection_id)
                .is_none_or(|saved| saved.database != preparation.database)
        {
            if let Some(progress) = self.csv_import_progress.take() {
                progress.close();
            }
            return;
        }
        let Some(progress) = self.csv_import_progress.take() else {
            return;
        };
        let cancelled = progress.is_cancelled();
        progress.close();
        if cancelled {
            return;
        }
        let mapping_scope = preparation.table.as_ref().map(|table| CsvMappingScope {
            connection_id: preparation.connection_id,
            database: preparation.database.clone(),
            driver_id: preparation.driver_id.clone(),
            capabilities: preparation.capabilities,
            schema: preparation.schema.clone(),
            table: table.clone(),
        });
        let mode = match preparation.table {
            Some(table) => ImportMode::ExistingTable {
                capabilities: preparation.capabilities,
                table,
                columns: preparation.columns,
                keys: preparation.keys,
            },
            None => ImportMode::NewTable {
                driver_id: preparation.driver_id,
                capabilities: preparation.capabilities,
                suggested_table: suggested_table_name(&preparation.path),
            },
        };
        let request = ImportRequest {
            schema: preparation.schema,
            path: preparation.path,
            bytes: preparation.bytes,
            format: preparation.format,
            mode,
            mapping_store: self.persistence.csv_import_mappings.clone(),
            mapping_scope,
            more_files: preparation.more_files,
        };
        crate::ui::import_dialog::present(&self.window, request, move |choice| {
            sender.input(AppMsg::StartCsvImport(Box::new(choice)));
        });
    }

    pub(super) fn on_csv_import_preparation_failed(&mut self, failure: CsvImportPreparationFailure) {
        if self.connection_id != Some(failure.connection_id)
            || self.current_driver_id.as_deref() != Some(failure.driver_id.as_str())
        {
            self.close_csv_import_progress();
            return;
        }
        let cancelled = self
            .csv_import_progress
            .as_ref()
            .is_some_and(crate::ui::import_dialog::ImportProgress::is_cancelled);
        self.close_csv_import_progress();
        if cancelled {
            return;
        }
        self.show_error_alert(&bookie_app_services::tr!("Can't import"), &failure.message);
    }

    pub(super) fn on_start_csv_import(&mut self, choice: ImportChoice, sender: ComponentSender<Self>) {
        if let Some(message) = self.import_refusal() {
            self.show_error_alert(&bookie_app_services::tr!("Can't import"), &message);
            return;
        }
        let (Some(connection_id), Some(driver_id)) = (self.connection_id, self.current_driver_id.clone()) else {
            return;
        };
        if self.begin_csv_import_preparation(&choice.table).is_none() {
            return;
        }
        if !choice.more_files.is_empty() {
            self.start_import_queue(choice, connection_id, sender);
            return;
        }
        let worker_driver_id = driver_id.clone();
        sender.clone().command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let prepared = prepare_csv_plan(connection_id, worker_driver_id, choice).await;
                    sender.input(AppMsg::CsvImportPlanPrepared(Box::new(prepared)));
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_csv_import_plan_prepared(
        &mut self,
        preparation: CsvImportPlanPreparation,
        sender: ComponentSender<Self>,
    ) {
        if self.connection_id != Some(preparation.connection_id)
            || self.current_driver_id.as_deref() != Some(preparation.driver_id.as_str())
        {
            self.close_csv_import_progress();
            return;
        }
        if let Some(message) = self.import_refusal() {
            self.close_csv_import_progress();
            self.show_error_alert(&bookie_app_services::tr!("Can't import"), &message);
            return;
        }
        let (plan, create_sql) = match preparation.result {
            Ok(prepared) => prepared,
            Err(message) => {
                self.close_csv_import_progress();
                self.show_error_alert(&bookie_app_services::tr!("Can't import"), &message);
                return;
            }
        };
        if self
            .csv_import_progress
            .as_ref()
            .is_none_or(crate::ui::import_dialog::ImportProgress::is_cancelled)
        {
            self.close_csv_import_progress();
            return;
        }
        let Some(guard) = self
            .database
            .guard(preparation.connection_id, bookie_policy::Principal::human_gui())
        else {
            self.close_csv_import_progress();
            self.show_error_alert(
                &bookie_app_services::tr!("Can't import"),
                &bookie_app_services::tr!("No active connection."),
            );
            return;
        };
        let Some(choice) = preparation.choice else {
            self.close_csv_import_progress();
            self.show_error_alert(
                &bookie_app_services::tr!("Can't import"),
                &bookie_app_services::tr!("The import plan could not be prepared."),
            );
            return;
        };
        let Some(progress) = self.csv_import_progress.as_mut() else {
            return;
        };
        progress.set_total(plan.row_count());
        let token = progress.cancel_token();
        self.run_import(guard, plan, create_sql, choice, token, sender);
    }

    fn run_import(
        &self,
        guard: Arc<PolicyGuard>,
        plan: InsertPlan,
        create_sql: Option<String>,
        choice: ImportChoice,
        token: CancellationToken,
        sender: ComponentSender<Self>,
    ) {
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let sender_for_cmd = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let run = ImportRun {
                        guard,
                        plan,
                        create_sql,
                        schema: choice.schema,
                        table: choice.table,
                        token,
                        timeout_secs,
                        skip_rejected_rows: choice.skip_rejected_rows,
                        updates_existing_rows: choice.write.updates_existing_rows(),
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

    pub(super) fn on_csv_import_finished(&mut self, report: CsvImportReport, _sender: ComponentSender<Self>) {
        if let Some(progress) = self.csv_import_progress.take() {
            progress.close();
        }
        self.refresh_import_targets(report.schema.as_deref(), &report.table);
        let Some(error) = report.error else {
            if report.skipped > 0 {
                self.report_skipped_rows(&report);
                return;
            }
            self.show_toast(&imported_toast(&report));
            return;
        };
        let message = bookie_app_services::tr!(
            "{done} of {total} rows were written to {table} before the import stopped. {error}"
        )
        .replace("{done}", &report.committed.to_string())
        .replace("{total}", &report.total.to_string())
        .replace("{table}", &report.table)
        .replace("{error}", &error)
            + &skipped_note(report.skipped, &report.skipped_summary);
        self.show_error_alert(&bookie_app_services::tr!("Import stopped"), &message);
    }

    fn report_skipped_rows(&self, report: &CsvImportReport) {
        self.show_toast(
            &bookie_app_services::tr!("Imported {done} rows into {table}; {skipped} skipped")
                .replace("{done}", &report.committed.to_string())
                .replace("{table}", &report.table)
                .replace("{skipped}", &report.skipped.to_string()),
        );
        let message = skipped_rows_message(report);
        self.show_error_alert(&bookie_app_services::tr!("Import finished with skipped rows"), &message);
    }
}

fn imported_toast(report: &CsvImportReport) -> String {
    let template = if report.updates_existing_rows {
        bookie_app_services::tr!("Imported or updated {done} rows in {table}")
    } else {
        bookie_app_services::tr!("Imported {done} rows into {table}")
    };
    template
        .replace("{done}", &report.committed.to_string())
        .replace("{table}", &report.table)
}

fn skipped_rows_message(report: &CsvImportReport) -> String {
    bookie_app_services::tr!(
        "{done} of {total} rows were written to {table}. The database rejected {skipped} rows, which were skipped and not imported."
    )
    .replace("{done}", &report.committed.to_string())
    .replace("{total}", &report.total.to_string())
    .replace("{table}", &report.table)
    .replace("{skipped}", &report.skipped.to_string())
        + "\n\n"
        + &report.skipped_summary
}

fn skipped_note(skipped: u64, summary: &str) -> String {
    if skipped == 0 {
        return String::new();
    }
    bookie_app_services::tr!("\n\n{skipped} rows were skipped before the import stopped.\n{summary}")
        .replace("{skipped}", &skipped.to_string())
        .replace("{summary}", summary)
}

impl App {
    fn refresh_import_targets(&self, schema: Option<&str>, table: &str) {
        let targets: Vec<Uuid> = self
            .workspace_tabs
            .borrow()
            .iter()
            .filter_map(|(id, tab)| match tab {
                WorkspaceTab::Table(slot)
                    if import_targets_table(slot.schema.as_deref(), &slot.table, schema, table) =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        for id in targets {
            let save_in_flight = self
                .workspace_tabs
                .borrow()
                .get(&id)
                .and_then(|tab| tab.browse_controller())
                .is_some_and(|controller| controller.model().save_in_flight());
            let dirty = self
                .change_trackers
                .with_tab_ref(id, |tr| tr.has_pending())
                .unwrap_or(false);
            if refresh_decision(dirty, save_in_flight) == RefreshDecision::Refresh {
                self.dispatch_to_tab(id, BrowseTabInput::Refresh);
            }
        }
    }
}

fn import_targets_table(tab_schema: Option<&str>, tab_table: &str, schema: Option<&str>, table: &str) -> bool {
    tab_schema == schema && tab_table == table
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    use crate::ui::app::browse::{RefreshDecision, refresh_decision};

    #[test]
    fn only_the_imported_schema_and_table_are_refreshed() {
        assert!(import_targets_table(Some("public"), "people", Some("public"), "people"));
        assert!(!import_targets_table(
            Some("public"),
            "orders",
            Some("public"),
            "people"
        ));
        assert!(!import_targets_table(
            Some("archive"),
            "people",
            Some("public"),
            "people"
        ));
        assert!(!import_targets_table(None, "people", Some("public"), "people"));
    }

    #[test]
    fn a_dirty_matching_tab_is_not_refreshed_by_an_import() {
        assert_eq!(refresh_decision(false, false), RefreshDecision::Refresh);
        assert_ne!(refresh_decision(true, false), RefreshDecision::Refresh);
        assert_ne!(refresh_decision(false, true), RefreshDecision::Refresh);
    }
}

struct ImportRun {
    guard: Arc<PolicyGuard>,
    plan: InsertPlan,
    /// The CREATE TABLE this import runs first, as its own governed
    /// statement. It is classified, decided and audited on its own.
    create_sql: Option<String>,
    schema: Option<String>,
    table: String,
    token: CancellationToken,
    timeout_secs: u32,
    skip_rejected_rows: bool,
    updates_existing_rows: bool,
}

impl ImportRun {
    async fn execute(self, sender: &ComponentSender<App>) -> CsvImportReport {
        let total = self.plan.row_count();
        if let Err(message) = self.create_table().await {
            return CsvImportReport::stopped(self.schema.clone(), self.table, total, message);
        }
        let request = BulkInsertRequest {
            schema: self.schema.clone(),
            table: self.table.clone(),
            statement: self.plan.statement.clone(),
            row_budget: total,
            sequence_columns: if self.guard.context().driver_id.eq_ignore_ascii_case("postgres") {
                self.plan
                    .columns
                    .iter()
                    .filter(|column| column.is_auto_increment)
                    .map(|column| column.name.clone())
                    .collect()
            } else {
                Vec::new()
            },
        };
        let mut scope = match self.guard.begin_bulk_insert(request).await {
            Ok(scope) => scope,
            Err(error) => {
                let message = error_text::driver_message(&error);
                return CsvImportReport::stopped(self.schema.clone(), self.table, total, message);
            }
        };
        let mut skips = skips::SkipLog::default();
        let outcome = self.run_batches(&mut scope, sender, &mut skips).await;
        let end = match &outcome {
            Ok(()) => BulkInsertEnd::Completed,
            Err(BatchStop::Cancelled) => BulkInsertEnd::Cancelled,
            Err(BatchStop::Failed(error)) => BulkInsertEnd::Failed(error),
        };
        let cancelled = matches!(outcome, Err(BatchStop::Cancelled));
        let committed = self
            .guard
            .finish_bulk_insert(&mut scope, end)
            .await
            .unwrap_or_else(|_| scope.committed_rows());
        CsvImportReport {
            schema: self.schema.clone(),
            table: self.table,
            committed,
            total,
            error: stop_message(outcome),
            skipped: skips.total(),
            skipped_summary: skips.summary(),
            updates_existing_rows: self.updates_existing_rows,
            cancelled,
        }
    }

    async fn create_table(&self) -> Result<(), String> {
        let Some(sql) = &self.create_sql else {
            return Ok(());
        };
        let control = self.control();
        self.guard
            .execute_controlled(sql, &control)
            .await
            .map(|_| ())
            .map_err(|error| error_text::driver_message(&error))
    }

    async fn run_batches(
        &self,
        scope: &mut bookie_policy::BulkInsertScope,
        sender: &ComponentSender<App>,
        skips: &mut skips::SkipLog,
    ) -> Result<(), BatchStop> {
        let mut committed = 0;
        let mut offered = 0u64;
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
            let written = if self.skip_rejected_rows {
                self.guard
                    .execute_bulk_batch_skipping_failed_rows(scope, batch, &control)
                    .await
                    .map(|outcome| {
                        skips.record(offered, &outcome.skipped);
                        outcome.written
                    })
            } else {
                self.guard.execute_bulk_batch(scope, batch, &control).await
            };
            offered += rows.len() as u64;
            match written {
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
        Err(BatchStop::Cancelled) => Some(bookie_app_services::tr!("The import was cancelled.")),
        Err(BatchStop::Failed(error)) => Some(error_text::driver_message(&error)),
    }
}

struct Prepare {
    schema: Option<String>,
    table: Option<String>,
    path: PathBuf,
    more_files: Vec<PathBuf>,
    driver_id: String,
    capabilities: DriverCapabilities,
    connection_id: uuid::Uuid,
    database: String,
}

async fn prepare_import(
    conn: Arc<dyn Connection>,
    control: &OperationControl,
    prepare: Prepare,
) -> Result<CsvImportPreparation, String> {
    let path = prepare.path.clone();
    let queued = prepare.more_files.clone();
    let first = prepare.path.clone();
    tokio::task::spawn_blocking(move || check_file_queue(&first, &queued))
        .await
        .map_err(|_| bookie_app_services::tr!("The import file could not be prepared."))??;
    let bytes = tokio::task::spawn_blocking(move || read_file(&path).map(Arc::from))
        .await
        .map_err(|_| bookie_app_services::tr!("The import file could not be prepared."))??;
    if control.cancellation_token().is_cancelled() {
        return Err(bookie_app_services::tr!("Import preparation was cancelled."));
    }
    let format = detect_format(&bytes);
    let (columns, keys) = match &prepare.table {
        None => (Vec::new(), Vec::new()),
        Some(table) => {
            let columns = existing_columns(conn.clone(), control, prepare.schema.as_deref(), table).await?;
            let keys = existing_keys(conn, control, &prepare, table, &columns).await?;
            (columns, keys)
        }
    };
    Ok(CsvImportPreparation {
        connection_id: prepare.connection_id,
        database: prepare.database,
        schema: prepare.schema,
        table: prepare.table,
        path: prepare.path,
        bytes,
        format,
        columns,
        keys,
        more_files: prepare.more_files,
        driver_id: prepare.driver_id,
        capabilities: prepare.capabilities,
    })
}

async fn existing_keys(
    conn: Arc<dyn Connection>,
    control: &OperationControl,
    prepare: &Prepare,
    table: &str,
    columns: &[ColumnInfo],
) -> Result<Vec<ConflictKey>, String> {
    if prepare.capabilities.import_upsert == bookie_core::ImportUpsertStyle::Unsupported {
        return Ok(Vec::new());
    }
    let indexes = conn
        .fetch_indexes_controlled(prepare.schema.as_deref(), table, control)
        .await
        .map_err(|error| error_text::driver_message(&error))?;
    Ok(conflict_key_candidates(columns, &indexes))
}

fn chosen_paths(files: &gio::ListModel) -> Vec<PathBuf> {
    (0..files.n_items())
        .filter_map(|index| files.item(index).and_downcast::<gio::File>())
        .filter_map(|file| file.path())
        .collect()
}

fn check_file_queue(first: &std::path::Path, more: &[PathBuf]) -> Result<(), String> {
    if more.is_empty() {
        return Ok(());
    }
    let sizes: Vec<(PathBuf, u64)> = std::iter::once(first)
        .chain(more.iter().map(PathBuf::as_path))
        .map(|path| {
            let bytes = std::fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0);
            (path.to_path_buf(), bytes)
        })
        .collect();
    bookie_app_services::import_queue::check_queue(&sizes).map_err(|error| error.to_string())
}

async fn existing_columns(
    conn: Arc<dyn Connection>,
    control: &OperationControl,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<ColumnInfo>, String> {
    let columns = conn
        .fetch_columns_controlled(schema, table, control)
        .await
        .map_err(|error| error_text::driver_message(&error))?;
    if columns.is_empty() {
        return Err(bookie_app_services::tr!("The table has no columns to import into."));
    }
    Ok(columns)
}

/// The CREATE the import runs before its first row, or nothing when the
/// table is already there.
fn create_statement(driver_id: &str, choice: &ImportChoice) -> Result<Option<String>, String> {
    let Some(drafts) = &choice.create else {
        return Ok(None);
    };
    let statements = build_create_table(driver_id, choice.schema.as_deref(), &choice.table, drafts, &[], &[])
        .map_err(|error| error.to_string())?;
    statements
        .into_iter()
        .next()
        .map(Some)
        .ok_or_else(|| bookie_app_services::tr!("The table could not be built from the file."))
}

fn suggested_table_name(path: &std::path::Path) -> String {
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().to_string());
    let cleaned: String = stem
        .unwrap_or_default()
        .chars()
        .map(|character| if character.is_alphanumeric() { character } else { '_' })
        .collect();
    let cleaned = cleaned.trim_matches('_').to_lowercase();
    if cleaned.is_empty() {
        return "imported_table".to_owned();
    }
    cleaned
}

fn read_file(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let bytes = read_import_file(path).map_err(|error| match error {
        ImportError::FileTooLarge { .. } => {
            bookie_app_services::tr!("The file is larger than the {limit} MB import limit.")
                .replace("{limit}", &(MAX_FILE_BYTES / (1024 * 1024)).to_string())
        }
        _ => bookie_app_services::tr!("The file could not be read."),
    })?;
    if bookie_core::import::is_xlsx(&bytes) {
        return bookie_core::import::xlsx_to_csv(&bytes).map_err(|error| error.to_string());
    }
    Ok(bytes)
}

fn plan_for(driver_id: &str, choice: &ImportChoice) -> Result<InsertPlan, String> {
    let sheet = read_csv(&choice.bytes, &choice.options, None).map_err(|error| error.to_string())?;
    let target = ImportTarget {
        driver_id,
        capabilities: choice.capabilities,
        schema: choice.schema.as_deref(),
        table: &choice.table,
        columns: &choice.columns,
        mapping: &choice.mapping,
    };
    let plan = build_insert_plan(&target, &sheet, &choice.options).map_err(plan_error_text)?;
    choice.write.apply(plan, &target).map_err(plan_error_text)
}

async fn prepare_csv_plan(connection_id: Uuid, driver_id: String, choice: ImportChoice) -> CsvImportPlanPreparation {
    let worker_driver_id = driver_id.clone();
    let prepared = tokio::task::spawn_blocking(move || {
        let result = (|| {
            let plan = plan_for(&worker_driver_id, &choice)?;
            let create_sql = create_statement(&worker_driver_id, &choice)?;
            Ok((plan, create_sql))
        })();
        CsvImportPlanPreparation {
            connection_id,
            driver_id: worker_driver_id,
            choice: Some(choice),
            result,
        }
    })
    .await;
    prepared.unwrap_or(CsvImportPlanPreparation {
        connection_id,
        driver_id,
        choice: None,
        result: Err(bookie_app_services::tr!("The import plan could not be prepared.")),
    })
}

/// A plan failure names the rows it could not read, never their contents.
fn plan_error_text(error: PlanError) -> String {
    let PlanError::Rows { first, .. } = &error else {
        return error.to_string();
    };
    let lines: Vec<String> = first
        .iter()
        .map(|row| {
            bookie_app_services::tr!("Line {line}, column {column}: {reason}")
                .replace("{line}", &row.line.to_string())
                .replace("{column}", &row.column)
                .replace("{reason}", &row.reason.to_string())
        })
        .collect();
    format!("{}\n\n{}", error, lines.join("\n"))
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "import_upsert_tests.rs"]
mod upsert_tests;
