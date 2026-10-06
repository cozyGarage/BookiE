use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::*;
use async_trait::async_trait;
use tablepro_core::{AuthMode, ColumnInfo, Environment, ExecResult, QueryResult, TableInfo, TlsMode};
use tablepro_policy::{ApprovalOutcome, ApprovalRequest, AuditError, AuditEvent};

struct WriteProbe {
    hang: bool,
    panic: bool,
    executed: Arc<AtomicUsize>,
}

#[async_trait]
impl Connection for WriteProbe {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn fetch_columns(&self, _: Option<&str>, _: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok(vec![])
    }

    async fn fetch_rows(&self, _: Option<&str>, _: &str, _: u64, _: u64) -> Result<QueryResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn query(&self, _: &str) -> Result<QueryResult, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn execute(&self, _: &str) -> Result<ExecResult, DriverError> {
        self.executed.fetch_add(1, Ordering::SeqCst);
        if self.panic {
            panic!("driver bug");
        }
        if self.hang {
            std::future::pending::<()>().await;
        }
        Ok(ExecResult { rows_affected: 1 })
    }

    async fn execute_params(&self, sql: &str, _: &[Value]) -> Result<ExecResult, DriverError> {
        self.execute(sql).await
    }

    async fn execute_in_transaction(&self, _: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Err(DriverError::Unsupported("unused".into()))
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct AcceptingAudit;

#[async_trait]
impl tablepro_policy::AuditSink for AcceptingAudit {
    async fn record(&self, _event: AuditEvent) -> Result<(), AuditError> {
        Ok(())
    }
}

struct AllowAll;

#[async_trait]
impl tablepro_policy::ApprovalSink for AllowAll {
    async fn request(&self, _request: ApprovalRequest) -> ApprovalOutcome {
        ApprovalOutcome::AllowOnce
    }
}

fn provider() -> DaemonProvider {
    DaemonProvider::new(
        Arc::new(DriverRegistry::new()),
        Arc::new(PolicyConfig::default()),
        Arc::new(AcceptingAudit),
        Arc::new(AuditState::new()),
        Arc::new(AllowAll),
    )
}

fn local_connection() -> SavedConnection {
    SavedConnection {
        id: Uuid::new_v4(),
        name: "Local".into(),
        driver_id: "postgres".into(),
        host: "localhost".into(),
        port: 5432,
        socket_dir: None,
        database: "app".into(),
        username: "dev".into(),
        use_tls: false,
        tls_mode: Some(TlsMode::Disabled),
        tls_root_cert: None,
        read_only: false,
        auth_mode: AuthMode::Password,
        environment: Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
    }
}

fn install_session(
    provider: &DaemonProvider,
    saved: &SavedConnection,
    hang: bool,
) -> (Arc<dyn Connection>, Arc<AtomicUsize>) {
    let executed = Arc::new(AtomicUsize::new(0));
    let connection: Arc<dyn Connection> = Arc::new(WriteProbe {
        hang,
        panic: false,
        executed: executed.clone(),
    });
    let session = provider.new_session(SessionKey::from_saved(saved, [0; 32]), connection.clone());
    provider.sessions.lock().expect("sessions").insert(saved.id, session);
    (connection, executed)
}

#[tokio::test]
async fn an_interrupted_write_blocks_only_its_own_connection_and_a_replacement_recovers() {
    let provider = provider();
    let (a, b) = (local_connection(), local_connection());
    let (raw_a, executed_a) = install_session(&provider, &a, true);
    let (raw_b, _) = install_session(&provider, &b, false);
    let guard_a = provider.guarded(&a, Principal::human_gui(), raw_a);
    let guard_b = provider.guarded(&b, Principal::human_gui(), raw_b);

    let interrupted =
        tokio::time::timeout(Duration::from_millis(20), guard_a.execute("INSERT INTO t VALUES (1)")).await;
    assert!(interrupted.is_err(), "the write must hang until it is dropped");
    assert_eq!(executed_a.load(Ordering::SeqCst), 1);

    guard_a
        .execute("INSERT INTO t VALUES (2)")
        .await
        .expect_err("a connection with an uncertain write must refuse more governed writes");
    assert_eq!(
        executed_a.load(Ordering::SeqCst),
        1,
        "the refused write must not reach the driver"
    );

    guard_b
        .execute("INSERT INTO t VALUES (3)")
        .await
        .expect("another connection stays writable");

    provider.sessions.lock().expect("sessions").remove(&a.id);
    let (replacement, _) = install_session(&provider, &a, false);
    let guard_replacement = provider.guarded(&a, Principal::human_gui(), replacement);
    guard_replacement
        .execute("INSERT INTO t VALUES (4)")
        .await
        .expect("a replacement session starts with a clean audit state");
}

#[tokio::test]
async fn a_session_whose_driver_panicked_is_not_reused_even_though_its_ping_is_healthy() {
    let provider = provider();
    let saved = local_connection();
    let connection: Arc<dyn Connection> = Arc::new(WriteProbe {
        hang: false,
        panic: true,
        executed: Arc::new(AtomicUsize::new(0)),
    });
    let material = tablepro_transport::session_material_digest(&saved)
        .await
        .expect("material digest");
    let session = provider.new_session(SessionKey::from_saved(&saved, material), connection.clone());
    provider.sessions.lock().expect("sessions").insert(saved.id, session);

    let reused = provider
        .open_session(&saved)
        .await
        .expect("a healthy session is reused before the fault");
    assert!(Arc::ptr_eq(&reused, &connection));

    let guard = provider.guarded(&saved, Principal::human_gui(), connection);
    guard
        .execute("INSERT INTO t VALUES (1)")
        .await
        .expect_err("a panicking driver call is contained as an error");

    assert!(
        provider.open_session(&saved).await.is_err(),
        "the faulted session must be retired and a fresh connection attempted (none is registered here)"
    );
    assert!(provider.sessions.lock().expect("sessions").get(&saved.id).is_none());
}
