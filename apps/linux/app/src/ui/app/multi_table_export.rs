use bookie_app_services::catalog::CatalogOrigin;
use bookie_app_services::export_tables::TableTarget;
use bookie_app_services::export_tables::{TableExportRequest, TableOutcome, TableResult, export_tables};
use bookie_core::export::{CsvOptions, ResultFormat};
use bookie_core::{Connection, DriverError, OperationControl, TableInfo};
use relm4::adw::prelude::*;
use relm4::gtk::gio;
use relm4::{ComponentSender, gtk};
use std::path::PathBuf;

use super::{App, AppMsg};

#[derive(Debug, Clone)]
pub struct Selection {
    origin: CatalogOrigin,
    tables: Vec<TableInfo>,
    max_rows: Option<usize>,
}

impl App {
    pub(super) fn on_export_tables(&self, sender: ComponentSender<Self>) {
        let Some(origin) = CatalogOrigin::capture(self.connection_id, &self.database) else {
            self.show_toast(&bookie_app_services::tr!("Connect to a database first."));
            return;
        };
        let supported = self
            .registry
            .get(self.driver_id())
            .is_some_and(|driver| driver.capabilities().supports_snapshot_export);
        if !supported {
            self.show_toast(&bookie_app_services::tr!(
                "This connection does not support consistent table export."
            ));
            return;
        }
        if self.multi_table_progress.is_some() {
            self.show_toast(&bookie_app_services::tr!("A table export is already running."));
            return;
        }
        let Some(catalog) = self.multi.catalogs.get(&origin.id) else {
            self.show_toast(&bookie_app_services::tr!("The table catalog is not available yet."));
            return;
        };
        crate::ui::multi_table_export::present(&self.window, catalog.tables.to_vec(), move |tables, max_rows| {
            sender.input(AppMsg::MultiTableDestination(Selection {
                origin: origin.clone(),
                tables,
                max_rows,
            }));
        });
    }

    pub(super) fn choose_multi_table_destination(&self, selection: Selection, sender: ComponentSender<Self>) {
        if !selection.origin.owns_window(self.connection_id, &self.database) {
            self.show_toast(&bookie_app_services::tr!(
                "The connection changed. Start the export again."
            ));
            return;
        }
        let dialog = gtk::FileDialog::builder()
            .title(bookie_app_services::tr!("Choose export folder"))
            .modal(true)
            .build();
        dialog.select_folder(Some(&self.window), gio::Cancellable::NONE, move |result| {
            if let Some(directory) = result.ok().and_then(|file| file.path()) {
                sender.input(AppMsg::StartMultiTableExport(selection.clone(), directory));
            }
        });
    }

    pub(super) fn start_multi_table_export(
        &mut self,
        selection: Selection,
        directory: PathBuf,
        sender: ComponentSender<Self>,
    ) {
        if self.multi_table_progress.is_some() || !selection.origin.owns_window(self.connection_id, &self.database) {
            self.show_toast(&bookie_app_services::tr!(
                "The connection changed or an export is running."
            ));
            return;
        }
        let Some(guard) = selection.origin.guard(&self.database) else {
            return;
        };
        let Some(driver) = self.registry.get(self.driver_id()) else {
            return;
        };
        let driver_id = self.driver_id().to_owned();
        let timeout =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let cancel = self.show_multi_table_progress();
        let finished = sender.clone();
        let completion = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout, cancel.clone());
                    let targets = prepare_targets(guard.as_ref(), selection.tables, &control).await;
                    let result = match targets {
                        Err(error) => Err(error),
                        Ok(targets) => tokio::task::spawn_blocking(move || {
                            let csv = CsvOptions::default();
                            let request = TableExportRequest {
                                driver_id: &driver_id,
                                tables: &targets,
                                directory: &directory,
                                format: ResultFormat::Csv,
                                extension: "csv",
                                csv: &csv,
                                max_rows_per_table: selection.max_rows,
                                timeout_secs: timeout,
                            };
                            let last = std::cell::Cell::new(None);
                            export_tables(
                                guard,
                                driver.as_ref(),
                                &request,
                                &super::browse::page_runner(),
                                &cancel,
                                |index, rows| {
                                    report_progress(&finished, &targets, &last, index, rows);
                                },
                            )
                        })
                        .await
                        .unwrap_or_else(|_| Err("Export task stopped unexpectedly.".into())),
                    };
                    completion.input(AppMsg::MultiTableExportFinished(result));
                })
                .drop_on_shutdown()
        });
    }

    fn show_multi_table_progress(&mut self) -> tokio_util::sync::CancellationToken {
        let label = gtk::Label::new(Some(&bookie_app_services::tr!("Preparing tables…")));
        label.set_wrap(true);
        let progress = crate::ui::progress_dialog::CancellableDialog::present(
            &self.window,
            &bookie_app_services::tr!("Exporting tables"),
            &[label.upcast_ref()],
        );
        let cancel = progress.cancel_token();
        self.multi_table_progress = Some(progress);
        self.multi_table_progress_label = Some(label);
        cancel
    }

    pub(super) fn update_multi_table_export_progress(&self, table: &str, rows: usize) {
        if let Some(label) = &self.multi_table_progress_label {
            label.set_label(&format!("{table}: {rows} rows"));
        }
    }

    pub(super) fn finish_multi_table_export(&mut self, result: Result<Vec<TableResult>, String>) {
        if let Some(progress) = self.multi_table_progress.take() {
            progress.close();
        }
        self.multi_table_progress_label = None;
        let body = match result {
            Err(error) => error,
            Ok(results) => results
                .into_iter()
                .map(|result| match result.outcome {
                    TableOutcome::Written { rows, capped } => format!(
                        "{}: {rows} rows{}",
                        result.table,
                        if capped { " (limited)" } else { "" }
                    ),
                    TableOutcome::Failed(error) => format!("{}: {error}", result.table),
                    TableOutcome::NotStarted => format!("{}: not started", result.table),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        };
        let dialog =
            relm4::adw::AlertDialog::new(Some(&bookie_app_services::tr!("Table export finished")), Some(&body));
        dialog.add_response("close", &bookie_app_services::tr!("Close"));
        dialog.present(Some(&self.window));
    }
}

fn target_name(target: &TableTarget) -> String {
    target
        .schema
        .as_ref()
        .map_or_else(|| target.table.clone(), |schema| format!("{schema}.{}", target.table))
}

fn report_progress(
    sender: &ComponentSender<App>,
    targets: &[TableTarget],
    last: &std::cell::Cell<Option<(usize, std::time::Instant)>>,
    index: usize,
    rows: usize,
) {
    if rows != 1 && !rows.is_multiple_of(1000) {
        return;
    }
    let now = std::time::Instant::now();
    if last
        .get()
        .is_none_or(|(table, sent)| table != index || now.duration_since(sent) >= std::time::Duration::from_millis(200))
    {
        last.set(Some((index, now)));
        sender.input(AppMsg::MultiTableExportProgress(target_name(&targets[index]), rows));
    }
}

async fn prepare_targets(
    connection: &dyn Connection,
    tables: Vec<TableInfo>,
    control: &OperationControl,
) -> Result<Vec<TableTarget>, String> {
    let mut targets = Vec::with_capacity(tables.len());
    for table in tables {
        if control.cancellation_token().is_cancelled() {
            return Err(crate::services::error_text::driver_message(&DriverError::Cancelled));
        }
        let columns = connection
            .fetch_columns_controlled(table.schema.as_deref(), &table.name, control)
            .await
            .map_err(|error| {
                let name = table
                    .schema
                    .as_ref()
                    .map_or_else(|| table.name.clone(), |schema| format!("{schema}.{}", table.name));
                format!("{name}: {}", crate::services::error_text::driver_message(&error))
            })?;
        if control.cancellation_token().is_cancelled() {
            return Err(crate::services::error_text::driver_message(&DriverError::Cancelled));
        }
        targets.push(TableTarget {
            schema: table.schema,
            table: table.name,
            columns,
        });
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::{ConnectOptions, DatabaseDriver};
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn metadata_preparation_keeps_target_identity_and_honors_cancellation() {
        let connection = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: ":memory:".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        connection
            .execute("CREATE TABLE orders (id INTEGER, name TEXT)")
            .await
            .unwrap();
        let table = TableInfo {
            schema: None,
            name: "orders".into(),
        };
        let token = CancellationToken::new();
        let control = OperationControl::new(token.clone(), None);
        let targets = prepare_targets(connection.as_ref(), vec![table.clone()], &control)
            .await
            .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].table, table.name);
        assert_eq!(targets[0].schema, table.schema);
        assert_eq!(
            targets[0]
                .columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "name"]
        );
        token.cancel();
        assert!(
            prepare_targets(connection.as_ref(), vec![table], &control)
                .await
                .is_err()
        );
    }
}
