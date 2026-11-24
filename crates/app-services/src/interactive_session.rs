use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use bookie_core::QueryResult;
use bookie_core::{
    Connection, DriverError, OperationControl, QueryCursorId, QueryCursorPage, QueryResultBatch, Session,
    SessionCommitOutcome, TableInfo, Value,
};
use uuid::Uuid;

use crate::connection_identity::ConnectionIdentity;
use bookie_policy::{AuditReceipt, Audited, AuditedSession, PolicyGuard, Principal};

pub type SharedSession = Arc<tokio::sync::Mutex<Option<SessionHandle>>>;

pub enum SessionHandle {
    Audited(AuditedSession),
    Plain(Box<dyn Session>),
}

impl AsMut<dyn Session> for SessionHandle {
    fn as_mut(&mut self) -> &mut (dyn Session + 'static) {
        self
    }
}

impl SessionHandle {
    pub fn plain(session: Box<dyn Session>) -> Self {
        Self::Plain(session)
    }

    pub async fn query_result_sets_with_audit(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Audited<QueryResultBatch> {
        match self {
            Self::Audited(session) => {
                session
                    .query_result_sets_params_controlled_with_audit(sql, params, control)
                    .await
            }
            Self::Plain(session) => Audited {
                result: session.query_result_sets_params_controlled(sql, params, control).await,
                audit_receipts: Vec::new(),
            },
        }
    }

    pub async fn open_cursor_with_audit(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Audited<QueryCursorId> {
        match self {
            Self::Audited(session) => {
                session
                    .open_query_cursor_controlled_with_audit(sql, params, control)
                    .await
            }
            Self::Plain(session) => Audited {
                result: session.open_query_cursor_controlled(sql, params, control).await,
                audit_receipts: Vec::new(),
            },
        }
    }

    pub async fn fetch_cursor_with_audit(
        &mut self,
        cursor_id: QueryCursorId,
        control: &OperationControl,
    ) -> Audited<QueryCursorPage> {
        match self {
            Self::Audited(session) => {
                session
                    .fetch_query_cursor_page_controlled_with_audit(cursor_id, control)
                    .await
            }
            Self::Plain(session) => Audited {
                result: session.fetch_query_cursor_page_controlled(cursor_id, control).await,
                audit_receipts: Vec::new(),
            },
        }
    }

    pub async fn close_cursor_with_audit(
        &mut self,
        cursor_id: QueryCursorId,
        control: &OperationControl,
    ) -> Audited<()> {
        match self {
            Self::Audited(session) => {
                session
                    .close_query_cursor_controlled_with_audit(cursor_id, control)
                    .await
            }
            Self::Plain(session) => Audited {
                result: session.close_query_cursor_controlled(cursor_id, control).await,
                audit_receipts: Vec::new(),
            },
        }
    }
}

#[async_trait]
impl Session for SessionHandle {
    async fn list_temporary_tables_controlled(
        &mut self,
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        match self {
            Self::Audited(session) => session.list_temporary_tables_controlled(control).await,
            Self::Plain(session) => session.list_temporary_tables_controlled(control).await,
        }
    }

    async fn query_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<bookie_core::QueryResult, DriverError> {
        match self {
            Self::Audited(session) => session.query_params_controlled(sql, params, control).await,
            Self::Plain(session) => session.query_params_controlled(sql, params, control).await,
        }
    }

    async fn commit_controlled(&mut self, control: &OperationControl) -> Result<SessionCommitOutcome, DriverError> {
        match self {
            Self::Audited(session) => session.commit_controlled(control).await,
            Self::Plain(session) => session.commit_controlled(control).await,
        }
    }

    async fn query_result_sets_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        self.query_result_sets_with_audit(sql, params, control).await.result
    }

    async fn open_query_cursor_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryCursorId, DriverError> {
        self.open_cursor_with_audit(sql, params, control).await.result
    }

    async fn fetch_query_cursor_page_controlled(
        &mut self,
        cursor_id: QueryCursorId,
        control: &OperationControl,
    ) -> Result<QueryCursorPage, DriverError> {
        self.fetch_cursor_with_audit(cursor_id, control).await.result
    }

    async fn close_query_cursor_controlled(
        &mut self,
        cursor_id: QueryCursorId,
        control: &OperationControl,
    ) -> Result<(), DriverError> {
        self.close_cursor_with_audit(cursor_id, control).await.result
    }

    async fn fetch_completion_columns_controlled(
        &mut self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<Vec<String>, DriverError> {
        match self {
            Self::Audited(session) => {
                session
                    .fetch_completion_columns_controlled(schema, table, control)
                    .await
            }
            Self::Plain(session) => {
                session
                    .fetch_completion_columns_controlled(schema, table, control)
                    .await
            }
        }
    }

    async fn lexical_settings_controlled(
        &mut self,
        control: &OperationControl,
    ) -> Result<Option<bookie_core::sql_syntax::script::LexicalSettings>, DriverError> {
        match self {
            Self::Audited(session) => session.lexical_settings_controlled(control).await,
            Self::Plain(session) => session.lexical_settings_controlled(control).await,
        }
    }

    fn is_usable(&self) -> bool {
        match self {
            Self::Audited(session) => session.is_usable(),
            Self::Plain(session) => session.is_usable(),
        }
    }

    fn transaction_open(&self) -> bool {
        match self {
            Self::Audited(session) => session.transaction_open(),
            Self::Plain(session) => session.transaction_open(),
        }
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        match *self {
            Self::Audited(session) => Box::new(session).close().await,
            Self::Plain(session) => session.close().await,
        }
    }
}

pub struct OpenedSession(pub SharedSession);

pub async fn close_for_teardown(shared: SharedSession) -> Result<(), DriverError> {
    let session = shared.lock().await.take();
    match session {
        Some(session) => Box::new(session).close().await,
        None => Ok(()),
    }
}

impl std::fmt::Debug for OpenedSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OpenedSession")
    }
}

#[derive(Clone)]
pub enum StatementTarget {
    Pool(Arc<dyn Connection>),
    GuardedPool(Arc<PolicyGuard>),
    Session { id: Uuid, shared: SharedSession },
    Retired { id: Uuid },
}

pub struct StatementCursor {
    pub target: StatementTarget,
    pub id: QueryCursorId,
    pub owned_session: bool,
}

impl StatementCursor {
    pub async fn fetch_page_with_audit(&self, control: &OperationControl) -> Audited<QueryCursorPage> {
        let StatementTarget::Session { shared, .. } = &self.target else {
            return audited_error(DriverError::Internal("cursor target is not a session".into()));
        };
        let mut session = shared.lock().await;
        match session.as_mut() {
            Some(session) => session.fetch_cursor_with_audit(self.id, control).await,
            None => audited_error(DriverError::Unsupported("the cursor session was closed".into())),
        }
    }

    pub async fn finish_with_audit(self, close_cursor: bool) -> Audited<()> {
        let StatementTarget::Session { shared, .. } = &self.target else {
            return audited_error(DriverError::Internal("cursor target is not a session".into()));
        };
        let mut audit_receipts = Vec::new();
        let close = if close_cursor {
            let mut session = shared.lock().await;
            match session.as_mut() {
                Some(session) => {
                    let control = OperationControl::with_timeout(std::time::Duration::from_secs(10));
                    let closed = session.close_cursor_with_audit(self.id, &control).await;
                    merge_audit_receipts(&mut audit_receipts, closed.audit_receipts);
                    closed.result
                }
                None => Ok(()),
            }
        } else {
            Ok(())
        };
        let rollback_control = OperationControl::with_timeout(std::time::Duration::from_secs(10));
        let rollback = self.target.query_with_audit("ROLLBACK", &[], &rollback_control).await;
        merge_audit_receipts(&mut audit_receipts, rollback.audit_receipts);
        let session_close = if self.owned_session {
            close_for_teardown(shared.clone()).await
        } else {
            Ok(())
        };
        Audited {
            result: close.and(rollback.result.map(|_| ())).and(session_close),
            audit_receipts,
        }
    }

    pub async fn finish(self, close_cursor: bool) -> Result<(), DriverError> {
        self.finish_with_audit(close_cursor).await.result
    }
}

impl StatementTarget {
    pub async fn open_query_cursor_with_audit(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Audited<Option<StatementCursor>> {
        let (target, owned_session) = match self.cursor_target().await {
            Ok(Some(target)) => target,
            Ok(None) => return audited_ok(None),
            Err(error) => return audited_error(error),
        };
        open_query_cursor_on_target(target, owned_session, sql, params, control).await
    }

    pub async fn cursor_target(&self) -> Result<Option<(StatementTarget, bool)>, DriverError> {
        match self {
            Self::Pool(connection) => {
                let session = connection.open_session().await?;
                let shared = Arc::new(tokio::sync::Mutex::new(Some(SessionHandle::plain(session))));
                Ok(Some((
                    Self::Session {
                        id: Uuid::new_v4(),
                        shared,
                    },
                    true,
                )))
            }
            Self::GuardedPool(guard) => {
                let session = guard.open_session_with_audit().await?;
                let shared = Arc::new(tokio::sync::Mutex::new(Some(SessionHandle::Audited(session))));
                Ok(Some((
                    Self::Session {
                        id: Uuid::new_v4(),
                        shared,
                    },
                    true,
                )))
            }
            Self::Session { .. } => {
                let Some((_, transaction_open, usable)) = self.session_state().await else {
                    return Ok(None);
                };
                if transaction_open || !usable {
                    return Ok(None);
                }
                Ok(Some((self.clone(), false)))
            }
            Self::Retired { .. } => Err(retired_session_error()),
        }
    }

    pub async fn release_cursor_session(target: &StatementTarget, owned_session: bool) {
        let rollback_control = OperationControl::with_timeout(std::time::Duration::from_secs(10));
        if let Err(error) = target.query("ROLLBACK", &[], &rollback_control).await {
            crate::driver_diagnostics::warn_driver_cleanup(&error, "setup_rollback");
        }
        if owned_session
            && let Self::Session { shared, .. } = target
            && let Err(error) = close_for_teardown(shared.clone()).await
        {
            crate::driver_diagnostics::warn_driver_cleanup(&error, "setup_close");
        }
    }

    pub async fn query(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        self.query_with_audit(sql, params, control).await.result
    }

    pub async fn query_with_audit(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Audited<QueryResultBatch> {
        match self {
            Self::Pool(connection) => Audited {
                result: connection.query_result_sets_controlled(sql, params, control).await,
                audit_receipts: Vec::new(),
            },
            Self::GuardedPool(guard) => {
                guard
                    .query_result_sets_controlled_with_audit(sql, params, control)
                    .await
            }
            Self::Session { shared, .. } => match shared.lock().await.as_mut() {
                Some(session) if session.is_usable() => {
                    session.query_result_sets_with_audit(sql, params, control).await
                }
                Some(_) => audited_error(retired_session_error()),
                None => audited_error(DriverError::Unsupported("the session was closed".into())),
            },
            Self::Retired { .. } => audited_error(retired_session_error()),
        }
    }

    pub async fn lexical_settings(
        &self,
        control: &OperationControl,
    ) -> Result<Option<bookie_core::sql_syntax::script::LexicalSettings>, DriverError> {
        match self {
            Self::Pool(connection) => connection.lexical_settings_controlled(control).await,
            Self::GuardedPool(guard) => guard.lexical_settings_controlled(control).await,
            Self::Session { shared, .. } => match shared.lock().await.as_mut() {
                Some(session) if session.is_usable() => session.lexical_settings_controlled(control).await,
                Some(_) => Err(retired_session_error()),
                None => Err(DriverError::Unsupported("the session was closed".into())),
            },
            Self::Retired { .. } => Err(retired_session_error()),
        }
    }

    pub async fn session_state(&self) -> Option<(Uuid, bool, bool)> {
        match self {
            Self::Pool(_) | Self::GuardedPool(_) => None,
            Self::Retired { id } => Some((*id, false, false)),
            Self::Session { id, shared } => {
                let session = shared.lock().await;
                Some(match session.as_ref() {
                    Some(session) => (*id, session.transaction_open(), session.is_usable()),
                    None => (*id, false, false),
                })
            }
        }
    }
}

pub async fn open_query_cursor_on_target(
    target: StatementTarget,
    owned_session: bool,
    sql: &str,
    params: &[Value],
    control: &OperationControl,
) -> Audited<Option<StatementCursor>> {
    let begin = target
        .query_with_audit("BEGIN READ ONLY ISOLATION LEVEL REPEATABLE READ", &[], control)
        .await;
    let mut audit_receipts = begin.audit_receipts;
    if let Err(error) = begin.result {
        StatementTarget::release_cursor_session(&target, owned_session).await;
        return Audited {
            result: Err(error),
            audit_receipts,
        };
    }
    let cursor = match &target {
        StatementTarget::Session { shared, .. } => {
            let mut session = shared.lock().await;
            match session.as_mut() {
                Some(session) => session.open_cursor_with_audit(sql, params, control).await,
                None => audited_error(DriverError::Unsupported("the cursor session was closed".into())),
            }
        }
        StatementTarget::Pool(_) | StatementTarget::GuardedPool(_) | StatementTarget::Retired { .. } => {
            audited_error(DriverError::Internal("cursor target is not a session".into()))
        }
    };
    audit_receipts.extend(cursor.audit_receipts);
    match cursor.result {
        Ok(id) => Audited {
            result: Ok(Some(StatementCursor {
                target,
                id,
                owned_session,
            })),
            audit_receipts,
        },
        Err(error) => {
            StatementTarget::release_cursor_session(&target, owned_session).await;
            Audited {
                result: if matches!(error, DriverError::Unsupported(_)) {
                    Ok(None)
                } else {
                    Err(error)
                },
                audit_receipts,
            }
        }
    }
}

pub fn retired_session_error() -> DriverError {
    DriverError::Unsupported("the editor session was retired; reopen Session before running more SQL".into())
}

pub fn audited_ok<T>(value: T) -> Audited<T> {
    Audited {
        result: Ok(value),
        audit_receipts: Vec::new(),
    }
}

pub fn audited_error<T>(error: DriverError) -> Audited<T> {
    Audited {
        result: Err(error),
        audit_receipts: Vec::new(),
    }
}

pub fn merge_audit_receipts(target: &mut Vec<AuditReceipt>, incoming: Vec<AuditReceipt>) {
    for receipt in incoming {
        if !target.contains(&receipt) {
            target.push(receipt);
        }
    }
}

pub fn retired_connection_matches(retired: Option<(Uuid, Uuid)>, active: Option<Uuid>) -> bool {
    active.is_some_and(|active| retired.is_some_and(|(connection, _)| connection == active))
}

pub struct EditorSession {
    pub id: Uuid,
    pub shared: SharedSession,
    pub connection_id: Uuid,
    pub connection_identity: Option<ConnectionIdentity>,
    pub transaction_open: bool,
}

pub fn session_callback_matches(callback_id: Uuid, current_id: Option<Uuid>) -> bool {
    current_id == Some(callback_id)
}

pub fn open_callback_matches(
    callback_id: Uuid,
    pending_id: Option<Uuid>,
    callback_connection_id: Uuid,
    current_connection_id: Option<Uuid>,
) -> bool {
    session_callback_matches(callback_id, pending_id) && Some(callback_connection_id) == current_connection_id
}

pub fn commit_callback_matches(callback_id: Uuid, current_id: Option<Uuid>, ending_id: Option<Uuid>) -> bool {
    session_callback_matches(callback_id, current_id) && Some(callback_id) == ending_id
}

pub fn apply_transaction_state(session: &mut EditorSession, callback_id: Uuid, transaction_open: bool) -> bool {
    if !session_callback_matches(callback_id, Some(session.id)) {
        return false;
    }
    session.transaction_open = transaction_open;
    true
}

pub fn can_start_session_close(
    teardown_active: bool,
    close_running: bool,
    open_running: bool,
    query_running: bool,
) -> bool {
    teardown_active && !close_running && !open_running && !query_running
}

pub async fn commit_session(shared: &SharedSession) -> Result<SessionCommitOutcome, DriverError> {
    let mut guard = shared.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| DriverError::Unsupported("the session was closed".into()))?;
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    session.commit_controlled(&control).await
}

pub async fn rollback_and_close(shared: &SharedSession) -> Result<(), DriverError> {
    let Some(mut session) = shared.lock().await.take() else {
        return Err(DriverError::Unsupported("the session was closed".into()));
    };
    if !session.is_usable() {
        return Box::new(session).close().await;
    }
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    let rollback = session.query_params_controlled("ROLLBACK", &[], &control).await;
    let close = Box::new(session).close().await;
    match (rollback, close) {
        (Err(error), _) => Err(error),
        (Ok(_), result) => result,
    }
}

pub async fn close_detached_session(shared: SharedSession) {
    let Some(session) = shared.lock().await.take() else {
        return;
    };
    if let Err(error) = Box::new(session).close().await {
        crate::driver_diagnostics::warn_driver_cleanup(&error, "session_close");
    }
}
