use super::*;
use drivers_sqlite::SqliteDriver;
use std::sync::atomic::{AtomicUsize, Ordering};
use tablepro_core::{ConnectOptions, DatabaseDriver, DriverError, Environment};
use tablepro_policy::{DenyApprovalSink, NullAuditSink};
use tablepro_storage::SavedSshAuth;

#[test]
fn the_agent_never_adds_an_unknown_ssh_host_key() {
    assert_eq!(AGENT_UNKNOWN_HOST_KEY, tablepro_ssh::UnknownHostKey::Refuse);
}

fn saved_connection() -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "Warehouse".into(),
        driver_id: "postgres".into(),
        host: "db.example".into(),
        port: 5432,
        socket_dir: None,
        socket_path: None,
        database: "warehouse".into(),
        username: "reader".into(),
        use_tls: true,
        tls_mode: Some(TlsMode::VerifyFull),
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: true,
        auth_mode: AuthMode::Password,
        environment: Environment::Prod,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
    }
}

struct ProbeConnection {
    queries: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl Drop for ProbeConnection {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait]
impl Connection for ProbeConnection {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(&self, _schema: Option<&str>, _table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_rows(
        &self,
        _schema: Option<&str>,
        _table: &str,
        _offset: u64,
        _limit: u64,
    ) -> Result<QueryResult, DriverError> {
        Err(DriverError::Internal("unused probe operation".into()))
    }

    async fn query(&self, _sql: &str) -> Result<QueryResult, DriverError> {
        self.queries.fetch_add(1, Ordering::SeqCst);
        Err(DriverError::Internal("probe query dispatched".into()))
    }

    async fn execute(&self, _sql: &str) -> Result<ExecResult, DriverError> {
        Err(DriverError::Internal("unused probe operation".into()))
    }

    async fn execute_params(&self, _sql: &str, _params: &[Value]) -> Result<ExecResult, DriverError> {
        Err(DriverError::Internal("unused probe operation".into()))
    }

    async fn execute_in_transaction(&self, _statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Err(DriverError::Internal("unused probe operation".into()))
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

#[test]
fn session_key_changes_when_saved_transport_changes() {
    let original = saved_connection();
    let original_key = SessionKey::from_saved(&original, [0; 32]);
    let mut changed = original.clone();
    changed.host = "replacement.example".into();

    assert!(original_key != SessionKey::from_saved(&changed, [0; 32]));

    changed = original.clone();
    changed.socket_dir = Some(PathBuf::from("/run/postgresql"));

    assert!(original_key != SessionKey::from_saved(&changed, [0; 32]));

    changed = original.clone();
    changed.socket_path = Some(PathBuf::from("/run/mysqld/mysqld.sock"));

    assert!(original_key != SessionKey::from_saved(&changed, [0; 32]));
}

#[test]
fn session_key_ignores_policy_only_changes() {
    let original = saved_connection();
    let original_key = SessionKey::from_saved(&original, [0; 32]);
    let mut changed = original.clone();
    changed.read_only = false;
    changed.environment = Environment::Dev;

    assert!(original_key == SessionKey::from_saved(&changed, [0; 32]));
}

#[test]
fn session_key_changes_when_material_changes() {
    let saved = saved_connection();
    let original_key = SessionKey::from_saved(&saved, [1; 32]);

    assert!(original_key != SessionKey::from_saved(&saved, [2; 32]));
}

#[tokio::test]
async fn cached_ping_is_bounded() {
    let ping = std::future::pending::<Result<(), DriverError>>();

    assert!(!ping_is_healthy(ping, Duration::from_millis(1)).await);
}

#[tokio::test]
async fn retired_session_drops_after_final_issued_reference() {
    let saved = saved_connection();
    let key = SessionKey::from_saved(&saved, [0; 32]);
    let mut opts = ConnectOptions {
        database: ":memory:".into(),
        ..Default::default()
    };
    opts.tls.mode = TlsMode::Disabled;
    let raw: Arc<dyn Connection> = Arc::from(SqliteDriver.connect(opts).await.expect("open test database"));
    let raw_lifetime = Arc::downgrade(&raw);
    let connection: Arc<dyn Connection> = Arc::new(SessionConnection {
        inner: raw,
        _tunnel: None,
    });
    let provider = DaemonProvider::new(
        Arc::new(DriverRegistry::new()),
        Arc::new(PolicyConfig::default()),
        Arc::new(NullAuditSink),
        Arc::new(AuditState::new()),
        Arc::new(DenyApprovalSink),
    );
    provider.sessions.lock().expect("sessions").insert(
        saved.id,
        OpenSession {
            key: key.clone(),
            connection: connection.clone(),
            audit_state: Arc::new(AuditState::new()),
            retired: Default::default(),
        },
    );
    let issued = connection.clone();

    provider
        .remove_session(saved.id, &key, &connection)
        .expect("remove cached session");
    drop(connection);

    assert!(raw_lifetime.upgrade().is_some());

    drop(issued);

    assert!(raw_lifetime.upgrade().is_none());
}

#[tokio::test]
async fn cached_session_is_dropped_when_material_changes() {
    let saved = saved_connection();
    let old_key = SessionKey::from_saved(&saved, [1; 32]);
    let new_key = SessionKey::from_saved(&saved, [2; 32]);
    let mut opts = ConnectOptions {
        database: ":memory:".into(),
        ..Default::default()
    };
    opts.tls.mode = TlsMode::Disabled;
    let connection: Arc<dyn Connection> = Arc::from(SqliteDriver.connect(opts).await.expect("open test database"));
    let provider = DaemonProvider::new(
        Arc::new(DriverRegistry::new()),
        Arc::new(PolicyConfig::default()),
        Arc::new(NullAuditSink),
        Arc::new(AuditState::new()),
        Arc::new(DenyApprovalSink),
    );
    provider.sessions.lock().expect("sessions").insert(
        saved.id,
        OpenSession {
            key: old_key,
            connection: connection.clone(),
            audit_state: Arc::new(AuditState::new()),
            retired: Default::default(),
        },
    );

    let cached = provider
        .cached_connection(saved.id, &new_key)
        .expect("inspect cache after material rotation");
    assert!(cached.is_none());
    assert!(provider.sessions.lock().expect("sessions").get(&saved.id).is_none());
}

struct RotatingCaDriver {
    ca_path: PathBuf,
    rotated_bytes: &'static [u8],
}

#[async_trait]
impl DatabaseDriver for RotatingCaDriver {
    fn id(&self) -> &'static str {
        "sqlite"
    }

    fn display_name(&self) -> &'static str {
        "Rotating CA test driver"
    }

    fn default_port(&self) -> u16 {
        0
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        let (path, bytes) = (self.ca_path.clone(), self.rotated_bytes);
        tokio::task::spawn_blocking(move || std::fs::write(path, bytes))
            .await
            .expect("the rotation task is not cancelled")
            .expect("rotate the ca file mid-connect");
        SqliteDriver.connect(opts).await
    }
}

#[tokio::test]
async fn a_key_material_rotation_during_connect_is_not_cached_under_the_stale_digest() {
    let directory = tempfile::tempdir().expect("temp directory");
    let ca_path = directory.path().join("ca.pem");
    std::fs::write(&ca_path, b"material-before-connect").expect("write initial ca material");

    let mut saved = saved_connection();
    saved.driver_id = "sqlite".into();
    saved.database = ":memory:".into();
    saved.tls_root_cert = Some(ca_path.clone());
    saved.ssh = None;

    let mut registry = DriverRegistry::new();
    registry.register(Arc::new(RotatingCaDriver {
        ca_path: ca_path.clone(),
        rotated_bytes: b"material-after-connect",
    }));
    let provider = DaemonProvider::new(
        Arc::new(registry),
        Arc::new(PolicyConfig::default()),
        Arc::new(NullAuditSink),
        Arc::new(AuditState::new()),
        Arc::new(DenyApprovalSink),
    );

    provider
        .open_session(&saved)
        .await
        .expect("open session across rotation");

    let post_connect_material = tablepro_transport::session_material_digest(&saved)
        .await
        .expect("digest after the rotation");
    let current_key = SessionKey::from_saved(&saved, post_connect_material);
    let sessions = provider.sessions.lock().expect("sessions");
    let cached = &sessions.get(&saved.id).expect("a session was cached").key;
    assert!(
        *cached == current_key,
        "the cached session key must reflect the material actually used to connect, \
             not a digest read before the key rotated"
    );
}

#[tokio::test]
async fn a_material_lookup_failure_does_not_reuse_a_healthy_cached_connection() {
    let mut saved = saved_connection();
    saved.driver_id = "sqlite".into();
    saved.ssh = Some(SavedSshConfig {
        hop_id: uuid::Uuid::new_v4(),
        credential_revision: 0,
        host: "bastion".into(),
        port: 22,
        username: "tunnel".into(),
        auth: SavedSshAuth::PrivateKey {
            path: "/nonexistent/tablepro-test-key".into(),
            has_passphrase: false,
        },
        jump: None,
        client: Default::default(),
        agent: false,
    });
    let key = SessionKey::from_saved(&saved, [0; 32]);
    let queries = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let connection: Arc<dyn Connection> = Arc::new(ProbeConnection {
        queries: queries.clone(),
        drops: drops.clone(),
    });
    let provider = DaemonProvider::new(
        Arc::new(DriverRegistry::new()),
        Arc::new(PolicyConfig::default()),
        Arc::new(NullAuditSink),
        Arc::new(AuditState::new()),
        Arc::new(DenyApprovalSink),
    );
    provider.sessions.lock().expect("sessions").insert(
        saved.id,
        OpenSession {
            key,
            connection: connection.clone(),
            audit_state: Arc::new(AuditState::new()),
            retired: Default::default(),
        },
    );

    let result = provider.open_session(&saved).await;
    if let Ok(connection) = &result {
        let _ = connection.query("SELECT 1").await;
    }
    assert!(result.is_err());
    assert_eq!(queries.load(Ordering::SeqCst), 0);
    assert!(provider.sessions.lock().expect("sessions").contains_key(&saved.id));
}

struct RemovingCaDriver {
    ca_path: PathBuf,
    queries: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

#[async_trait]
impl DatabaseDriver for RemovingCaDriver {
    fn id(&self) -> &'static str {
        "sqlite"
    }

    fn display_name(&self) -> &'static str {
        "Removing CA test driver"
    }

    fn default_port(&self) -> u16 {
        0
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        let path = self.ca_path.clone();
        tokio::task::spawn_blocking(move || std::fs::remove_file(path))
            .await
            .expect("the removal task is not cancelled")
            .expect("remove ca material during connect");
        let _ = opts;
        Ok(Box::new(ProbeConnection {
            queries: self.queries.clone(),
            drops: self.drops.clone(),
        }))
    }
}

#[tokio::test]
async fn a_material_lookup_failure_after_connect_discards_the_new_connection() {
    let directory = tempfile::tempdir().expect("temp directory");
    let ca_path = directory.path().join("ca.pem");
    std::fs::write(&ca_path, b"material-before-connect").expect("write initial ca material");
    let mut saved = saved_connection();
    saved.driver_id = "sqlite".into();
    saved.database = ":memory:".into();
    saved.tls_root_cert = Some(ca_path.clone());
    saved.ssh = None;

    let mut registry = DriverRegistry::new();
    let queries = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    registry.register(Arc::new(RemovingCaDriver {
        ca_path,
        queries: queries.clone(),
        drops: drops.clone(),
    }));
    let provider = DaemonProvider::new(
        Arc::new(registry),
        Arc::new(PolicyConfig::default()),
        Arc::new(NullAuditSink),
        Arc::new(AuditState::new()),
        Arc::new(DenyApprovalSink),
    );

    assert!(provider.open_session(&saved).await.is_err());
    assert_eq!(queries.load(Ordering::SeqCst), 0);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(provider.sessions.lock().expect("sessions").get(&saved.id).is_none());
}

#[tokio::test]
async fn a_healthy_cached_connection_is_reused_when_material_is_verified() {
    let mut saved = saved_connection();
    saved.driver_id = "sqlite".into();
    saved.database = ":memory:".into();
    saved.tls_root_cert = None;
    saved.ssh = None;
    let material = tablepro_transport::session_material_digest(&saved)
        .await
        .expect("digest valid material");
    let key = SessionKey::from_saved(&saved, material);
    let mut opts = ConnectOptions {
        database: ":memory:".into(),
        ..Default::default()
    };
    opts.tls.mode = TlsMode::Disabled;
    let connection: Arc<dyn Connection> = Arc::from(SqliteDriver.connect(opts).await.expect("open test database"));
    let provider = DaemonProvider::new(
        Arc::new(DriverRegistry::new()),
        Arc::new(PolicyConfig::default()),
        Arc::new(NullAuditSink),
        Arc::new(AuditState::new()),
        Arc::new(DenyApprovalSink),
    );
    provider.sessions.lock().expect("sessions").insert(
        saved.id,
        OpenSession {
            key,
            connection: connection.clone(),
            audit_state: Arc::new(AuditState::new()),
            retired: Default::default(),
        },
    );

    let reused = provider
        .open_session(&saved)
        .await
        .expect("verified healthy connection is reusable");
    assert!(Arc::ptr_eq(&reused, &connection));
}
