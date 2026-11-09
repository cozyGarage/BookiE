#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A frontend without GTK opens a saved connection and reads through the
//! policy guard, the path ADR 0043 gives the GPUI experiment.

use std::sync::Arc;

use bookie_app_services::connection_service::open_saved;
use bookie_app_services::database_service::DatabaseService;
use bookie_app_services::open_failure::OpenFailure;
use bookie_core::{AuthMode, Connection, DriverRegistry, Environment, Value};
use bookie_policy::Principal;
use bookie_storage::SavedConnection;
use bookie_transport::SshEnvironment;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn saved(path: &std::path::Path, driver_id: &str) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "headless".into(),
        driver_id: driver_id.into(),
        host: String::new(),
        port: 0,
        socket_dir: None,
        socket_path: None,
        database: path.to_string_lossy().into_owned(),
        username: String::new(),
        use_tls: false,
        tls_mode: None,
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: true,
        auth_mode: AuthMode::Password,
        environment: Environment::Dev,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        attached_databases: Vec::new(),
        tls_strict_encryption: false,
    }
}

fn registry() -> Arc<DriverRegistry> {
    let mut registry = DriverRegistry::new();
    registry.register(Arc::new(drivers_sqlite::SqliteDriver));
    Arc::new(registry)
}

async fn open(
    database: &DatabaseService,
    saved: SavedConnection,
) -> Result<bookie_app_services::connection_service::PreparedConnection, OpenFailure> {
    open_saved(
        registry(),
        saved,
        Vec::new(),
        30,
        SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Refuse),
        CancellationToken::new(),
        database.candidate_guard_factory(),
    )
    .await
}

#[tokio::test]
async fn a_saved_connection_opens_lists_its_tables_and_reads_through_the_guard_with_audit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("headless.db");
    seed(&path).await;
    let database = DatabaseService::new_isolated();

    let prepared = open(&database, saved(&path, "sqlite")).await.expect("open");
    assert!(
        prepared.tables.iter().any(|table| table.name == "items"),
        "{:?}",
        prepared.tables
    );
    let id = prepared.id();
    let activated = prepared.activate(&database).expect("activate");
    assert_eq!(activated.driver_id, "sqlite");

    let guard = database.guard(id, Principal::human_gui()).expect("guard");
    let page = guard.fetch_rows(None, "items", 0, 10).await.expect("page");
    let ids: Vec<Value> = page.rows.iter().map(|row| row[0].clone()).collect();
    assert_eq!(ids, vec![Value::Int(1), Value::Int(2)]);

    let audit = std::fs::read_to_string(database.audit_journal_path_for_test()).expect("audit journal");
    assert!(audit.contains("items"), "the read must be recorded: {audit}");
}

#[tokio::test]
async fn an_unregistered_driver_is_a_typed_failure_not_text() {
    let database = DatabaseService::new_isolated();
    let failure = open(&database, saved(std::path::Path::new("x"), "nope"))
        .await
        .unwrap_err();
    assert_eq!(failure, OpenFailure::Other("driver nope not registered".into()));
}

async fn seed(path: &std::path::Path) {
    use bookie_core::{ConnectOptions, DatabaseDriver};
    std::fs::File::create(path).unwrap();
    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: path.to_string_lossy().into_owned(),
            ..ConnectOptions::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO items VALUES (1, 'a'), (2, 'b')")
        .await
        .unwrap();
    connection.close().await.unwrap();
}
