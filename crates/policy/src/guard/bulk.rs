use super::*;
use crate::classify::StatementClass;
use crate::classify::classify_parsed;
use crate::parsed_sql::ParsedSql;
use sqlparser::ast::{OnConflict, OnConflictAction, OnInsert, Statement, TableObject};

/// Rows one approved import may write. A budget larger than this is
/// refused before the user is asked, so no single approval can cover an
/// unbounded amount of work.
pub const MAX_BULK_ROW_BUDGET: u64 = 1_000_000;

pub const MAX_SKIPPED_ROWS: u64 = 1_000;

const MAX_SKIP_REASON_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedRow {
    pub index: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkBatchOutcome {
    pub written: u64,
    pub skipped: Vec<SkippedRow>,
}

/// What the user is asked to approve: one INSERT shape, one table, one
/// connection and a fixed number of rows.
#[derive(Debug, Clone)]
pub struct BulkInsertRequest {
    pub schema: Option<String>,
    pub table: String,
    pub statement: String,
    pub row_budget: u64,
    pub sequence_columns: Vec<String>,
}

/// One bounded batch offered against an open scope. Every field is
/// checked against what the scope was granted for; a mismatch is denied.
#[derive(Debug, Clone, Copy)]
pub struct BulkBatch<'a> {
    pub schema: Option<&'a str>,
    pub table: &'a str,
    pub statement: &'a str,
    pub rows: &'a [Vec<Value>],
}

/// How an import stopped. Every import ends through exactly one of these.
#[derive(Debug, Clone, Copy)]
pub enum BulkInsertEnd<'a> {
    Completed,
    Cancelled,
    Failed(&'a DriverError),
}

/// One approval, granted once and spendable only on the connection,
/// table, statement and row count it names.
///
/// The scope is not a flag that turns policy off. It carries the identity
/// of what was approved, and every batch is checked against it. It cannot
/// be cloned or copied, it is ended by
/// [`PolicyGuard::finish_bulk_insert`], and dropping it without finishing
/// disables governed writes the same way a write whose outcome was never
/// recorded does.
pub struct BulkInsertScope {
    connection_id: Uuid,
    operation_id: Uuid,
    batch_id: Uuid,
    schema: Option<String>,
    table: String,
    statement: String,
    target: String,
    class: AuditOperationClass,
    decision_rule: String,
    approval_outcome: AuditApprovalOutcome,
    preview_state: AuditPreviewState,
    row_budget: u64,
    sequence_columns: Vec<String>,
    committed: u64,
    skipped: u64,
    open: bool,
    started: Instant,
    audit_state: Arc<AuditState>,
}

struct GrantedBulkDecision {
    decision_rule: String,
    approval_outcome: AuditApprovalOutcome,
    preview_state: AuditPreviewState,
}

impl BulkInsertScope {
    pub fn committed_rows(&self) -> u64 {
        self.committed
    }

    pub fn skipped_rows(&self) -> u64 {
        self.skipped
    }

    pub fn row_budget(&self) -> u64 {
        self.row_budget
    }

    pub fn remaining_rows(&self) -> u64 {
        self.row_budget.saturating_sub(self.committed)
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn operation(&self) -> AuditOperation<'_> {
        AuditOperation {
            operation_id: self.operation_id,
            batch_id: Some(self.batch_id),
            sql: &self.statement,
            class: self.class,
            targets: vec![self.target.clone()],
            decision_rule: self.decision_rule.clone(),
            approval_outcome: self.approval_outcome,
            preview_state: self.preview_state.clone(),
        }
    }

    fn admit(&mut self, connection_id: Uuid, batch: &BulkBatch<'_>) -> Result<(), DriverError> {
        let verdict = self.verdict(connection_id, batch);
        if let Err(error) = verdict {
            self.open = false;
            return Err(error);
        }
        Ok(())
    }

    fn verdict(&self, connection_id: Uuid, batch: &BulkBatch<'_>) -> Result<(), DriverError> {
        if !self.open {
            return Err(denied("the import approval has already been spent or ended"));
        }
        if connection_id != self.connection_id {
            return Err(denied("the import approval was granted for another connection"));
        }
        if batch.table != self.table || batch.schema != self.schema.as_deref() {
            return Err(denied("the import approval was granted for another table"));
        }
        if batch.statement != self.statement {
            return Err(denied("the import approval was granted for another statement"));
        }
        let rows = batch.rows.len() as u64;
        if rows > self.remaining_rows() {
            return Err(denied("the import would write more rows than were approved"));
        }
        Ok(())
    }
}

/// A scope that reaches its drop still open means an import stopped
/// without a terminal audit record. That is the same hole an abandoned
/// write leaves, and it closes the same way.
impl Drop for BulkInsertScope {
    fn drop(&mut self) {
        if !self.open {
            return;
        }
        tracing::error!("an import ended without a terminal audit record; governed writes are disabled");
        self.audit_state.disable_governed_writes();
    }
}

fn rejected_row(error: &DriverError) -> Option<usize> {
    match error {
        DriverError::Transaction {
            statement_index,
            source,
        } if matches!(**source, DriverError::Query { .. }) => Some(*statement_index),
        _ => None,
    }
}

fn skip_reason(error: &DriverError) -> String {
    let DriverError::Transaction { source, .. } = error else {
        return String::new();
    };
    let text = match &**source {
        DriverError::Query { message, .. } => message.as_str(),
        _ => "the database rejected the row",
    };
    let line = text.lines().next().unwrap_or_default();
    let cleaned: String = line
        .chars()
        .map(|character| if character.is_control() { ' ' } else { character })
        .take(MAX_SKIP_REASON_CHARS)
        .collect();
    if cleaned.trim().is_empty() {
        "the database rejected the row".to_owned()
    } else {
        cleaned
    }
}

fn too_many_skipped_rows() -> DriverError {
    DriverError::Query {
        message: format!("more than {MAX_SKIPPED_ROWS} rows were rejected by the database; the import stopped"),
        sqlstate: None,
        position: None,
    }
}

fn denied(message: &str) -> DriverError {
    DriverError::PolicyDenied(message.to_owned())
}

fn qualified_target(schema: Option<&str>, table: &str) -> String {
    schema.map_or_else(|| table.to_owned(), |schema| format!("{schema}.{table}"))
}

fn valid_sequence_columns(driver_id: &str, columns: &[String]) -> bool {
    columns.len() <= bookie_core::import::MAX_COLUMNS
        && (columns.is_empty() || driver_id.eq_ignore_ascii_case("postgres"))
        && columns.iter().enumerate().all(|(index, column)| {
            bookie_core::sql_dialect::validate_ident(column).is_ok() && !columns[..index].contains(column)
        })
}

fn approval_preview(target: &str, rows: u64, statement: &str, upserts: bool) -> String {
    let notice = if upserts { UPSERT_NOTICE } else { "" };
    format!(
        "Import {rows} rows into {target}{notice}\n\nEvery row runs this statement with its own bound values:\n{statement}"
    )
}

const UPSERT_REASON: &str = " (existing rows may be updated)";

const UPSERT_AUDIT_SUFFIX: &str = " (upsert: existing rows may be updated)";

const UPSERT_NOTICE: &str = ". Rows whose key already exists are updated, so existing rows may change";

fn upserts_existing_rows(parsed: &ParsedSql) -> bool {
    let Ok([Statement::Insert(insert)]) = parsed.statements() else {
        return false;
    };
    matches!(
        insert.on,
        Some(OnInsert::DuplicateKeyUpdate(_))
            | Some(OnInsert::OnConflict(OnConflict {
                action: OnConflictAction::DoUpdate(_),
                ..
            }))
    )
}

impl PolicyGuard {
    /// Classify one INSERT shape, decide it once, ask for one approval when
    /// the policy requires it, and record one intent. The returned scope is
    /// the only thing that lets [`PolicyGuard::execute_bulk_batch`] run.
    pub async fn begin_bulk_insert(&self, request: BulkInsertRequest) -> Result<BulkInsertScope, DriverError> {
        self.require_governed_write_available()?;
        let parsed = ParsedSql::new(&request.statement, &self.ctx.driver_id);
        let facts = bulk_facts(&request, &parsed, &self.ctx.driver_id)?;
        if !valid_sequence_columns(&self.ctx.driver_id, &request.sequence_columns) {
            self.record_bulk_refusal(
                &request,
                &facts,
                "invalid_import_sequence_columns",
                AuditApprovalOutcome::NotRequired,
                None,
            )
            .await?;
            return Err(denied("the import sequence columns are invalid"));
        }
        if !statement_target_matches_request(&request, &parsed, &self.ctx.driver_id) {
            self.record_bulk_refusal(
                &request,
                &facts,
                "bulk_target_mismatch",
                AuditApprovalOutcome::NotRequired,
                None,
            )
            .await?;
            return Err(denied("the INSERT target does not match the approved import target"));
        }
        let env_policy = self
            .ctx
            .policy
            .for_connection(&self.ctx.connection_id.to_string(), self.ctx.environment);
        let decision = evaluate_with_effects(
            &self.ctx.principal,
            self.ctx.environment,
            &facts,
            self.ctx.read_only,
            &env_policy,
            Some(request.row_budget),
        );
        let upserts = upserts_existing_rows(&parsed);
        let granted = self.settle_bulk_decision(&request, &facts, decision, upserts).await?;
        let scope = self.new_scope(request, &facts, granted, upserts);
        self.handle_intent_failure(self.record_intent(&scope.operation()).await)?;
        Ok(scope)
    }

    /// Run one bounded batch under an open scope. The batch must name the
    /// connection, table and statement the scope was granted for, and must
    /// fit in what is left of its row budget.
    pub async fn execute_bulk_batch(
        &self,
        scope: &mut BulkInsertScope,
        batch: BulkBatch<'_>,
        control: &OperationControl,
    ) -> Result<u64, DriverError> {
        scope.admit(self.ctx.connection_id, &batch)?;
        if let Err(error) = self.require_governed_write_available().and(check_pre_dispatch(control)) {
            scope.open = false;
            return Err(error);
        }
        if batch.rows.is_empty() {
            return Ok(0);
        }
        let statements: Vec<(String, Vec<Value>)> = batch
            .rows
            .iter()
            .map(|row| (scope.statement.clone(), row.clone()))
            .collect();
        let result = self
            .caught_write(
                "BULK INSERT BATCH",
                self.inner.execute_in_transaction_controlled(&statements, control),
            )
            .await;
        if let Err(error) = result {
            scope.open = false;
            return Err(error);
        }
        let rows = statements.len() as u64;
        scope.committed += rows;
        self.synchronize_import_sequences(scope).await?;
        Ok(rows)
    }

    pub async fn execute_bulk_batch_skipping_failed_rows(
        &self,
        scope: &mut BulkInsertScope,
        batch: BulkBatch<'_>,
        control: &OperationControl,
    ) -> Result<BulkBatchOutcome, DriverError> {
        scope.admit(self.ctx.connection_id, &batch)?;
        if let Err(error) = self.require_governed_write_available().and(check_pre_dispatch(control)) {
            scope.open = false;
            return Err(error);
        }
        let mut remaining: Vec<(usize, &Vec<Value>)> = batch.rows.iter().enumerate().collect();
        let mut skipped: Vec<SkippedRow> = Vec::new();
        while !remaining.is_empty() {
            let statements: Vec<(String, Vec<Value>)> = remaining
                .iter()
                .map(|(_, row)| (scope.statement.clone(), (*row).clone()))
                .collect();
            let result = self
                .caught_write(
                    "BULK INSERT BATCH",
                    self.inner.execute_in_transaction_controlled(&statements, control),
                )
                .await;
            let Err(error) = result else { break };
            let rejected = rejected_row(&error).filter(|at| *at < remaining.len());
            let Some(at) = rejected else {
                scope.skipped += skipped.len() as u64;
                scope.open = false;
                return Err(error);
            };
            if scope.skipped + skipped.len() as u64 >= MAX_SKIPPED_ROWS {
                scope.skipped += skipped.len() as u64;
                scope.open = false;
                return Err(too_many_skipped_rows());
            }
            let (index, _) = remaining.remove(at);
            skipped.push(SkippedRow {
                index,
                reason: skip_reason(&error),
            });
        }
        let written = remaining.len() as u64;
        scope.committed += written;
        scope.skipped += skipped.len() as u64;
        self.synchronize_import_sequences(scope).await?;
        Ok(BulkBatchOutcome { written, skipped })
    }

    async fn synchronize_import_sequences(&self, scope: &mut BulkInsertScope) -> Result<(), DriverError> {
        if scope.sequence_columns.is_empty() {
            return Ok(());
        }
        let repair_control = OperationControl::with_timeout(std::time::Duration::from_secs(5));
        self.caught_write(
            "IMPORT SEQUENCE SYNC",
            self.inner.synchronize_auto_increment_sequences_controlled(
                scope.schema.as_deref(),
                &scope.table,
                &scope.sequence_columns,
                &repair_control,
            ),
        )
        .await
    }

    /// End the import and write its single terminal audit record, carrying
    /// the rows that actually committed.
    pub async fn finish_bulk_insert(
        &self,
        scope: &mut BulkInsertScope,
        end: BulkInsertEnd<'_>,
    ) -> Result<u64, DriverError> {
        scope.open = false;
        let committed = scope.committed;
        let (terminal_status, transaction_outcome, error_category) = bulk_terminal(end, committed);
        if terminal_status == AuditTerminalStatus::Unknown {
            self.ctx.audit_state.disable_governed_writes();
        }
        let rows_skipped = (scope.skipped > 0).then_some(scope.skipped);
        if let Some(count) = rows_skipped {
            tracing::warn!(rows_skipped = count, "an import skipped rows the database rejected");
        }
        let audit_result = self
            .record(
                &scope.operation(),
                AuditRecordPhase::Outcome,
                RecordFields {
                    terminal_status,
                    transaction_outcome,
                    rows_affected: Some(committed),
                    error_category,
                    duration_ms: Some(scope.started.elapsed().as_millis() as u64),
                    rows_skipped,
                },
            )
            .await;
        self.handle_write_outcome_failure(audit_result)?;
        Ok(committed)
    }

    async fn settle_bulk_decision(
        &self,
        request: &BulkInsertRequest,
        facts: &StatementFacts,
        decision: Decision,
        upserts: bool,
    ) -> Result<GrantedBulkDecision, DriverError> {
        let target = qualified_target(request.schema.as_deref(), &request.table);
        match decision {
            Decision::Allow { rule } => Ok(GrantedBulkDecision {
                decision_rule: rule,
                approval_outcome: AuditApprovalOutcome::NotRequired,
                preview_state: AuditPreviewState::NotRequested,
            }),
            Decision::Deny { ref rule, ref message } => {
                self.record_bulk_refusal(request, facts, rule, AuditApprovalOutcome::NotRequired, None)
                    .await?;
                Err(denied(message))
            }
            Decision::RequireApproval { ref rule, .. } => {
                let preview = approval_preview(&target, request.row_budget, &request.statement, upserts);
                let outcome = self
                    .ctx
                    .approval
                    .request(ApprovalRequest {
                        principal: self.ctx.principal.clone(),
                        environment: self.ctx.environment,
                        connection_id: self.ctx.connection_id,
                        connection_name: self.ctx.connection_name.clone(),
                        sql: request.statement.clone(),
                        facts: facts.clone(),
                        rule: rule.clone(),
                        reason: format!(
                            "import {} rows into {target}{}",
                            request.row_budget,
                            if upserts { UPSERT_REASON } else { "" }
                        ),
                        preview: Some(preview.clone()),
                        estimated_rows: Some(request.row_budget),
                        cancellation: None,
                    })
                    .await;
                if outcome == ApprovalOutcome::Deny {
                    self.record_bulk_refusal(request, facts, rule, AuditApprovalOutcome::Denied, Some(preview))
                        .await?;
                    return Err(denied("the import was not approved"));
                }
                Ok(GrantedBulkDecision {
                    decision_rule: format!("{rule}:approved"),
                    approval_outcome: super::approved_audit_outcome(outcome),
                    preview_state: AuditPreviewState::Available(preview),
                })
            }
        }
    }

    async fn record_bulk_refusal(
        &self,
        request: &BulkInsertRequest,
        facts: &StatementFacts,
        rule: &str,
        approval_outcome: AuditApprovalOutcome,
        preview: Option<String>,
    ) -> Result<(), DriverError> {
        let operation = AuditOperation {
            operation_id: Uuid::new_v4(),
            batch_id: None,
            sql: &request.statement,
            class: AuditOperationClass::from_statement(facts.class, facts.writes()),
            targets: facts.tables.clone(),
            decision_rule: rule.to_owned(),
            approval_outcome,
            preview_state: preview.map_or(AuditPreviewState::NotRequested, AuditPreviewState::Available),
        };
        let audit_result = self
            .record_outcome(
                &operation,
                AuditOutcome {
                    terminal_status: AuditTerminalStatus::Denied,
                    transaction_outcome: AuditTransactionOutcome::NotApplicable,
                    rows_affected: None,
                    error_category: Some(AuditErrorCategory::Policy),
                    duration_ms: 0,
                },
            )
            .await;
        self.handle_non_execution_audit_failure(audit_result)
    }

    fn new_scope(
        &self,
        request: BulkInsertRequest,
        facts: &StatementFacts,
        granted: GrantedBulkDecision,
        upserts: bool,
    ) -> BulkInsertScope {
        let target = qualified_target(request.schema.as_deref(), &request.table);
        BulkInsertScope {
            connection_id: self.ctx.connection_id,
            operation_id: Uuid::new_v4(),
            batch_id: Uuid::new_v4(),
            target: if upserts {
                format!("{target}{UPSERT_AUDIT_SUFFIX}")
            } else {
                target
            },
            schema: request.schema,
            table: request.table,
            statement: request.statement,
            class: AuditOperationClass::from_statement(facts.class, facts.writes()),
            decision_rule: granted.decision_rule,
            approval_outcome: granted.approval_outcome,
            preview_state: granted.preview_state,
            row_budget: request.row_budget,
            sequence_columns: request.sequence_columns,
            committed: 0,
            skipped: 0,
            open: true,
            started: Instant::now(),
            audit_state: self.ctx.audit_state.clone(),
        }
    }
}

pub(super) fn bulk_terminal(
    end: BulkInsertEnd<'_>,
    committed: u64,
) -> (AuditTerminalStatus, AuditTransactionOutcome, Option<AuditErrorCategory>) {
    match end {
        BulkInsertEnd::Completed => (AuditTerminalStatus::Succeeded, AuditTransactionOutcome::Committed, None),
        BulkInsertEnd::Cancelled => (
            AuditTerminalStatus::Cancelled,
            partial_transaction_outcome(committed),
            Some(AuditErrorCategory::Cancelled),
        ),
        BulkInsertEnd::Failed(error) if is_ambiguous_post_dispatch(error) => (
            AuditTerminalStatus::Unknown,
            AuditTransactionOutcome::Unknown,
            Some(error_category(error)),
        ),
        BulkInsertEnd::Failed(error @ DriverError::PartiallyApplied { .. }) => (
            controlled_error_terminal_status(error),
            AuditTransactionOutcome::Failed,
            Some(error_category(error)),
        ),
        BulkInsertEnd::Failed(error) => (
            controlled_error_terminal_status(error),
            partial_transaction_outcome(committed),
            Some(error_category(error)),
        ),
    }
}

/// An import commits batch by batch, so a stopped import leaves the
/// batches that already committed in place. Reporting the whole import as
/// rolled back would hide them.
fn partial_transaction_outcome(committed: u64) -> AuditTransactionOutcome {
    if committed == 0 {
        return AuditTransactionOutcome::RolledBack;
    }
    AuditTransactionOutcome::Committed
}

fn bulk_facts(request: &BulkInsertRequest, parsed: &ParsedSql, driver_id: &str) -> Result<StatementFacts, DriverError> {
    if request.table.trim().is_empty() {
        return Err(denied("an import needs a target table"));
    }
    if request.row_budget == 0 || request.row_budget > MAX_BULK_ROW_BUDGET {
        return Err(denied("an import must name between one row and the import row limit"));
    }
    let facts = classify_parsed(parsed, driver_id);
    if facts.is_multi_statement || facts.class != StatementClass::Insert || !facts.writes() {
        return Err(denied(
            "an import approval covers one INSERT statement and nothing else",
        ));
    }
    Ok(facts)
}

fn statement_target_matches_request(request: &BulkInsertRequest, parsed: &ParsedSql, driver_id: &str) -> bool {
    let Ok(statements) = parsed.statements() else {
        return false;
    };
    let [Statement::Insert(insert)] = statements else {
        return false;
    };
    let TableObject::TableName(name) = &insert.table else {
        return false;
    };
    let mut expected = Vec::with_capacity(2);
    if let Some(schema) = request.schema.as_deref() {
        expected.push(schema);
    }
    expected.push(request.table.as_str());
    name.0.len() == expected.len()
        && name.0.iter().zip(expected).all(|(part, requested)| {
            let Some(identifier) = part.as_ident() else {
                return false;
            };
            let actual = if identifier.quote_style.is_none() && driver_id == "postgres" {
                identifier.value.to_ascii_lowercase()
            } else {
                identifier.value.clone()
            };
            actual == requested
        })
}
