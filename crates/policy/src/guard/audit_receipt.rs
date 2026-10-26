use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tablepro_core::{
    Connection, DriverError, OperationControl, QueryCursorId, QueryCursorPage, QueryResult, QueryResultBatch, Session,
    SessionCommitOutcome, Value,
};
use uuid::Uuid;

use super::PolicyGuard;
use crate::audit::{AuditError, AuditEvent, AuditSink};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuditReceipt {
    pub operation_id: Uuid,
}

pub struct Audited<T> {
    pub result: Result<T, DriverError>,
    pub audit_receipts: Vec<AuditReceipt>,
}

struct CaptureState {
    operation_ids: Mutex<Vec<Uuid>>,
}

struct CapturingAuditSink {
    inner: Arc<dyn AuditSink>,
    active: Mutex<Option<Arc<CaptureState>>>,
}

impl CapturingAuditSink {
    fn new(inner: Arc<dyn AuditSink>) -> Self {
        Self {
            inner,
            active: Mutex::new(None),
        }
    }

    fn begin(&self) -> AuditCaptureScope<'_> {
        let state = Arc::new(CaptureState {
            operation_ids: Mutex::new(Vec::new()),
        });
        if let Ok(mut active) = self.active.lock() {
            *active = Some(state.clone());
        }
        AuditCaptureScope { sink: self, state }
    }
}

struct AuditCaptureScope<'a> {
    sink: &'a CapturingAuditSink,
    state: Arc<CaptureState>,
}

impl AuditCaptureScope<'_> {
    fn receipts(&self) -> Vec<AuditReceipt> {
        self.state
            .operation_ids
            .lock()
            .map(|operation_ids| {
                operation_ids
                    .iter()
                    .copied()
                    .map(|operation_id| AuditReceipt { operation_id })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for AuditCaptureScope<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.sink.active.lock()
            && active.as_ref().is_some_and(|state| Arc::ptr_eq(state, &self.state))
        {
            *active = None;
        }
    }
}

#[async_trait]
impl AuditSink for CapturingAuditSink {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        let capture = self.active.lock().ok().and_then(|active| active.as_ref().cloned());
        let operation_id = event.operation_id;
        self.inner.record(event).await?;
        if let Some(capture) = capture
            && let Ok(mut operation_ids) = capture.operation_ids.lock()
            && !operation_ids.contains(&operation_id)
        {
            operation_ids.push(operation_id);
        }
        Ok(())
    }
}

pub struct AuditedSession {
    inner: Box<dyn Session>,
    capture: Arc<CapturingAuditSink>,
    cursor: Option<QueryCursorId>,
}

impl AuditedSession {
    pub async fn query_params_controlled_with_audit(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Audited<QueryResult> {
        let capture = self.capture.begin();
        let result = self.inner.query_params_controlled(sql, params, control).await;
        Audited {
            result,
            audit_receipts: capture.receipts(),
        }
    }

    pub async fn query_result_sets_params_controlled_with_audit(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Audited<QueryResultBatch> {
        let capture = self.capture.begin();
        let result = self
            .inner
            .query_result_sets_params_controlled(sql, params, control)
            .await;
        Audited {
            result,
            audit_receipts: capture.receipts(),
        }
    }

    pub async fn commit_controlled_with_audit(&mut self, control: &OperationControl) -> Audited<SessionCommitOutcome> {
        let capture = self.capture.begin();
        let result = self.inner.commit_controlled(control).await;
        Audited {
            result,
            audit_receipts: capture.receipts(),
        }
    }

    pub async fn open_query_cursor_controlled_with_audit(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Audited<QueryCursorId> {
        let capture = self.capture.begin();
        let result = self.inner.open_query_cursor_controlled(sql, params, control).await;
        if let Ok(cursor_id) = &result {
            self.cursor = Some(*cursor_id);
        }
        Audited {
            result,
            audit_receipts: capture.receipts(),
        }
    }

    pub async fn fetch_query_cursor_page_controlled_with_audit(
        &mut self,
        cursor_id: QueryCursorId,
        control: &OperationControl,
    ) -> Audited<QueryCursorPage> {
        let capture = self
            .cursor
            .filter(|active_id| *active_id == cursor_id)
            .map(|_| self.capture.begin());
        let result = self.inner.fetch_query_cursor_page_controlled(cursor_id, control).await;
        Audited {
            result,
            audit_receipts: capture.map(|capture| capture.receipts()).unwrap_or_default(),
        }
    }

    pub async fn close_query_cursor_controlled_with_audit(
        &mut self,
        cursor_id: QueryCursorId,
        control: &OperationControl,
    ) -> Audited<()> {
        let capture = self
            .cursor
            .filter(|active_id| *active_id == cursor_id)
            .map(|_| self.capture.begin());
        let result = self.inner.close_query_cursor_controlled(cursor_id, control).await;
        if self.cursor == Some(cursor_id) {
            self.cursor = None;
        }
        Audited {
            result,
            audit_receipts: capture.map(|capture| capture.receipts()).unwrap_or_default(),
        }
    }
}

#[async_trait]
impl Session for AuditedSession {
    async fn query_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        self.inner.query_params_controlled(sql, params, control).await
    }

    async fn commit_controlled(&mut self, control: &OperationControl) -> Result<SessionCommitOutcome, DriverError> {
        self.inner.commit_controlled(control).await
    }

    async fn query_result_sets_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        self.inner
            .query_result_sets_params_controlled(sql, params, control)
            .await
    }

    async fn fetch_completion_columns_controlled(
        &mut self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<Vec<String>, DriverError> {
        self.inner
            .fetch_completion_columns_controlled(schema, table, control)
            .await
    }

    async fn open_query_cursor_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryCursorId, DriverError> {
        self.inner.open_query_cursor_controlled(sql, params, control).await
    }

    async fn fetch_query_cursor_page_controlled(
        &mut self,
        cursor_id: QueryCursorId,
        control: &OperationControl,
    ) -> Result<QueryCursorPage, DriverError> {
        self.inner.fetch_query_cursor_page_controlled(cursor_id, control).await
    }

    async fn close_query_cursor_controlled(
        &mut self,
        cursor_id: QueryCursorId,
        control: &OperationControl,
    ) -> Result<(), DriverError> {
        self.inner.close_query_cursor_controlled(cursor_id, control).await
    }

    async fn lexical_settings_controlled(
        &mut self,
        control: &OperationControl,
    ) -> Result<Option<tablepro_core::sql_syntax::script::LexicalSettings>, DriverError> {
        self.inner.lexical_settings_controlled(control).await
    }

    fn is_usable(&self) -> bool {
        self.inner.is_usable()
    }

    fn transaction_open(&self) -> bool {
        self.inner.transaction_open()
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        self.inner.close().await
    }
}

impl PolicyGuard {
    pub async fn query_result_sets_controlled_with_audit(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Audited<QueryResultBatch> {
        let capture = Arc::new(CapturingAuditSink::new(self.ctx.audit.clone()));
        let scope = capture.begin();
        let mut guard = self.clone();
        guard.ctx.audit = capture.clone();
        let result = Connection::query_result_sets_controlled(&guard, sql, params, control).await;
        Audited {
            result,
            audit_receipts: scope.receipts(),
        }
    }

    pub async fn open_session_with_audit(&self) -> Result<AuditedSession, DriverError> {
        let capture = Arc::new(CapturingAuditSink::new(self.ctx.audit.clone()));
        let mut guard = self.clone();
        guard.ctx.audit = capture.clone();
        let inner = guard.caught_read("OPEN SESSION", guard.inner.open_session()).await?;
        Ok(AuditedSession {
            inner: guard.wrap_session(inner),
            capture,
            cursor: None,
        })
    }
}
