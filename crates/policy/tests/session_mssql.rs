#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{borrow::Cow, future::Future};

use async_trait::async_trait;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, DriverError, Environment, OperationControl, Value};
use bookie_policy::{
    AuditError, AuditEvent, AuditOperationClass, AuditRecordPhase, AuditSink, AuditState, AuditTerminalStatus,
    AuditTransactionOutcome, AutoApproveSink, GuardContext, PolicyConfig, PolicyGuard, Principal, StatementClass,
    classify,
};
use drivers_mssql::MssqlDriver;
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, Image, ImageExt};
use testcontainers_modules::mssql_server::MssqlServer;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use uuid::Uuid;

const MSSQL_STARTUP_TIMEOUT: Duration = Duration::from_secs(180);
const MSSQL_LOG_LIMIT: u64 = 32 * 1024;

#[derive(Debug, Clone)]
struct PolicyMssqlImage(MssqlServer);

impl Image for PolicyMssqlImage {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn tag(&self) -> &str {
        self.0.tag()
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::Nothing]
    }

    fn env_vars(&self) -> impl IntoIterator<Item = (impl Into<Cow<'_, str>>, impl Into<Cow<'_, str>>)> {
        self.0.env_vars()
    }
}

async fn start_policy_mssql() -> (ContainerAsync<PolicyMssqlImage>, Arc<dyn Connection>) {
    let image = PolicyMssqlImage(MssqlServer::default().with_accept_eula()).with_startup_timeout(MSSQL_STARTUP_TIMEOUT);
    let deadline = tokio::time::Instant::now() + MSSQL_STARTUP_TIMEOUT;
    let container = tokio::time::timeout_at(deadline, image.start())
        .await
        .unwrap_or_else(|_| panic!("SQL Server container creation exceeded 180 seconds"))
        .unwrap_or_else(|error| panic!("SQL Server container creation failed: {error}"));
    let host = container
        .get_host()
        .await
        .unwrap_or_else(|error| panic!("SQL Server host lookup failed: {error}"))
        .to_string();
    let port = container
        .get_host_port_ipv4(1433)
        .await
        .unwrap_or_else(|error| panic!("SQL Server port lookup failed: {error}"));
    let options = ConnectOptions {
        host: host.clone(),
        port,
        database: "master".into(),
        username: "sa".into(),
        password: secrecy::SecretString::new(MssqlServer::DEFAULT_SA_PASSWORD.to_string().into()),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    };
    if let Err(error) = wait_for_log_markers(&container, deadline).await {
        panic!(
            "SQL Server log readiness failed: {error}\n{}",
            container_diagnostics(&container).await
        );
    }
    if let Err(error) = wait_for_tcp(&host, port, deadline).await {
        panic!(
            "SQL Server TCP readiness failed: {error}\n{}",
            container_diagnostics(&container).await
        );
    }
    let connection = wait_for_authenticated_mssql(deadline, || async {
        let connection = MssqlDriver.connect(options.clone()).await?;
        connection.query("SELECT 1").await?;
        Ok(Arc::from(connection) as Arc<dyn Connection>)
    })
    .await;
    match connection {
        Ok(connection) => (container, connection),
        Err(error) => panic!(
            "SQL Server authenticated readiness failed: {error}\n{}",
            container_diagnostics(&container).await
        ),
    }
}

async fn wait_for_tcp(host: &str, port: u16, deadline: tokio::time::Instant) -> Result<(), DriverError> {
    loop {
        match tokio::time::timeout_at(deadline, TcpStream::connect((host, port))).await {
            Err(_) => return Err(DriverError::TimedOut),
            Ok(Ok(stream)) => {
                drop(stream);
                return Ok(());
            }
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    return Err(DriverError::TimedOut);
                }
                tokio::time::sleep(Duration::from_millis(250).min(remaining)).await;
            }
            Ok(Err(_)) => return Err(DriverError::ConnectionRefused),
        }
    }
}

async fn wait_for_log_markers(
    container: &ContainerAsync<PolicyMssqlImage>,
    deadline: tokio::time::Instant,
) -> Result<(), DriverError> {
    loop {
        let stdout = read_log_prefix(container.stdout(false)).await;
        if has_mssql_readiness_markers(&stdout) {
            return Ok(());
        }
        if matches!(container.is_running().await, Ok(false)) {
            return Err(DriverError::ConnectionRefused);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(DriverError::TimedOut);
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        tokio::time::sleep(Duration::from_millis(500).min(remaining)).await;
    }
}

fn has_mssql_readiness_markers(stdout: &str) -> bool {
    stdout.contains("SQL Server is now ready for client connections") && stdout.contains("Recovery is complete")
}

async fn wait_for_authenticated_mssql<F, Fut, T>(
    deadline: tokio::time::Instant,
    mut attempt: F,
) -> Result<T, DriverError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, DriverError>>,
{
    loop {
        match tokio::time::timeout_at(deadline, attempt()).await {
            Err(_) => return Err(DriverError::TimedOut),
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(DriverError::AuthFailed)) => {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    return Err(DriverError::TimedOut);
                }
                tokio::time::sleep(Duration::from_millis(500).min(remaining)).await;
            }
            Ok(Err(error)) => return Err(error),
        }
    }
}

async fn container_diagnostics(container: &ContainerAsync<PolicyMssqlImage>) -> String {
    let running = container.is_running().await;
    let exit_code = container.exit_code().await;
    let stdout = read_log_prefix(container.stdout(false)).await;
    let stderr = read_log_prefix(container.stderr(false)).await;
    let marker_status = format!(
        "SQL Server is now ready for client connections={}, Recovery is complete={}",
        stdout.contains("SQL Server is now ready for client connections"),
        stdout.contains("Recovery is complete")
    );
    let state = inspect_container_state(container.id()).await;
    format!(
        "container={} running={running:?} exit_code={exit_code:?} inspect={state}\nreadiness markers in captured stdout: {marker_status}\nstdout prefix:\n{stdout}\nstderr prefix:\n{stderr}",
        container.id()
    )
}

async fn read_log_prefix(reader: impl tokio::io::AsyncRead + Unpin) -> String {
    let mut bytes = Vec::new();
    let _ = reader.take(MSSQL_LOG_LIMIT).read_to_end(&mut bytes).await;
    redact_mssql_secret(&String::from_utf8_lossy(&bytes))
}

fn redact_mssql_secret(log: &str) -> String {
    log.replace(MssqlServer::DEFAULT_SA_PASSWORD, "[redacted]")
}

async fn inspect_container_state(container_id: &str) -> String {
    let result = tokio::process::Command::new("docker")
        .args([
            "inspect",
            "--format={{.State.Status}} oom={{.State.OOMKilled}} exit={{.State.ExitCode}} error={{.State.Error}}",
            container_id,
        ])
        .output()
        .await;
    match result {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        Ok(output) => format!("unavailable (docker inspect status {})", output.status),
        Err(_) => "unavailable (docker inspect command not found or failed)".to_string(),
    }
}

#[derive(Default)]
struct CapturingAudit(Mutex<Vec<AuditEvent>>);

#[async_trait]
impl AuditSink for CapturingAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.0.lock().expect("audit event lock").push(event);
        Ok(())
    }
}

async fn run_session(
    session: &mut Box<dyn bookie_core::Session>,
    sql: &str,
) -> Result<bookie_core::QueryResult, DriverError> {
    session
        .query_params_controlled(sql, &[], &OperationControl::with_timeout(Duration::from_secs(10)))
        .await
}

#[tokio::test]
#[ignore = "requires docker"]
async fn sql_server_tran_forms_are_refused_shared_and_tracked_in_sessions() {
    let (_container, connection) = start_policy_mssql().await;
    let table = format!("dbo.b4_transaction_{}", Uuid::new_v4().simple());
    connection
        .execute(&format!(
            "CREATE TABLE {table} (id int PRIMARY KEY, value int NOT NULL)"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!("INSERT INTO {table} VALUES (1, 10)"))
        .await
        .unwrap();
    let audit = Arc::new(CapturingAudit::default());
    let guard = PolicyGuard::new(
        connection.clone(),
        GuardContext {
            connection_id: Uuid::new_v4(),
            connection_name: "SQL Server transaction-control fixture".into(),
            driver_id: "mssql".into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit: audit.clone(),
            audit_state: Arc::new(AuditState::new()),
        },
    );

    assert!(matches!(
        guard.query("BEGIN TRAN").await,
        Err(DriverError::PolicyDenied(_))
    ));
    let mut session = guard.open_session().await.expect("open session");
    run_session(&mut session, "BEGIN TRAN").await.expect("begin tran");
    let count = run_session(&mut session, "SELECT @@TRANCOUNT")
        .await
        .expect("transaction count");
    assert_eq!(count.rows, vec![vec![Value::Int(1)]]);
    run_session(&mut session, &format!("UPDATE {table} SET value = 20 WHERE id = 1"))
        .await
        .expect("update within session transaction");
    run_session(&mut session, "ROLLBACK TRAN").await.expect("rollback tran");
    session.close().await.expect("close session");

    let state = connection
        .query(&format!("SELECT value FROM {table} WHERE id = 1"))
        .await
        .unwrap();
    assert_eq!(state.rows, vec![vec![Value::Int(10)]]);
    let outcomes = audit.0.lock().expect("audit events");
    assert!(outcomes.iter().any(|event| {
        event.phase == AuditRecordPhase::Outcome
            && event.operation_class == AuditOperationClass::TransactionRollback
            && event.transaction_outcome == AuditTransactionOutcome::RolledBack
            && event.terminal_status == AuditTerminalStatus::Succeeded
    }));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn sql_server_batches_with_mutations_are_classified_and_denied_before_dispatch() {
    let (_container, connection) = start_policy_mssql().await;
    connection
        .execute(
            "CREATE TABLE dbo.batch_target (id int PRIMARY KEY, value int NOT NULL); \
             CREATE TABLE dbo.batch_source (id int PRIMARY KEY, value int NOT NULL); \
             INSERT INTO dbo.batch_target VALUES (1, 10); \
             INSERT INTO dbo.batch_source VALUES (1, 99)",
        )
        .await
        .unwrap();

    let cases = [
        (
            "SELECT 1; UPDATE dbo.batch_target SET value = 20 WHERE id = 1",
            StatementClass::Update,
        ),
        ("SELECT 1; DROP TABLE dbo.batch_target", StatementClass::Ddl),
        (
            "MERGE dbo.batch_target AS target USING dbo.batch_source AS source \
             ON target.id = source.id WHEN MATCHED THEN UPDATE SET target.value = source.value;",
            StatementClass::Other,
        ),
        (
            "SELECT 1; MERGE dbo.batch_target AS target USING dbo.batch_source AS source \
             ON target.id = source.id WHEN MATCHED THEN UPDATE SET target.value = source.value;",
            StatementClass::Other,
        ),
    ];
    let audit = Arc::new(CapturingAudit::default());
    let guard = PolicyGuard::new(
        connection.clone(),
        GuardContext {
            connection_id: Uuid::new_v4(),
            connection_name: "SQL Server SEC-4 fixture".into(),
            driver_id: "mssql".into(),
            environment: Environment::Local,
            read_only: true,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit: audit.clone(),
            audit_state: Arc::new(AuditState::new()),
        },
    );

    for &(sql, expected_class) in &cases {
        let facts = classify(sql, "mssql");
        assert_eq!(facts.class, expected_class, "classification for {sql}");
        assert!(facts.writes(), "batch must require write capability: {sql}");
        assert!(
            facts.is_multi_statement || sql.starts_with("MERGE"),
            "batch flag for {sql}"
        );
        assert_ne!(
            facts.class,
            StatementClass::Unparseable,
            "SQL Server syntax must parse: {sql}"
        );
        assert!(
            matches!(guard.query(sql).await, Err(DriverError::PolicyDenied(_))),
            "read-only guard must deny before dispatch: {sql}"
        );
        let state = connection
            .query(
                "SELECT CASE WHEN OBJECT_ID(N'dbo.batch_target') IS NULL THEN 0 ELSE 1 END, COUNT(*), MAX(value) \
                 FROM dbo.batch_target",
            )
            .await
            .expect("target remains queryable after denial");
        assert_eq!(
            state.rows,
            vec![vec![
                bookie_core::Value::Int(1),
                bookie_core::Value::Int(1),
                bookie_core::Value::Int(10)
            ]]
        );
    }

    let outcomes = audit
        .0
        .lock()
        .expect("audit event lock")
        .iter()
        .filter(|event| event.phase == AuditRecordPhase::Outcome)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(outcomes.len(), cases.len());
    assert!(
        outcomes
            .iter()
            .all(|event| event.terminal_status == AuditTerminalStatus::Denied)
    );
}

#[tokio::test(start_paused = true)]
async fn mssql_startup_retries_authentication_until_system_databases_are_ready() {
    let mut attempts = 0;
    wait_for_authenticated_mssql(tokio::time::Instant::now() + MSSQL_STARTUP_TIMEOUT, || {
        attempts += 1;
        async move {
            if attempts < 3 {
                Err(DriverError::AuthFailed)
            } else {
                Ok(())
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(attempts, 3);
}

#[tokio::test(start_paused = true)]
async fn mssql_startup_does_not_retry_terminal_connection_errors() {
    let mut attempts = 0;
    let error = wait_for_authenticated_mssql(tokio::time::Instant::now() + MSSQL_STARTUP_TIMEOUT, || {
        attempts += 1;
        async { Err::<(), _>(DriverError::ConnectionRefused) }
    })
    .await
    .unwrap_err();
    assert!(matches!(error, DriverError::ConnectionRefused));
    assert_eq!(attempts, 1);
}

#[tokio::test(start_paused = true)]
async fn mssql_startup_retry_is_bounded_by_the_existing_deadline() {
    let error = wait_for_authenticated_mssql(tokio::time::Instant::now() + Duration::from_secs(1), || async {
        Err::<(), _>(DriverError::AuthFailed)
    })
    .await
    .unwrap_err();
    assert!(matches!(error, DriverError::TimedOut));
}

#[test]
fn mssql_readiness_requires_both_system_database_markers() {
    assert!(!has_mssql_readiness_markers(
        "SQL Server is now ready for client connections"
    ));
    assert!(!has_mssql_readiness_markers("Recovery is complete"));
    assert!(has_mssql_readiness_markers(
        "SQL Server is now ready for client connections\nRecovery is complete"
    ));
}

#[test]
fn mssql_startup_log_redacts_the_fixture_password() {
    let log = format!("MSSQL_SA_PASSWORD={}", MssqlServer::DEFAULT_SA_PASSWORD);
    assert_eq!(redact_mssql_secret(&log), "MSSQL_SA_PASSWORD=[redacted]");
}
