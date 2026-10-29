use super::*;

#[test]
fn the_major_version_is_read_from_the_driver_version_text() {
    assert_eq!(
        parse_postgres_major("PostgreSQL 16.4 (Debian 16.4-1.pgdg120+2)"),
        Some(16)
    );
    assert_eq!(parse_postgres_major("PostgreSQL 18beta1"), Some(18));
    assert_eq!(parse_postgres_major("MySQL 8.4.2"), None);
}

#[test]
fn only_postgres_connections_with_a_known_version_can_be_backed_up() {
    assert_eq!(postgres_server_major("postgres", Some("PostgreSQL 17.2")).unwrap(), 17);
    for (driver, version) in [
        ("mysql", Some("PostgreSQL 17.2")),
        ("postgres", None),
        ("postgres", Some("odd")),
    ] {
        assert!(matches!(
            postgres_server_major(driver, version),
            Err(DriverError::Unsupported(_))
        ));
    }
}

#[test]
fn a_cancelled_tool_or_tunnel_is_reported_as_cancelled() {
    assert!(matches!(dump_error(DumpError::Cancelled), DriverError::Cancelled));
    assert!(matches!(
        transport_error(TransportError::Cancelled),
        DriverError::Cancelled
    ));
    assert!(matches!(
        dump_error(DumpError::ToolMissing { tool: "pg_dump".into() }),
        DriverError::Unsupported(_)
    ));
}

#[test]
fn a_failed_or_unreadable_tool_run_is_an_internal_error_and_not_an_unsupported_one() {
    let io = std::io::Error::other("disk full");
    assert!(matches!(dump_error(DumpError::Io(io)), DriverError::Internal(_)));
}

#[test]
fn each_job_names_its_tool_and_its_administrative_operation() {
    let backup = BackupJob::Backup {
        output: PathBuf::from("/tmp/out.dump"),
    };
    let restore = BackupJob::Restore {
        input: PathBuf::from("/tmp/in.dump"),
    };
    assert_eq!(backup.tool_name(), "pg_dump");
    assert_eq!(restore.tool_name(), "pg_restore");
    assert_eq!(backup.operation(), AdministrativeOperation::Backup);
    assert_eq!(restore.operation(), AdministrativeOperation::Restore);
}

struct IdleConnection;

#[async_trait::async_trait]
impl tablepro_core::Connection for IdleConnection {
    async fn list_tables(&self) -> Result<Vec<tablepro_core::TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(
        &self,
        _schema: Option<&str>,
        _table: &str,
    ) -> Result<Vec<tablepro_core::ColumnInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_rows(
        &self,
        _schema: Option<&str>,
        _table: &str,
        _offset: u64,
        _limit: u64,
    ) -> Result<tablepro_core::QueryResult, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn query(&self, _sql: &str) -> Result<tablepro_core::QueryResult, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn execute(&self, _sql: &str) -> Result<tablepro_core::ExecResult, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn execute_params(
        &self,
        _sql: &str,
        _params: &[tablepro_core::Value],
    ) -> Result<tablepro_core::ExecResult, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn execute_in_transaction(
        &self,
        _statements: &[(String, Vec<tablepro_core::Value>)],
    ) -> Result<Vec<u64>, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct UnusedDriver;

#[async_trait::async_trait]
impl DatabaseDriver for UnusedDriver {
    fn id(&self) -> &'static str {
        "postgres"
    }

    fn display_name(&self) -> &'static str {
        "Unused"
    }

    fn default_port(&self) -> u16 {
        5432
    }

    async fn connect(&self, _opts: ConnectOptions) -> Result<Box<dyn tablepro_core::Connection>, DriverError> {
        Err(DriverError::Internal("the driver must not be dialled".into()))
    }
}

struct DiscardAudit;

#[async_trait::async_trait]
impl tablepro_policy::AuditSink for DiscardAudit {
    async fn record(&self, _event: tablepro_policy::AuditEvent) -> Result<(), tablepro_policy::AuditError> {
        Ok(())
    }
}

fn guard() -> PolicyGuard {
    PolicyGuard::new(
        Arc::new(IdleConnection),
        tablepro_policy::GuardContext {
            connection_id: uuid::Uuid::new_v4(),
            connection_name: "unit".into(),
            driver_id: "mysql".into(),
            environment: tablepro_core::Environment::Local,
            read_only: false,
            principal: tablepro_policy::Principal::human_gui(),
            policy: Arc::new(tablepro_policy::PolicyConfig::default()),
            approval: Arc::new(tablepro_policy::AutoApproveSink),
            audit: Arc::new(DiscardAudit),
            audit_state: Arc::new(tablepro_policy::AuditState::new()),
        },
    )
}

#[tokio::test]
async fn a_saved_connection_that_is_not_postgres_is_refused_before_any_tool_runs() {
    let saved: SavedConnection = serde_json::from_value(serde_json::json!({
        "id": uuid::Uuid::new_v4(),
        "name": "not postgres",
        "driver_id": "mysql",
        "host": "127.0.0.1",
        "port": 3306,
        "database": "app",
        "username": "root",
    }))
    .unwrap();
    let environment = SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Refuse);
    let runtime = std::env::temp_dir();
    let guard = guard();
    let context = SavedBackupContext {
        guard: &guard,
        driver: &UnusedDriver,
        saved: &saved,
        environment: &environment,
        server_version: Some("PostgreSQL 17.2"),
        runtime_dir: &runtime,
    };
    let control = OperationControl::new(tokio_util::sync::CancellationToken::new(), None);
    let outcome = run_saved_backup_job(
        context,
        BackupJob::Backup {
            output: PathBuf::from("/tmp/never-written.dump"),
        },
        &control,
    )
    .await;
    assert!(matches!(outcome, Err(DriverError::Unsupported(_))), "{outcome:?}");
}
