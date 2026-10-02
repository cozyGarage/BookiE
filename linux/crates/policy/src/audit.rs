use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tablepro_core::Environment;
use thiserror::Error;
use uuid::Uuid;

use crate::classify::StatementClass;
use crate::principal::Principal;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuditError {
    #[error("audit sink is unavailable: {0}")]
    Unavailable(String),
    #[error("audit record could not be persisted: {0}")]
    Persistence(String),
    #[error("audit journal is corrupt: {0}")]
    Corrupt(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditRecordPhase {
    Intent,
    Outcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOperationClass {
    Read,
    Mutation,
    Ddl,
    Administrative,
    TransactionCommit,
    TransactionRollback,
    UnknownWrite,
}

impl AuditOperationClass {
    pub fn from_statement(class: StatementClass, writes: bool) -> Self {
        match class {
            StatementClass::Select if !writes => Self::Read,
            StatementClass::Insert | StatementClass::Update | StatementClass::Delete => Self::Mutation,
            StatementClass::Ddl => Self::Ddl,
            StatementClass::Administrative => Self::Administrative,
            StatementClass::Transaction if !writes => Self::Read,
            StatementClass::Other | StatementClass::Unparseable if writes => Self::UnknownWrite,
            _ if writes => Self::UnknownWrite,
            _ => Self::Read,
        }
    }

    pub fn is_write(self) -> bool {
        !matches!(self, Self::Read)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditApprovalOutcome {
    NotRequired,
    Approved,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "detail")]
pub enum AuditPreviewState {
    NotRequested,
    Available(String),
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditTerminalStatus {
    Pending,
    Succeeded,
    Failed,
    Denied,
    Cancelled,
    TimedOut,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditTransactionOutcome {
    NotApplicable,
    Pending,
    Committed,
    RolledBack,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditErrorCategory {
    Policy,
    Audit,
    Connection,
    Authentication,
    Tls,
    Query,
    ReadOnly,
    Unsupported,
    Internal,
    Transaction,
    Cancelled,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub operation_id: Uuid,
    pub batch_id: Option<Uuid>,
    pub phase: AuditRecordPhase,
    pub principal: Principal,
    pub connection_id: Uuid,
    pub connection_name: String,
    pub environment: Environment,
    pub driver_id: String,
    pub operation_class: AuditOperationClass,
    pub redacted_sql: String,
    pub sql_hash: String,
    pub targets: Vec<String>,
    pub decision_rule: String,
    pub approval_outcome: AuditApprovalOutcome,
    pub preview_state: AuditPreviewState,
    pub terminal_status: AuditTerminalStatus,
    pub transaction_outcome: AuditTransactionOutcome,
    pub error_category: Option<AuditErrorCategory>,
    pub error: Option<String>,
    pub rows_affected: Option<u64>,
    pub duration_ms: Option<u64>,
}

#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError>;
}

pub struct NullAuditSink;

#[async_trait]
impl AuditSink for NullAuditSink {
    async fn record(&self, _event: AuditEvent) -> Result<(), AuditError> {
        Err(AuditError::Unavailable("no audit sink configured".into()))
    }
}

#[derive(Debug, Default)]
pub struct AuditState {
    globally_disabled: std::sync::Arc<AtomicBool>,
    connection_uncertain: AtomicBool,
}

impl AuditState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_governed_writes_disabled() -> Self {
        let state = Self::new();
        state.disable_globally();
        state
    }

    pub fn governed_writes_disabled(&self) -> bool {
        self.globally_disabled.load(Ordering::Acquire) || self.connection_uncertain.load(Ordering::Acquire)
    }

    pub(crate) fn disable_governed_writes(&self) {
        self.connection_uncertain.store(true, Ordering::Release);
    }

    pub(crate) fn disable_globally(&self) {
        self.globally_disabled.store(true, Ordering::Release);
    }

    /// Share journal-wide failure state with a clean connection generation.
    /// Call only after establishing a replacement connection; old guards keep
    /// their original state and cannot disable the replacement later.
    pub fn new_connection_generation(&self) -> Self {
        Self {
            globally_disabled: self.globally_disabled.clone(),
            connection_uncertain: AtomicBool::new(false),
        }
    }

    pub(crate) fn pending_write(&self) -> PendingWrite<'_> {
        PendingWrite {
            state: self,
            armed: true,
        }
    }
}

pub(crate) struct PendingWrite<'a> {
    state: &'a AuditState,
    armed: bool,
}

impl PendingWrite<'_> {
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PendingWrite<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.state.disable_governed_writes();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncertainty_is_scoped_to_a_connection_generation_but_journal_failure_is_global() {
        let journal = AuditState::new();
        let connection_a = journal.new_connection_generation();
        let connection_b = journal.new_connection_generation();

        connection_a.disable_governed_writes();
        assert!(connection_a.governed_writes_disabled());
        assert!(!connection_b.governed_writes_disabled());

        let replacement_a = connection_a.new_connection_generation();
        connection_a.disable_governed_writes(); // A late callback from the retired generation.
        assert!(!replacement_a.governed_writes_disabled());

        journal.disable_globally();
        assert!(connection_b.governed_writes_disabled());
        assert!(replacement_a.governed_writes_disabled());
    }
}
