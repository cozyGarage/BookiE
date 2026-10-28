use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use tablepro_core::{ConnectOptions, Connection, DatabaseDriver, Environment};
use tablepro_policy::{
    AuditAdministrativeAction, AuditEvent, AuditRecordPhase, AuditState, AuditTerminalStatus, DenyApprovalSink,
    GuardContext, NullAuditSink, PolicyConfig, PolicyGuard, Principal, load_from_path,
};
use tablepro_storage::AuditJournal;
use tablepro_transport::{OpenSshEnvironment, SshEnvironment, SshRoute, Tunnel};

use super::config_io::xdg_config_path;
use super::connection_monitor;

fn load_policy() -> Result<PolicyConfig, String> {
    let path = xdg_config_path("policy.toml").ok_or("neither XDG_CONFIG_HOME nor HOME is set")?;
    load_policy_file(&path)
}

fn load_policy_file(path: &std::path::Path) -> Result<PolicyConfig, String> {
    if !path.exists() {
        return Ok(PolicyConfig::default());
    }
    load_from_path(path)
}

pub use tablepro_app_services::connection_identity::ConnectionIdentity;

pub(super) struct EntryInner {
    pub(super) connection: Arc<dyn Connection>,
    pub(super) tunnel: Option<Tunnel>,
    pub(super) health: ConnectionHealth,
    pub(super) audit_state: Arc<AuditState>,
}

#[derive(Debug, Clone)]
pub struct ConnectionMetadata {
    pub id: Uuid,
    pub name: String,
    pub driver_id: String,
    pub environment: Environment,
    pub read_only: bool,
    pub server_version: Option<String>,
    pub query_timeout_secs: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionHealth {
    Healthy,
    Reconnecting { attempt: u32 },
    Failed { reason: String },
}

pub struct ReconnectParams {
    pub driver: Arc<dyn DatabaseDriver>,
    pub opts: ConnectOptions,
    pub ssh: Option<SshRoute>,
    pub environment: SshEnvironment,
}

/// Builds the same audited policy wrapper used by active connections for
/// connection bootstrap reads. A candidate is not registered yet, but its
/// metadata reads still belong to the durable audit journal.
#[derive(Clone)]
pub struct CandidateGuardFactory {
    policy: Arc<PolicyConfig>,
    approval: Arc<dyn tablepro_policy::ApprovalSink>,
    audit: Arc<dyn tablepro_policy::AuditSink>,
    audit_state: Arc<AuditState>,
    unresolved_connection_ids: Arc<HashSet<Uuid>>,
}

impl CandidateGuardFactory {
    pub fn guard(
        &self,
        connection: Arc<dyn Connection>,
        metadata: &ConnectionMetadata,
        principal: Principal,
    ) -> PolicyGuard {
        PolicyGuard::new(
            connection,
            GuardContext {
                connection_id: metadata.id,
                connection_name: metadata.name.clone(),
                driver_id: metadata.driver_id.clone(),
                environment: metadata.environment,
                read_only: metadata.read_only,
                principal,
                policy: self.policy.clone(),
                approval: self.approval.clone(),
                audit: self.audit.clone(),
                audit_state: Arc::new(self.connection_audit_state(metadata.id)),
            },
        )
    }
}

impl CandidateGuardFactory {
    fn connection_audit_state(&self, id: Uuid) -> AuditState {
        self.audit_state
            .new_connection_generation_with_writes_disabled(self.unresolved_connection_ids.contains(&id))
    }
}

struct Entry {
    inner: Arc<Mutex<EntryInner>>,
    metadata: ConnectionMetadata,
    read_only: bool,
    environment: Environment,
    cancel: CancellationToken,
    fault: Arc<Notify>,
    _monitor: tokio::task::JoinHandle<()>,
}

/// A driver that panicked left the connection's protocol state
/// unverified, so the next statement on it cannot be trusted. Waking the
/// monitor replaces it instead of waiting for the ping interval to
/// notice, which it may not: a desynchronised connection can still
/// answer a ping.
struct EntryFaultSink {
    connection_id: Uuid,
    fault: Arc<Notify>,
}

impl tablepro_policy::ConnectionFaultSink for EntryFaultSink {
    fn connection_became_unusable(&self, operation: &str) {
        tracing::warn!(
            connection_id = %self.connection_id,
            operation,
            "driver stopped mid-operation; replacing the connection"
        );
        self.fault.notify_one();
    }
}

struct AuditRuntime {
    sink: Arc<dyn tablepro_policy::AuditSink>,
    journal: Option<Arc<AuditJournal>>,
    available: bool,
    state: Arc<AuditState>,
    unresolved_connection_ids: Arc<HashSet<Uuid>>,
}

impl AuditRuntime {
    fn open_default() -> Self {
        Self::from_journal(AuditJournal::open_default())
    }

    #[cfg(test)]
    fn open_path(path: std::path::PathBuf) -> Self {
        Self::from_journal(AuditJournal::open_validated(path))
    }

    fn from_journal(result: Result<AuditJournal, tablepro_storage::StorageError>) -> Self {
        match result {
            Ok(journal) => {
                let recovery = journal.recovery();
                let has_unresolved_writes = recovery.has_unresolved_writes();
                let unresolved_connection_ids: HashSet<Uuid> =
                    recovery.unresolved_connection_ids().iter().copied().collect();
                let has_unresolved_global_writes = recovery.has_unresolved_global_writes();
                if has_unresolved_writes {
                    tracing::error!(
                        operations = journal.recovery().unresolved_operation_ids().len(),
                        "unresolved audit write outcomes detected; affected connections remain blocked"
                    );
                }
                let state = if has_unresolved_global_writes {
                    AuditState::with_governed_writes_disabled()
                } else {
                    AuditState::new()
                };
                let journal = Arc::new(journal);
                Self {
                    sink: journal.clone(),
                    journal: Some(journal),
                    available: true,
                    state: Arc::new(state),
                    unresolved_connection_ids: Arc::new(unresolved_connection_ids),
                }
            }
            Err(error) => {
                tracing::error!(%error, "audit journal unavailable; MCP and governed writes are disabled");
                Self::unavailable()
            }
        }
    }

    fn unavailable() -> Self {
        Self {
            sink: Arc::new(NullAuditSink),
            journal: None,
            available: false,
            state: Arc::new(AuditState::with_governed_writes_disabled()),
            unresolved_connection_ids: Arc::new(HashSet::new()),
        }
    }
}

pub struct DatabaseService {
    connections: Mutex<HashMap<Uuid, Entry>>,
    policy: Mutex<Arc<PolicyConfig>>,
    audit: Arc<dyn tablepro_policy::AuditSink>,
    audit_journal: Option<Arc<AuditJournal>>,
    audit_available: bool,
    policy_available: bool,
    audit_state: Arc<AuditState>,
    unresolved_connection_ids: Arc<HashSet<Uuid>>,
    approval: Mutex<Arc<dyn tablepro_policy::ApprovalSink>>,
    ssh_environment: Mutex<SshEnvironment>,
    stream_result_spools: tablepro_app_services::stream_result_spool::StreamResultSpoolManager,
    #[cfg(test)]
    _audit_temp_dir: Option<tempfile::TempDir>,
}

impl DatabaseService {
    pub async fn recent_audit(&self, limit: usize) -> Result<Vec<tablepro_policy::AuditEvent>, String> {
        let journal = self
            .audit_journal
            .clone()
            .ok_or_else(|| "the audit journal is unavailable".to_string())?;
        journal.recent(limit).await.map_err(|error| error.to_string())
    }

    pub fn new() -> Self {
        let audit = AuditRuntime::open_default();
        Self::with_audit(
            audit,
            #[cfg(test)]
            None,
        )
    }

    #[cfg(test)]
    pub(crate) fn new_isolated() -> Self {
        let audit_temp_dir = tempfile::tempdir().expect("temporary audit directory");
        let audit = AuditRuntime::open_path(audit_temp_dir.path().join("audit.jsonl"));
        Self::with_audit(audit, Some(audit_temp_dir))
    }

    fn with_audit(audit: AuditRuntime, #[cfg(test)] audit_temp_dir: Option<tempfile::TempDir>) -> Self {
        let (policy, policy_available) = match load_policy() {
            Ok(policy) => (Arc::new(policy), true),
            Err(error) => {
                tracing::error!(
                    error = %error,
                    "policy file could not be loaded; using defaults and disabling MCP"
                );
                (Arc::new(PolicyConfig::default()), false)
            }
        };
        Self {
            connections: Mutex::new(HashMap::new()),
            policy: Mutex::new(policy),
            audit: audit.sink,
            audit_journal: audit.journal,
            audit_available: audit.available,
            policy_available,
            audit_state: audit.state,
            unresolved_connection_ids: audit.unresolved_connection_ids,
            approval: Mutex::new(Arc::new(DenyApprovalSink)),
            ssh_environment: Mutex::new(SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Refuse)),
            stream_result_spools: tablepro_app_services::stream_result_spool::StreamResultSpoolManager::default(),
            #[cfg(test)]
            _audit_temp_dir: audit_temp_dir,
        }
    }

    pub fn audit_available(&self) -> bool {
        self.audit_available
    }

    pub fn stream_result_spool_manager(&self) -> tablepro_app_services::stream_result_spool::StreamResultSpoolManager {
        self.stream_result_spools.clone()
    }

    pub fn policy_available(&self) -> bool {
        self.policy_available
    }

    pub async fn audit_administrative_action(
        &self,
        operation_id: Uuid,
        action: AuditAdministrativeAction,
        phase: AuditRecordPhase,
        terminal_status: AuditTerminalStatus,
        affected_count: usize,
        duration_ms: Option<u64>,
    ) -> Result<(), tablepro_policy::AuditError> {
        let event = AuditEvent::administrative_action(
            operation_id,
            action,
            phase,
            terminal_status,
            affected_count,
            duration_ms,
        );
        match self.audit.record(event).await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.audit_state.disable_after_audit_failure();
                Err(error)
            }
        }
    }

    /// Whether a prior governed operation has an unresolved outcome.  The UI
    /// uses this to stop a connection transition after cancelling running
    /// work: switching databases must not hide a newly ambiguous write.
    pub fn governed_writes_disabled(&self, id: Uuid) -> bool {
        if self.audit_state.governed_writes_disabled() || self.unresolved_connection_ids.contains(&id) {
            return true;
        }
        self.connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&id)
            .and_then(|entry| {
                entry
                    .inner
                    .lock()
                    .ok()
                    .map(|inner| inner.audit_state.governed_writes_disabled())
            })
            .unwrap_or(false)
    }

    pub fn ssh_environment(&self) -> SshEnvironment {
        self.ssh_environment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn ssh_environment_for_connection(
        &self,
        saved: &tablepro_storage::SavedConnection,
        principal: tablepro_policy::Principal,
    ) -> SshEnvironment {
        self.ssh_environment()
            .with_audit(tablepro_transport::TransportAuditContext::new(
                self.audit.clone(),
                self.audit_state.clone(),
                principal,
                saved.id,
                saved.name.clone(),
                saved.environment,
                saved.driver_id.clone(),
            ))
    }

    pub fn transport_audit_factory(&self) -> tablepro_transport::TransportAuditFactory {
        tablepro_transport::TransportAuditFactory::new(self.audit.clone(), self.audit_state.clone())
    }

    pub fn candidate_guard_factory(&self) -> CandidateGuardFactory {
        CandidateGuardFactory {
            policy: self
                .policy
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
            approval: self
                .approval
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
            audit: self.audit.clone(),
            audit_state: self.audit_state.clone(),
            unresolved_connection_ids: self.unresolved_connection_ids.clone(),
        }
    }

    pub fn enable_system_openssh(&self, openssh: OpenSshEnvironment) {
        self.ssh_environment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .openssh = Some(openssh);
    }

    pub fn enable_builtin_ssh_prompter(&self, prompter: Arc<dyn tablepro_ssh::openssh::Prompter>) {
        self.ssh_environment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_builtin_prompter(prompter);
    }

    pub fn set_approval_sink(&self, sink: Arc<dyn tablepro_policy::ApprovalSink>) {
        *self.approval.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = sink;
    }

    pub fn reload_policy(&self) -> Result<(), String> {
        let next = load_policy()?;
        *self.policy.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(next);
        tracing::info!("policy reloaded");
        Ok(())
    }

    /// Whether some window already owns the saved connection `id`. Callers
    /// must check this before activating: two windows opening the same
    /// saved connection would otherwise silently overwrite each other's
    /// entry, so a caller that steals a live entry can leave the other
    /// window's connection handle answering for a session it no longer
    /// owns.
    pub fn is_active(&self, id: Uuid) -> bool {
        self.connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(&id)
    }

    /// Register a fully validated connection and give it focus. Callers must
    /// prepare the connection before this method: activation itself does no
    /// I/O and is the final step of a connection switch. Activation is
    /// additive. A caller that means to replace a connection closes the
    /// previous one itself, so ownership is never dropped as a side effect
    /// of opening something else.
    ///
    /// Returns `false` without registering anything when `id` is already
    /// active. `connection` and `tunnel` are dropped in that case -- this
    /// consumes them either way, so refusal alone is enough to close them.
    #[cfg(test)]
    #[must_use]
    pub fn activate(
        &self,
        id: Uuid,
        metadata: ConnectionMetadata,
        connection: Box<dyn Connection>,
        tunnel: Option<Tunnel>,
        read_only: bool,
        params: ReconnectParams,
    ) -> bool {
        self.activate_shared(id, metadata, Arc::from(connection), tunnel, read_only, params)
    }

    pub(crate) fn activate_shared(
        &self,
        id: Uuid,
        metadata: ConnectionMetadata,
        connection: Arc<dyn Connection>,
        tunnel: Option<Tunnel>,
        read_only: bool,
        params: ReconnectParams,
    ) -> bool {
        let mut connections = self
            .connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if connections.contains_key(&id) {
            return false;
        }
        let fault = Arc::new(Notify::new());
        connection.attach_fault_notify(Arc::clone(&fault));
        let arc = connection;
        let environment = metadata.environment;
        let inner = Arc::new(Mutex::new(EntryInner {
            connection: arc,
            tunnel,
            health: ConnectionHealth::Healthy,
            audit_state: Arc::new(
                self.audit_state
                    .new_connection_generation_with_writes_disabled(self.unresolved_connection_ids.contains(&id)),
            ),
        }));
        let cancel = CancellationToken::new();
        let monitor = tokio::spawn(connection_monitor::run(
            inner.clone(),
            params,
            cancel.clone(),
            fault.clone(),
        ));
        let entry = Entry {
            inner,
            metadata,
            read_only,
            environment,
            cancel,
            fault,
            _monitor: monitor,
        };
        connections.insert(id, entry);
        true
    }

    pub fn metadata(&self, id: Uuid) -> Option<ConnectionMetadata> {
        let entries = self
            .connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        entries.get(&id).map(|e| e.metadata.clone())
    }

    pub fn all_connections(&self) -> Vec<ConnectionMetadata> {
        let entries = self
            .connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out: Vec<_> = entries.values().map(|e| e.metadata.clone()).collect();
        out.sort_by_key(|connection| connection.name.to_lowercase());
        out
    }

    /// Policy-gated handle. Raw connections are not exposed; the returned
    /// `Arc<dyn Connection>` is always a [`PolicyGuard`].
    pub fn handle(&self, id: Uuid, principal: Principal) -> Option<Arc<dyn Connection>> {
        self.handle_with_identity(id, principal).map(|(handle, _)| handle)
    }

    pub fn guard_with_identity(
        &self,
        id: Uuid,
        principal: Principal,
    ) -> Option<(Arc<PolicyGuard>, ConnectionIdentity)> {
        self.build_guard_with_identity(id, principal)
            .map(|(guard, identity)| (Arc::new(guard), identity))
    }

    /// The same guarded handle as [`handle`], kept as its concrete type so
    /// a caller can open a scoped bulk insert on it. It is the same
    /// `PolicyGuard`; nothing here reaches the driver underneath.
    pub fn guard(&self, id: Uuid, principal: Principal) -> Option<Arc<PolicyGuard>> {
        self.guard_with_identity(id, principal).map(|(guard, _)| guard)
    }

    fn handle_with_identity(
        &self,
        id: Uuid,
        principal: Principal,
    ) -> Option<(Arc<dyn Connection>, ConnectionIdentity)> {
        let (guard, identity) = self.guard_with_identity(id, principal)?;
        Some((guard as Arc<dyn Connection>, identity))
    }

    fn build_guard_with_identity(&self, id: Uuid, principal: Principal) -> Option<(PolicyGuard, ConnectionIdentity)> {
        let entries = self
            .connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = entries.get(&id)?;
        let inner = entry.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let policy = self
            .policy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let approval = self
            .approval
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let ctx = GuardContext {
            connection_id: entry.metadata.id,
            connection_name: entry.metadata.name.clone(),
            driver_id: entry.metadata.driver_id.clone(),
            environment: entry.environment,
            read_only: entry.read_only,
            principal,
            policy,
            approval,
            audit: self.audit.clone(),
            audit_state: inner.audit_state.clone(),
        };
        let fault = Arc::new(EntryFaultSink {
            connection_id: entry.metadata.id,
            fault: entry.fault.clone(),
        });
        Some((
            PolicyGuard::new(inner.connection.clone(), ctx).with_fault_sink(fault),
            ConnectionIdentity::from_connection(&inner.connection),
        ))
    }

    /// Alias for [`handle`] with the human GUI principal.
    pub fn get(&self, id: Uuid) -> Option<Arc<dyn Connection>> {
        self.handle(id, Principal::human_gui())
    }

    pub fn get_with_identity(&self, id: Uuid) -> Option<(Arc<dyn Connection>, ConnectionIdentity)> {
        self.handle_with_identity(id, Principal::human_gui())
    }

    pub fn identity(&self, id: Uuid) -> Option<ConnectionIdentity> {
        let entries = self
            .connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = entries.get(&id)?;
        let inner = entry.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(ConnectionIdentity::from_connection(&inner.connection))
    }

    pub fn health(&self, id: Uuid) -> Option<ConnectionHealth> {
        let entries = self
            .connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = entries.get(&id)?;
        let inner = entry.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(inner.health.clone())
    }

    pub fn close(&self, id: Uuid) {
        let mut entries = self
            .connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entry) = entries.remove(&id) {
            entry.cancel.cancel();
        }
    }
}

#[cfg(test)]
#[path = "database_service_tests.rs"]
mod tests;
