use relm4::adw::prelude::*;
use relm4::{ComponentController, ComponentSender};

use bookie_core::sql_literal::RowStatement;
use bookie_core::{DriverError, Value};
use bookie_storage::query_history::{Outcome, Source};
use uuid::Uuid;

use crate::services::change_tracker::StatementSource;
use crate::services::error_text;

use super::ran_statements::{finish_if_recorded, sql_without_parameters};
use super::{App, AppMsg};

const MAX_COPIED_STATEMENT_ROWS: usize = 5_000;

impl App {
    /// Atomic Save handler for the inline-spreadsheet pattern.
    /// Receives a fully-materialised `Vec<(SQL, params)>` from a
    /// BrowseTab's `TabChangeTracker` and runs them all inside one
    /// transaction. On success, dispatches `BrowseTabInput::
    /// SaveCompleted` so the tab clears its tracker + refetches. On
    /// failure, the entire transaction has already been rolled back
    /// by the driver; we just surface the error message.
    pub(super) fn on_execute_browse_transaction(
        &mut self,
        tab_id: Uuid,
        save_id: Uuid,
        statements: Vec<(String, Vec<Value>)>,
        sources: Vec<StatementSource>,
        sender: ComponentSender<App>,
    ) {
        let (Some(conn), Some(tab_driver_id)) = (self.tab_connection(tab_id), self.tab_driver_id(tab_id)) else {
            self.finish_browse_save_failure(tab_id, save_id, bookie_app_services::tr!("No active connection"));
            return;
        };
        self.active_browse_saves.insert(tab_id, save_id);
        // Drivers that cannot report a row count for UPDATE / DELETE
        // return 0 for every statement, which the concurrency guard
        // below would read as "every row vanished". Skip the guard for
        // them rather than warn on every successful save.
        let reports_rows_affected = self
            .registry
            .get(&tab_driver_id)
            .is_none_or(|driver| driver.capabilities().reports_rows_affected);
        let checked_writes = checked_write_indexes(Some(&tab_driver_id), &sources);
        self.set_row_op_in_flight(true);
        // Increment the in-flight counter so window-close blocks until
        // the transaction resolves. Decrement happens in the
        // SaveCompletedForTab / SaveFailedForTab handlers regardless of
        // outcome.
        self.in_flight_saves.set(self.in_flight_saves.get() + 1);
        let sender_for_cmd = sender.clone();
        let mut recorded = self.ran_statements(tab_id, Source::Browse, &sql_without_parameters(&statements));
        let timeout_secs = self.tab_timeout_secs(tab_id);
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    let result = if checked_writes.is_empty() {
                        conn.execute_in_transaction_controlled(&statements, &control).await
                    } else {
                        conn.execute_in_transaction_checked_controlled(&statements, &checked_writes, &control)
                            .await
                    };
                    match result {
                        Ok(affected) => {
                            // Optimistic-concurrency guard: every UPDATE
                            // and DELETE in our materialised set targets
                            // a single PK-identified row, so each must
                            // affect exactly one row. Zero affected for
                            // any of them means the row was modified or
                            // deleted by another session between fetch
                            // and save. The transaction still committed
                            // — there's nothing to roll back — but the
                            // user must hear about it so a phantom save
                            // doesn't pass silently.
                            let warning = reports_rows_affected
                                .then(|| compute_concurrency_warning(&statements, &affected))
                                .flatten();
                            finish_if_recorded(&mut recorded, Outcome::Success).await;
                            sender_for_cmd.input(AppMsg::RowOpStarted);
                            sender_for_cmd.input(AppMsg::WorkspaceSchemaWordsChanged);
                            sender_for_cmd.input(AppMsg::SaveCompletedForTab(tab_id, save_id, warning));
                        }
                        Err(e) => {
                            // If the driver pinpointed which statement
                            // failed, look up its source and ask the
                            // tab to scroll-and-select that row before
                            // showing the error dialog. The user sees
                            // the row in question without scanning the
                            // whole grid.
                            let failed_statement = match &e {
                                DriverError::Transaction { statement_index, .. }
                                | DriverError::PartiallyApplied { statement_index, .. }
                                | DriverError::TransactionRollbackFailed { statement_index, .. } => {
                                    Some(*statement_index)
                                }
                                _ => None,
                            };
                            if let Some(source) = failed_statement.and_then(|index| sources.get(index).cloned()) {
                                sender_for_cmd.input(AppMsg::FlashErrorRowForTab(tab_id, source));
                            }
                            let msg = error_text::driver_message(&e);
                            finish_if_recorded(&mut recorded, Outcome::Error(msg.clone())).await;
                            sender_for_cmd.input(AppMsg::SaveFailedForTab(tab_id, save_id, msg));
                        }
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn take_active_browse_save(&mut self, tab_id: Uuid, save_id: Uuid) -> bool {
        if !take_save_id(&mut self.active_browse_saves, tab_id, save_id) {
            return false;
        }
        self.in_flight_saves.set(self.in_flight_saves.get().saturating_sub(1));
        true
    }

    pub(super) fn set_row_op_in_flight(&self, in_flight: bool) {
        self.row_op_spinner.set_visible(in_flight);
        if in_flight {
            self.row_op_spinner.start();
        } else {
            self.row_op_spinner.stop();
        }
    }

    pub(super) fn on_copy_row_as_statement(
        &self,
        tab_id: Uuid,
        row_positions: &[u32],
        statement: bookie_core::sql_literal::RowStatement,
    ) {
        if let Some(message) = copy_limit_refusal(row_positions.len()) {
            self.show_toast(&message);
            return;
        }
        let (columns, driver_id, rows, table, schema) = {
            let tabs = self.workspace_tabs.borrow();
            let Some(controller) = tabs.get(&tab_id).and_then(|t| t.browse_controller()) else {
                return;
            };
            let model = controller.model();
            (
                model.columns().to_vec(),
                model.driver_id().to_string(),
                // Reads the live grid row at this position (sort order,
                // any prepended drafts), not `snapshot().rows[row_position]`
                // -- that's fetch order and drifts by one row per pending
                // draft, copying a different row's values to the clipboard.
                row_positions
                    .iter()
                    .map(|position| model.row_cells_at(*position))
                    .collect::<Option<Vec<_>>>(),
                model.table().to_string(),
                model.schema().map(str::to_owned),
            )
        };
        let sql = match copied_statements(statement, &driver_id, schema.as_deref(), &table, &columns, rows) {
            Ok(sql) => sql,
            Err(message) => {
                self.show_toast(&message);
                return;
            }
        };
        self.window.clipboard().set_text(&sql);
        self.show_toast(&match statement {
            RowStatement::Insert => bookie_app_services::tr!("INSERT statement copied"),
            RowStatement::Update => bookie_app_services::tr!("UPDATE statement copied"),
            RowStatement::Delete => bookie_app_services::tr!("DELETE statement copied"),
            RowStatement::Where => bookie_app_services::tr!("WHERE clause copied"),
        });
    }

    pub(super) fn on_copy_to_clipboard(&self, text: String) {
        self.window.clipboard().set_text(&text);
        self.show_toast(&bookie_app_services::tr!("Copied to clipboard"));
    }
}

fn copy_limit_refusal(rows: usize) -> Option<String> {
    (rows > MAX_COPIED_STATEMENT_ROWS).then(|| bookie_app_services::tr!("Select fewer rows to copy as statements."))
}

fn copied_statements(
    statement: RowStatement,
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[bookie_core::ColumnInfo],
    rows: Option<Vec<Vec<Value>>>,
) -> Result<String, String> {
    let Some(rows) = rows else {
        return Err(bookie_app_services::tr!(
            "Some row values were not fetched. Show hidden columns and reload before copying or exporting."
        ));
    };
    statement
        .build_all(driver_id, schema, table, columns, &rows)
        .map_err(|error| error_text::build_sql_message(&error))
}

/// Inspect the SQL prefix of each statement and compare its expected
/// affected-row count against the driver's reported count. Returns
/// `Some(message)` if any UPDATE / DELETE matched zero rows.
///
/// `materialize` always emits one UPDATE / DELETE per PK-identified
/// row, so each is expected to affect exactly one row. Zero means the
/// target row was modified or deleted out-of-band between fetch and
/// save. INSERTs are ignored — auto-increment / generated-column rows
/// can plausibly affect zero according to driver quirks (e.g. ON
/// CONFLICT DO NOTHING in user-extended SQL), and we don't produce
/// those today.
fn compute_concurrency_warning(statements: &[(String, Vec<Value>)], affected: &[u64]) -> Option<String> {
    let mut zero_updates = 0usize;
    let mut zero_deletes = 0usize;
    for (idx, (sql, _)) in statements.iter().enumerate() {
        let count = affected.get(idx).copied().unwrap_or(0);
        if count > 0 {
            continue;
        }
        let trimmed = sql.trim_start().to_ascii_uppercase();
        if trimmed.starts_with("UPDATE") {
            zero_updates += 1;
        } else if trimmed.starts_with("DELETE") {
            zero_deletes += 1;
        }
    }
    if zero_updates == 0 && zero_deletes == 0 {
        return None;
    }
    let total = zero_updates + zero_deletes;
    Some(
        bookie_app_services::tr!(
            "{n} rows could not be located. They may have been changed by another session. Refresh and review."
        )
        .replace("{n}", &total.to_string()),
    )
}

/// Render a `Value` as a SQL literal — used by the "Copy row as
/// INSERT" clipboard helper to produce a self-contained statement
/// that round-trips through any SQL client.
#[cfg(test)]
mod tests {
    use super::{MAX_COPIED_STATEMENT_ROWS, compute_concurrency_warning, copied_statements, copy_limit_refusal};
    use bookie_core::sql_literal::RowStatement;
    use bookie_core::{ColumnInfo, Value};

    fn stmt(sql: &str) -> (String, Vec<bookie_core::Value>) {
        (sql.to_string(), Vec::new())
    }

    #[test]
    fn warning_none_when_all_match() {
        let stmts = vec![stmt("UPDATE \"t\" SET …"), stmt("DELETE FROM \"t\" …")];
        assert!(compute_concurrency_warning(&stmts, &[1, 1]).is_none());
    }

    #[test]
    fn warning_present_when_update_matches_zero() {
        let stmts = vec![stmt("UPDATE \"t\" SET …"), stmt("DELETE FROM \"t\" …")];
        let w = compute_concurrency_warning(&stmts, &[0, 1]).unwrap();
        assert!(w.contains("1 rows"));
    }

    #[test]
    fn warning_counts_update_and_delete_zero_together() {
        let stmts = vec![stmt("UPDATE a"), stmt("DELETE FROM b"), stmt("UPDATE c")];
        let w = compute_concurrency_warning(&stmts, &[0, 0, 1]).unwrap();
        assert!(w.contains("2 rows"));
    }

    #[test]
    fn warning_ignores_zero_inserts() {
        // INSERT with affected=0 is unusual but not necessarily a phantom
        // — keep the warning specific to UPDATE / DELETE for now.
        let stmts = vec![stmt("INSERT INTO t (a) VALUES (?)"), stmt("UPDATE t SET …")];
        assert!(compute_concurrency_warning(&stmts, &[0, 1]).is_none());
    }

    #[test]
    fn warning_handles_lowercase_and_whitespace() {
        let stmts = vec![stmt("  update t set x = 1 where id = 5")];
        assert!(compute_concurrency_warning(&stmts, &[0]).is_some());
    }
    fn column(name: &str, primary_key: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            primary_key,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    #[test]
    fn a_selection_over_the_row_limit_is_refused_and_one_at_the_limit_is_not() {
        assert!(copy_limit_refusal(MAX_COPIED_STATEMENT_ROWS).is_none());
        let message = copy_limit_refusal(MAX_COPIED_STATEMENT_ROWS + 1).unwrap();
        assert_eq!(message, "Select fewer rows to copy as statements.");
    }

    #[test]
    fn copied_update_and_where_keep_null_and_empty_text_apart() {
        let columns = vec![column("id", true), column("note", false)];
        let rows = vec![
            vec![Value::Int(1), Value::Text(String::new())],
            vec![Value::Int(2), Value::Null],
        ];
        let update =
            copied_statements(RowStatement::Update, "sqlite", None, "t", &columns, Some(rows.clone())).unwrap();
        assert_eq!(
            update,
            "UPDATE \"t\" SET \"note\" = '' WHERE \"id\" = 1;\nUPDATE \"t\" SET \"note\" = NULL WHERE \"id\" = 2;"
        );
        let predicate = copied_statements(RowStatement::Where, "sqlite", None, "t", &columns, Some(rows)).unwrap();
        assert_eq!(predicate, "(\"id\" = 1)\nOR (\"id\" = 2)");
    }

    #[test]
    fn a_selection_with_an_unfetched_row_copies_nothing_and_says_why() {
        let columns = vec![column("id", true)];
        let message = copied_statements(RowStatement::Update, "sqlite", None, "t", &columns, None).unwrap_err();
        assert!(message.starts_with("Some row values were not fetched"), "{message}");
    }
}

fn take_save_id(active: &mut std::collections::HashMap<Uuid, Uuid>, tab_id: Uuid, save_id: Uuid) -> bool {
    if active.get(&tab_id) != Some(&save_id) {
        return false;
    }
    active.remove(&tab_id);
    true
}

#[cfg(test)]
mod save_completion_tests {
    use super::take_save_id;
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn stale_completion_keeps_the_newer_save_active() {
        let tab_id = Uuid::new_v4();
        let old_save = Uuid::new_v4();
        let new_save = Uuid::new_v4();
        let mut active = HashMap::from([(tab_id, new_save)]);

        assert!(!take_save_id(&mut active, tab_id, old_save));
        assert_eq!(active.get(&tab_id), Some(&new_save));
        assert!(take_save_id(&mut active, tab_id, new_save));
        assert!(!take_save_id(&mut active, tab_id, new_save));
    }
}

fn checked_write_indexes(driver_id: Option<&str>, sources: &[StatementSource]) -> Vec<usize> {
    if !driver_id.is_some_and(|id| matches!(id, "postgres" | "mysql" | "mssql" | "sqlite")) {
        return Vec::new();
    }
    sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            matches!(source, StatementSource::Update { .. } | StatementSource::Delete { .. }).then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod checked_write_tests {
    use super::*;
    use crate::services::change_tracker::RowKey;

    fn sources() -> Vec<StatementSource> {
        vec![
            StatementSource::Insert { draft_id: 1 },
            StatementSource::Update {
                row_key: RowKey::Persisted(vec![]),
            },
            StatementSource::Delete {
                row_key: RowKey::Persisted(vec![]),
            },
        ]
    }

    #[test]
    fn keyed_updates_and_deletes_are_checked_and_inserts_are_not() {
        assert_eq!(checked_write_indexes(Some("postgres"), &sources()), vec![1, 2]);
    }

    #[test]
    fn engines_without_checked_transactions_check_nothing() {
        assert!(checked_write_indexes(Some("clickhouse"), &sources()).is_empty());
        assert!(checked_write_indexes(None, &sources()).is_empty());
    }
}
