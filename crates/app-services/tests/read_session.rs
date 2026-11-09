#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use bookie_app_services::database_service::DatabaseService;
use bookie_app_services::driver_error_presentation::DriverFailurePresentation;
use bookie_app_services::read_session::ReadSession;
use bookie_core::{AuthMode, ConnectOptions, DatabaseDriver, DriverRegistry, Environment, TableInfo, Value};
use bookie_storage::SavedConnection;
use uuid::Uuid;

fn saved(path: &std::path::Path) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "session".into(),
        driver_id: "sqlite".into(),
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
    }
}

async fn seeded(path: &std::path::Path) {
    std::fs::File::create(path).unwrap();
    let writer = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: path.to_string_lossy().into_owned(),
            ..ConnectOptions::default()
        })
        .await
        .unwrap();
    writer
        .execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    writer
        .execute("INSERT INTO items VALUES (1, 'a'), (2, 'b'), (3, NULL)")
        .await
        .unwrap();
    writer.close().await.unwrap();
}

async fn session(path: &std::path::Path) -> ReadSession {
    let mut registry = DriverRegistry::new();
    registry.register(Arc::new(drivers_sqlite::SqliteDriver));
    ReadSession::open(
        saved(path),
        Arc::new(registry),
        Arc::new(DatabaseService::new_isolated()),
    )
    .await
    .expect("open")
}

#[tokio::test]
async fn a_session_lists_tables_and_reads_typed_pages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let session = session(&path).await;

    let items = session
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table")
        .clone();
    let first = session.read_page(&items, 0, 2).await.expect("first page");
    let last = session.read_page(&items, 2, 2).await.expect("last page");
    assert_eq!(first.rows.len(), 2);
    assert_eq!(last.rows.len(), 1);
    assert_eq!(last.rows.iter().next().map(|row| row[1].clone()), Some(Value::Null));
}

#[tokio::test]
async fn a_missing_table_is_a_typed_query_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let session = session(&path).await;

    let missing = TableInfo {
        schema: None,
        name: "nope".into(),
    };
    let failure = session.read_page(&missing, 0, 10).await.unwrap_err();
    assert!(
        matches!(failure, DriverFailurePresentation::Query { .. }),
        "{failure:?}"
    );
}
