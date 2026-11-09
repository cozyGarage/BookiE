#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use bookie_app_services::database_service::DatabaseService;
use bookie_app_services::driver_error_presentation::DriverFailurePresentation;
use bookie_app_services::read_session::ReadSession;
use bookie_core::{
    AuthMode, ConnectOptions, DatabaseDriver, DriverRegistry, Environment, OperationControl, TableInfo, Value,
};
use bookie_storage::SavedConnection;
use uuid::Uuid;

fn saved(path: &std::path::Path, id: Uuid) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id,
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

async fn session(path: &std::path::Path, id: Uuid, database: Arc<DatabaseService>) -> ReadSession {
    let mut registry = DriverRegistry::new();
    registry.register(Arc::new(drivers_sqlite::SqliteDriver));
    ReadSession::open(saved(path, id), Arc::new(registry), database)
        .await
        .expect("open")
}

#[tokio::test]
async fn a_session_lists_tables_and_reads_typed_pages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let id = Uuid::new_v4();
    let database = Arc::new(DatabaseService::new_isolated());
    let session = session(&path, id, database.clone()).await;

    let items = session
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table")
        .clone();
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(5));
    let first = session.read_page(&items, 0, 2, &control).await.expect("first page");
    let last = session.read_page(&items, 2, 2, &control).await.expect("last page");
    assert_eq!(first.rows.len(), 2);
    assert_eq!(last.rows.len(), 1);
    assert_eq!(last.rows.iter().next().map(|row| row[1].clone()), Some(Value::Null));
}

#[tokio::test]
async fn a_cancelled_page_read_returns_cancelled() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let id = Uuid::new_v4();
    let database = Arc::new(DatabaseService::new_isolated());
    let session = session(&path, id, database).await;
    let items = session
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table");
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let control = OperationControl::new(cancellation, None);

    assert!(matches!(
        session.read_page(items, 0, 2, &control).await,
        Err(DriverFailurePresentation::Cancelled)
    ));
}

#[tokio::test]
async fn dropping_a_session_removes_its_connection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let id = Uuid::new_v4();
    let database = Arc::new(DatabaseService::new_isolated());
    let session = session(&path, id, database.clone()).await;
    assert!(database.is_active(id));

    drop(session);

    assert!(!database.is_active(id));
}

#[tokio::test]
async fn a_missing_table_is_a_typed_query_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let id = Uuid::new_v4();
    let session = session(&path, id, Arc::new(DatabaseService::new_isolated())).await;

    let missing = TableInfo {
        schema: None,
        name: "nope".into(),
    };
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(5));
    let failure = session.read_page(&missing, 0, 10, &control).await.unwrap_err();
    assert!(
        matches!(failure, DriverFailurePresentation::Query { .. }),
        "{failure:?}"
    );
}
