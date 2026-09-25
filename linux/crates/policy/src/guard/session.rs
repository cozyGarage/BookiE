use tablepro_core::Session;

use super::*;
use crate::transaction_control::{TransactionControl, transaction_control};

const CLOSE_ROLLBACK_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct PolicySession {
    guard: PolicyGuard,
    inner: Box<dyn Session>,
    batch: Option<OpenBatch>,
}

#[derive(Clone, Copy)]
struct OpenBatch {
    id: Uuid,
    wrote: bool,
    uncertain: bool,
}

impl OpenBatch {
    fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            wrote: false,
            uncertain: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Finish {
    Commit,
    Rollback,
}

impl PolicyGuard {
    pub(super) fn wrap_session(&self, inner: Box<dyn Session>) -> Box<dyn Session> {
        Box::new(PolicySession {
            guard: self.clone(),
            inner,
            batch: None,
        })
    }
}

#[async_trait]
impl Session for PolicySession {
    async fn query_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        match transaction_control(sql, &self.guard.ctx.driver_id) {
            Some(TransactionControl::Begin) => self.begin(sql, control).await,
            Some(TransactionControl::Commit { chain }) => self.finish(sql, control, Finish::Commit, chain).await,
            Some(TransactionControl::Rollback { chain }) => self.finish(sql, control, Finish::Rollback, chain).await,
            None => self.statement(sql, params, control).await,
        }
    }

    fn is_usable(&self) -> bool {
        self.inner.is_usable()
    }

    fn transaction_open(&self) -> bool {
        self.batch.is_some()
    }

    async fn close(mut self: Box<Self>) -> Result<(), DriverError> {
        if let Some(batch) = self.batch {
            if self.inner.is_usable() && !batch.uncertain {
                let control = OperationControl::with_timeout(CLOSE_ROLLBACK_TIMEOUT);
                let _ = self.finish("ROLLBACK", &control, Finish::Rollback, false).await;
            } else if batch.wrote {
                record_abandoned_transaction(&self.guard, batch.id).await;
            }
        }
        self.inner.close().await
    }
}

impl PolicySession {
    async fn begin(&mut self, sql: &str, control: &OperationControl) -> Result<QueryResult, DriverError> {
        let result = self
            .guard
            .caught_write("BEGIN", self.inner.query_params_controlled(sql, &[], control))
            .await;
        if result.is_ok() && self.batch.is_none() {
            self.batch = Some(OpenBatch::new());
        }
        result
    }

    async fn statement(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        let facts = classify(sql, &self.guard.ctx.driver_id);
        let authorization = self.guard.authorize_classified(sql, facts, Some(control)).await?;
        let writes = authorization.facts.writes;
        let result = match self.batch {
            Some(batch) => {
                self.batch_statement(sql, params, control, authorization, batch.id)
                    .await
            }
            None if writes => self.lone_write(sql, params, control, authorization).await,
            None => self.lone_read(sql, params, control, authorization).await,
        };
        self.note_statement(writes, &result);
        result
    }

    fn note_statement(&mut self, writes: bool, result: &Result<QueryResult, DriverError>) {
        let Some(batch) = self.batch.as_mut() else {
            return;
        };
        batch.wrote |= writes;
        if matches!(
            result,
            Err(DriverError::Cancelled | DriverError::TimedOut | DriverError::OperationOutcomeUnknown { .. })
        ) {
            batch.uncertain = true;
        }
    }

    async fn batch_statement(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
        authorization: Authorization,
        batch_id: Uuid,
    ) -> Result<QueryResult, DriverError> {
        let writes = authorization.facts.writes;
        if writes {
            self.guard.require_governed_write_available()?;
        }
        let operation = self.guard.operation(
            sql,
            Some(batch_id),
            &authorization.facts,
            &authorization.decision,
            authorization.approval_outcome,
            authorization.preview_state,
        );
        let mut pending_write = if writes {
            self.guard
                .handle_intent_failure(self.guard.record_intent(&operation).await)?;
            Some(self.guard.ctx.audit_state.pending_write())
        } else {
            self.guard.prepare_governed_read(&operation).await?;
            None
        };
        let start = Instant::now();
        let execution = self.inner.query_params_controlled(sql, params, control);
        let result = match writes {
            true => self.guard.caught_write("SESSION QUERY", execution).await,
            false => self.guard.caught_read("SESSION QUERY", execution).await,
        }
        .map(|value| self.guard.mask_result_for_sql(Some(sql), value));
        let rows = result.as_ref().ok().map(|value| value.rows.len() as u64);
        self.guard
            .audit_transaction_result(&operation, start, &result, rows)
            .await?;
        if let Some(pending) = pending_write.as_mut() {
            pending.disarm();
        }
        result
    }

    async fn lone_write(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
        authorization: Authorization,
    ) -> Result<QueryResult, DriverError> {
        self.guard.require_governed_write_available()?;
        let operation = self.guard.operation(
            sql,
            None,
            &authorization.facts,
            &authorization.decision,
            authorization.approval_outcome,
            authorization.preview_state,
        );
        self.guard
            .handle_intent_failure(self.guard.record_intent(&operation).await)?;
        let mut pending_write = self.guard.ctx.audit_state.pending_write();
        let start = Instant::now();
        let result = self
            .guard
            .caught_write(
                "SESSION QUERY",
                self.inner.query_params_controlled(sql, params, control),
            )
            .await
            .map(|value| self.guard.mask_result_for_sql(Some(sql), value));
        let rows = result.as_ref().ok().map(|value| value.rows.len() as u64);
        self.guard.audit_write_result(&operation, start, &result, rows).await?;
        pending_write.disarm();
        result
    }

    async fn lone_read(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
        authorization: Authorization,
    ) -> Result<QueryResult, DriverError> {
        let operation = self.guard.operation(
            sql,
            None,
            &authorization.facts,
            &authorization.decision,
            authorization.approval_outcome,
            authorization.preview_state,
        );
        self.guard.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .guard
            .caught_read(
                "SESSION QUERY",
                self.inner.query_params_controlled(sql, params, control),
            )
            .await
            .map(|value| self.guard.mask_result_for_sql(Some(sql), value));
        let rows = result.as_ref().ok().map(|value| value.rows.len() as u64);
        self.guard
            .audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }
}

impl PolicySession {
    async fn finish(
        &mut self,
        sql: &str,
        control: &OperationControl,
        kind: Finish,
        chain: bool,
    ) -> Result<QueryResult, DriverError> {
        let Some(batch) = self.batch else {
            return self
                .guard
                .caught_read("SESSION QUERY", self.inner.query_params_controlled(sql, &[], control))
                .await;
        };
        if kind == Finish::Commit {
            self.guard.require_governed_write_available()?;
        }
        let operation = finish_operation(kind, batch.id);
        self.guard.record_intent(&operation).await.map_err(|error| {
            DriverError::PolicyDenied(format!(
                "{} denied because audit intent could not be persisted: {error}",
                operation.sql
            ))
        })?;
        let mut pending_write = self.guard.ctx.audit_state.pending_write();
        let start = Instant::now();
        let result = self
            .guard
            .caught_write(operation.sql, self.inner.query_params_controlled(sql, &[], control))
            .await;
        let (terminal_status, transaction_outcome, error_category, ambiguous) = finish_outcome(kind, &result);
        if ambiguous {
            self.guard.ctx.audit_state.disable_governed_writes();
        }
        let audit_result = self
            .guard
            .record_outcome(
                &operation,
                AuditOutcome {
                    terminal_status,
                    transaction_outcome,
                    rows_affected: None,
                    error_category,
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            )
            .await;
        self.batch = if result.is_ok() {
            chain.then(OpenBatch::new)
        } else {
            Some(OpenBatch {
                uncertain: batch.uncertain || ambiguous,
                ..batch
            })
        };
        self.guard.handle_write_outcome_failure(audit_result)?;
        pending_write.disarm();
        result
    }
}

async fn record_abandoned_transaction(guard: &PolicyGuard, batch_id: Uuid) {
    guard.ctx.audit_state.disable_governed_writes();
    let operation = finish_operation(Finish::Rollback, batch_id);
    let _ = guard.record_intent(&operation).await;
    let outcome = AuditOutcome {
        terminal_status: AuditTerminalStatus::Unknown,
        transaction_outcome: AuditTransactionOutcome::Unknown,
        rows_affected: None,
        error_category: None,
        duration_ms: 0,
    };
    let _ = guard.handle_write_outcome_failure(guard.record_outcome(&operation, outcome).await);
}

fn finish_operation(kind: Finish, batch_id: Uuid) -> AuditOperation<'static> {
    match kind {
        Finish::Commit => PolicyGuard::commit_operation(batch_id),
        Finish::Rollback => AuditOperation {
            operation_id: Uuid::new_v4(),
            batch_id: Some(batch_id),
            sql: "ROLLBACK",
            class: AuditOperationClass::TransactionRollback,
            targets: Vec::new(),
            decision_rule: "transaction_rollback".into(),
            approval_outcome: AuditApprovalOutcome::NotRequired,
            preview_state: AuditPreviewState::NotRequested,
        },
    }
}

fn finish_outcome(
    kind: Finish,
    result: &Result<QueryResult, DriverError>,
) -> (
    AuditTerminalStatus,
    AuditTransactionOutcome,
    Option<AuditErrorCategory>,
    bool,
) {
    match result {
        Ok(_) => {
            let outcome = match kind {
                Finish::Commit => AuditTransactionOutcome::Committed,
                Finish::Rollback => AuditTransactionOutcome::RolledBack,
            };
            (AuditTerminalStatus::Succeeded, outcome, None, false)
        }
        Err(error) if is_ambiguous_post_dispatch(error) => (
            AuditTerminalStatus::Unknown,
            AuditTransactionOutcome::Unknown,
            Some(error_category(error)),
            true,
        ),
        Err(error) => (
            AuditTerminalStatus::Failed,
            AuditTransactionOutcome::Failed,
            Some(error_category(error)),
            false,
        ),
    }
}
