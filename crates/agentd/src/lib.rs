//! Shared composition pieces for the headless TablePro agent daemon.
//!
//! Agents reach a database through exactly the transport a saved connection
//! describes, and every handle handed out is wrapped by `PolicyGuard`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tablepro_core::{
    AuthMode, ColumnInfo, Connection, DriverError, DriverRegistry, ExecResult, ForeignKeyInfo, IndexInfo,
    OperationControl, QueryResult, TableInfo, TlsMode, Transaction, Value,
};
use tablepro_mcp::ConnectionProvider;
use tablepro_policy::{AuditState, GuardContext, PolicyConfig, PolicyGuard, Principal};
use tablepro_storage::{SavedConnection, SavedSshConfig, load_connections};
use uuid::Uuid;

const SESSION_PING_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) const AGENT_UNKNOWN_HOST_KEY: tablepro_ssh::UnknownHostKey = tablepro_ssh::UnknownHostKey::Refuse;

struct OpenSession {
    key: SessionKey,
    connection: Arc<dyn Connection>,
    audit_state: Arc<AuditState>,
    retired: Arc<std::sync::atomic::AtomicBool>,
}

struct SessionFaultSink {
    retired: Arc<std::sync::atomic::AtomicBool>,
}

impl tablepro_policy::ConnectionFaultSink for SessionFaultSink {
    fn connection_became_unusable(&self, operation: &str) {
        tracing::warn!(operation, "a cached session became unusable and will be replaced");
        self.retired.store(true, std::sync::atomic::Ordering::Release);
    }
}

struct SessionConnection {
    inner: Arc<dyn Connection>,
    _tunnel: Option<tablepro_transport::Tunnel>,
}

#[async_trait]
impl Connection for SessionConnection {
    fn supports_server_cancellation(&self) -> bool {
        self.inner.supports_server_cancellation()
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        self.inner.list_tables().await
    }

    async fn list_tables_controlled(&self, control: &OperationControl) -> Result<Vec<TableInfo>, DriverError> {
        self.inner.list_tables_controlled(control).await
    }

    async fn list_views(&self) -> Result<Vec<TableInfo>, DriverError> {
        self.inner.list_views().await
    }

    async fn list_views_controlled(&self, control: &OperationControl) -> Result<Vec<TableInfo>, DriverError> {
        self.inner.list_views_controlled(control).await
    }

    async fn list_objects(
        &self,
        kind: tablepro_core::CatalogObjectKind,
        schema: Option<&str>,
    ) -> Result<Vec<tablepro_core::CatalogObject>, DriverError> {
        self.inner.list_objects(kind, schema).await
    }

    async fn list_objects_controlled(
        &self,
        kind: tablepro_core::CatalogObjectKind,
        schema: Option<&str>,
        control: &OperationControl,
    ) -> Result<Vec<tablepro_core::CatalogObject>, DriverError> {
        self.inner.list_objects_controlled(kind, schema, control).await
    }

    async fn fetch_columns(&self, schema: Option<&str>, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        self.inner.fetch_columns(schema, table).await
    }

    async fn fetch_columns_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<Vec<ColumnInfo>, DriverError> {
        self.inner.fetch_columns_controlled(schema, table, control).await
    }

    async fn fetch_rows(
        &self,
        schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<QueryResult, DriverError> {
        self.inner.fetch_rows(schema, table, offset, limit).await
    }

    async fn fetch_rows_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        self.inner
            .fetch_rows_controlled(schema, table, offset, limit, control)
            .await
    }

    async fn query(&self, sql: &str) -> Result<QueryResult, DriverError> {
        self.inner.query(sql).await
    }

    async fn query_controlled(&self, sql: &str, control: &OperationControl) -> Result<QueryResult, DriverError> {
        self.inner.query_controlled(sql, control).await
    }

    async fn query_params(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DriverError> {
        self.inner.query_params(sql, params).await
    }

    async fn query_params_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        self.inner.query_params_controlled(sql, params, control).await
    }

    async fn execute(&self, sql: &str) -> Result<ExecResult, DriverError> {
        self.inner.execute(sql).await
    }

    async fn execute_controlled(&self, sql: &str, control: &OperationControl) -> Result<ExecResult, DriverError> {
        self.inner.execute_controlled(sql, control).await
    }

    async fn execute_params(&self, sql: &str, params: &[Value]) -> Result<ExecResult, DriverError> {
        self.inner.execute_params(sql, params).await
    }

    async fn execute_params_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<ExecResult, DriverError> {
        self.inner.execute_params_controlled(sql, params, control).await
    }

    async fn execute_in_transaction(&self, statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        self.inner.execute_in_transaction(statements).await
    }

    async fn execute_in_transaction_controlled(
        &self,
        statements: &[(String, Vec<Value>)],
        control: &OperationControl,
    ) -> Result<Vec<u64>, DriverError> {
        self.inner.execute_in_transaction_controlled(statements, control).await
    }

    async fn synchronize_auto_increment_sequences(
        &self,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
    ) -> Result<(), DriverError> {
        self.inner
            .synchronize_auto_increment_sequences(schema, table, columns)
            .await
    }

    async fn synchronize_auto_increment_sequences_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
        control: &OperationControl,
    ) -> Result<(), DriverError> {
        self.inner
            .synchronize_auto_increment_sequences_controlled(schema, table, columns, control)
            .await
    }

    async fn fetch_indexes(&self, schema: Option<&str>, table: &str) -> Result<Vec<IndexInfo>, DriverError> {
        self.inner.fetch_indexes(schema, table).await
    }

    async fn fetch_indexes_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<Vec<IndexInfo>, DriverError> {
        self.inner.fetch_indexes_controlled(schema, table, control).await
    }

    async fn fetch_foreign_keys(&self, schema: Option<&str>, table: &str) -> Result<Vec<ForeignKeyInfo>, DriverError> {
        self.inner.fetch_foreign_keys(schema, table).await
    }

    async fn fetch_enum_labels(&self, schema: &str, name: &str) -> Result<Vec<String>, DriverError> {
        self.inner.fetch_enum_labels(schema, name).await
    }

    async fn fetch_enum_labels_controlled(
        &self,
        schema: &str,
        name: &str,
        control: &OperationControl,
    ) -> Result<Vec<String>, DriverError> {
        self.inner.fetch_enum_labels_controlled(schema, name, control).await
    }

    async fn fetch_foreign_keys_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<Vec<ForeignKeyInfo>, DriverError> {
        self.inner.fetch_foreign_keys_controlled(schema, table, control).await
    }

    async fn list_databases(&self) -> Result<Vec<String>, DriverError> {
        self.inner.list_databases().await
    }

    async fn open_session(&self) -> Result<Box<dyn tablepro_core::Session>, DriverError> {
        self.inner.open_session().await
    }

    async fn query_result_sets_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<tablepro_core::QueryResultBatch, DriverError> {
        self.inner.query_result_sets_controlled(sql, params, control).await
    }

    async fn execute_in_transaction_checked(
        &self,
        statements: &[(String, Vec<Value>)],
        expect_one: &[usize],
    ) -> Result<Vec<u64>, DriverError> {
        self.inner.execute_in_transaction_checked(statements, expect_one).await
    }

    async fn execute_in_transaction_checked_controlled(
        &self,
        statements: &[(String, Vec<Value>)],
        expect_one: &[usize],
        control: &OperationControl,
    ) -> Result<Vec<u64>, DriverError> {
        self.inner
            .execute_in_transaction_checked_controlled(statements, expect_one, control)
            .await
    }

    async fn begin(&self) -> Result<Box<dyn Transaction>, DriverError> {
        self.inner.begin().await
    }

    async fn begin_controlled(&self, control: &OperationControl) -> Result<Box<dyn Transaction>, DriverError> {
        self.inner.begin_controlled(control).await
    }

    async fn server_version(&self) -> Result<Option<String>, DriverError> {
        self.inner.server_version().await
    }

    async fn ping(&self) -> Result<(), DriverError> {
        self.inner.ping().await
    }

    async fn lexical_settings_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Option<tablepro_core::sql_syntax::script::LexicalSettings>, DriverError> {
        self.inner.lexical_settings_controlled(control).await
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
struct SessionKey {
    driver_id: String,
    host: String,
    port: u16,
    socket_dir: Option<PathBuf>,
    socket_path: Option<PathBuf>,
    database: String,
    username: String,
    tls_mode: TlsMode,
    tls_root_cert: Option<PathBuf>,
    auth_mode: AuthMode,
    ssh: Option<SavedSshConfig>,
    material: [u8; 32],
}

impl SessionKey {
    fn from_saved(saved: &SavedConnection, material: [u8; 32]) -> Self {
        Self {
            driver_id: saved.driver_id.clone(),
            host: saved.host.clone(),
            port: saved.port,
            socket_dir: saved.socket_dir.clone(),
            socket_path: saved.socket_path.clone(),
            database: saved.database.clone(),
            username: saved.username.clone(),
            tls_mode: saved.effective_tls_mode(),
            tls_root_cert: saved.tls_root_cert.clone(),
            auth_mode: saved.auth_mode,
            ssh: saved.ssh.clone(),
            material,
        }
    }
}

pub struct DaemonProvider {
    registry: Arc<DriverRegistry>,
    policy: Arc<PolicyConfig>,
    audit: Arc<dyn tablepro_policy::AuditSink>,
    audit_state: Arc<AuditState>,
    approval: Arc<dyn tablepro_policy::ApprovalSink>,
    sessions: Mutex<HashMap<Uuid, OpenSession>>,
    session_locks: Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
    ssh: tablepro_transport::SshEnvironment,
}

#[async_trait]
impl ConnectionProvider for DaemonProvider {
    async fn list_saved_connections(&self) -> Result<Vec<SavedConnection>, String> {
        load_connections().await.map_err(|e| e.to_string())
    }

    async fn connection(&self, connection_id: Uuid, principal: Principal) -> Result<Arc<dyn Connection>, String> {
        let saved = load_connections()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|c| c.id == connection_id)
            .ok_or_else(|| format!("connection {connection_id} not found"))?;

        let raw = self.open_session_for(&saved, principal.clone()).await?;
        Ok(self.guarded(&saved, principal, raw))
    }
}

impl DaemonProvider {
    pub fn new(
        registry: Arc<DriverRegistry>,
        policy: Arc<PolicyConfig>,
        audit: Arc<dyn tablepro_policy::AuditSink>,
        audit_state: Arc<AuditState>,
        approval: Arc<dyn tablepro_policy::ApprovalSink>,
    ) -> Self {
        Self {
            registry,
            policy,
            audit,
            audit_state,
            approval,
            sessions: Mutex::new(HashMap::new()),
            session_locks: Mutex::new(HashMap::new()),
            ssh: tablepro_transport::SshEnvironment::builtin(AGENT_UNKNOWN_HOST_KEY),
        }
    }

    fn new_session(&self, key: SessionKey, connection: Arc<dyn Connection>) -> OpenSession {
        OpenSession {
            key,
            connection,
            audit_state: Arc::new(self.audit_state.new_connection_generation()),
            retired: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    fn session_retirement(&self, id: Uuid, connection: &Arc<dyn Connection>) -> Arc<std::sync::atomic::AtomicBool> {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| {
                sessions
                    .get(&id)
                    .filter(|session| Arc::ptr_eq(&session.connection, connection))
                    .map(|session| session.retired.clone())
            })
            .unwrap_or_default()
    }

    fn session_audit_state(&self, id: Uuid, connection: &Arc<dyn Connection>) -> Arc<AuditState> {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| {
                sessions
                    .get(&id)
                    .filter(|session| Arc::ptr_eq(&session.connection, connection))
                    .map(|session| session.audit_state.clone())
            })
            .unwrap_or_else(|| Arc::new(self.audit_state.new_connection_generation()))
    }

    fn guarded(&self, saved: &SavedConnection, principal: Principal, raw: Arc<dyn Connection>) -> Arc<dyn Connection> {
        let ctx = GuardContext {
            connection_id: saved.id,
            connection_name: saved.name.clone(),
            driver_id: saved.driver_id.clone(),
            environment: saved.environment,
            read_only: saved.read_only,
            principal,
            policy: self.policy.clone(),
            approval: self.approval.clone(),
            audit: self.audit.clone(),
            audit_state: self.session_audit_state(saved.id, &raw),
        };
        let fault = Arc::new(SessionFaultSink {
            retired: self.session_retirement(saved.id, &raw),
        });
        Arc::new(PolicyGuard::new(raw, ctx).with_fault_sink(fault))
    }

    pub fn with_system_openssh(mut self, openssh: tablepro_transport::OpenSshEnvironment) -> Self {
        self.ssh.openssh = Some(openssh);
        self
    }

    #[cfg(test)]
    async fn open_session(&self, saved: &SavedConnection) -> Result<Arc<dyn Connection>, String> {
        self.open_session_for(saved, Principal::human_gui()).await
    }

    async fn open_session_for(
        &self,
        saved: &SavedConnection,
        principal: Principal,
    ) -> Result<Arc<dyn Connection>, String> {
        let session_lock = self.session_lock(saved.id)?;
        let _open = session_lock.lock_owned().await;
        let material = match tablepro_transport::session_material_digest(saved).await {
            Ok(material) => material,
            Err(error) => return Err(error.to_string()),
        };
        let key = SessionKey::from_saved(saved, material);
        let cached = self.cached_connection(saved.id, &key)?;
        if let Some(connection) = cached {
            let healthy = ping_is_healthy(connection.ping(), SESSION_PING_TIMEOUT).await;
            let retired = self
                .session_retirement(saved.id, &connection)
                .load(std::sync::atomic::Ordering::Acquire);
            if healthy && !retired && self.session_is_current(saved.id, &key, &connection)? {
                return Ok(connection);
            }
            self.remove_session(saved.id, &key, &connection)?;
        }

        let connection = self.connect_session(saved, principal).await?;
        let material = match tablepro_transport::session_material_digest(saved).await {
            Ok(material) => material,
            Err(error) => {
                drop(connection);
                return Err(error.to_string());
            }
        };
        let key = SessionKey::from_saved(saved, material);
        self.sessions
            .lock()
            .map_err(|_| "session cache unavailable".to_string())?
            .insert(saved.id, self.new_session(key, connection.clone()));
        Ok(connection)
    }

    async fn connect_session(
        &self,
        saved: &SavedConnection,
        principal: Principal,
    ) -> Result<Arc<dyn Connection>, String> {
        let driver = self
            .registry
            .get(&saved.driver_id)
            .ok_or_else(|| format!("driver {} not registered", saved.driver_id))?;
        let ssh = tablepro_transport::saved_ssh_route(saved)
            .await
            .map_err(|e| e.to_string())?;
        let mut opts = tablepro_transport::connect_options_for(saved)
            .await
            .map_err(|e| e.to_string())?;
        opts.application_name = Some("BookiE agent".into());
        let ssh_environment = self
            .ssh
            .clone()
            .with_audit(tablepro_transport::TransportAuditContext::new(
                self.audit.clone(),
                self.audit_state.clone(),
                principal,
                saved.id,
                saved.name.clone(),
                saved.environment,
                saved.driver_id.clone(),
            ));
        let (raw, tunnel) = tablepro_transport::establish(driver.as_ref(), opts, ssh, &ssh_environment)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(SessionConnection {
            inner: Arc::from(raw),
            _tunnel: tunnel,
        }))
    }

    fn cached_connection(&self, id: Uuid, key: &SessionKey) -> Result<Option<Arc<dyn Connection>>, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "session cache unavailable".to_string())?;
        let stale = if sessions.get(&id).is_some_and(|session| session.key != *key) {
            sessions.remove(&id)
        } else {
            None
        };
        let connection = sessions.get(&id).map(|session| session.connection.clone());
        drop(stale);
        Ok(connection)
    }

    fn session_is_current(&self, id: Uuid, key: &SessionKey, connection: &Arc<dyn Connection>) -> Result<bool, String> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| "session cache unavailable".to_string())?;
        Ok(sessions
            .get(&id)
            .is_some_and(|session| session.key == *key && Arc::ptr_eq(&session.connection, connection)))
    }

    fn remove_session(&self, id: Uuid, key: &SessionKey, connection: &Arc<dyn Connection>) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "session cache unavailable".to_string())?;
        if sessions
            .get(&id)
            .is_some_and(|session| session.key == *key && Arc::ptr_eq(&session.connection, connection))
        {
            sessions.remove(&id);
        }
        Ok(())
    }

    fn session_lock(&self, id: Uuid) -> Result<Arc<tokio::sync::Mutex<()>>, String> {
        let mut locks = self
            .session_locks
            .lock()
            .map_err(|_| "session lock cache unavailable".to_string())?;
        Ok(locks
            .entry(id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone())
    }
}

async fn ping_is_healthy(
    ping: impl std::future::Future<Output = Result<(), tablepro_core::DriverError>>,
    timeout: Duration,
) -> bool {
    tokio::time::timeout(timeout, ping)
        .await
        .is_ok_and(|result| result.is_ok())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
mod session_tests;

#[cfg(test)]
mod audit_isolation_tests;

#[cfg(test)]
mod session_forwarding_tests;
