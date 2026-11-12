#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![recursion_limit = "256"]

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
        tls_strict_encryption: false,
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
async fn page_sizes_are_bounded_before_a_read() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let session = session(&path, Uuid::new_v4(), Arc::new(DatabaseService::new_isolated())).await;
    let table = session.tables.iter().find(|table| table.name == "items").unwrap();
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(5));
    let maximum = bookie_core::MAX_QUERY_CURSOR_PAGE_ROWS as u64;

    assert_eq!(
        session.read_page(table, 0, maximum, &control).await.unwrap().rows.len(),
        3
    );
    for limit in [0, maximum + 1, u64::MAX] {
        assert!(matches!(
            session.read_page(table, 0, limit, &control).await,
            Err(DriverFailurePresentation::Unsupported(_))
        ));
    }
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
async fn an_old_session_cannot_read_or_close_a_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let id = Uuid::new_v4();
    let database = Arc::new(DatabaseService::new_isolated());
    let old = session(&path, id, database.clone()).await;
    let table = old.tables.iter().find(|table| table.name == "items").unwrap().clone();
    database.close(id);
    let replacement = session(&path, id, database.clone()).await;
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(5));

    assert!(matches!(
        old.read_page(&table, 0, 2, &control).await,
        Err(DriverFailurePresentation::Disconnected)
    ));
    assert!(matches!(
        old.run_query("SELECT 1", &control).await,
        Err(DriverFailurePresentation::Disconnected)
    ));
    let registry = DriverRegistry::new();
    assert!(matches!(
        old.with_subscription::<(), _, _>(&registry, &control, |_| async { panic!("stale callback") })
            .await,
        Err(DriverFailurePresentation::Disconnected)
    ));
    drop(old);
    assert_eq!(
        replacement.read_page(&table, 0, 2, &control).await.unwrap().rows.len(),
        2
    );
}

#[tokio::test]
async fn a_writable_saved_configuration_cannot_write_through_a_read_session() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let mut configuration = saved(&path, Uuid::new_v4());
    configuration.read_only = false;
    let mut registry = DriverRegistry::new();
    registry.register(Arc::new(drivers_sqlite::SqliteDriver));
    let session = ReadSession::open(
        configuration,
        Arc::new(registry),
        Arc::new(DatabaseService::new_isolated()),
    )
    .await
    .unwrap();
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(5));
    for sql in [
        "INSERT INTO items VALUES (4, 'x')",
        "UPDATE items SET name = 'x'",
        "DELETE FROM items",
        "CREATE TABLE extra (id INTEGER)",
    ] {
        assert!(session.run_query(sql, &control).await.is_err(), "write accepted: {sql}");
    }
    let result = session.run_query("SELECT count(*) FROM items", &control).await.unwrap();
    assert_eq!(result.rows.iter().next().map(|row| row[0].clone()), Some(Value::Int(3)));
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

#[tokio::test]
async fn sql_runs_through_the_guard_and_a_write_is_refused_on_a_read_only_session() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.db");
    seeded(&path).await;
    let session = session(&path, Uuid::new_v4(), Arc::new(DatabaseService::new_isolated())).await;
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(5));

    let result = session
        .run_query("SELECT id, name FROM items WHERE id >= 2 ORDER BY id", &control)
        .await
        .expect("select");
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows.iter().nth(1).map(|row| row[1].clone()), Some(Value::Null));

    let refused = session.run_query("DELETE FROM items", &control).await.unwrap_err();
    assert!(
        matches!(
            refused,
            DriverFailurePresentation::ReadOnly | DriverFailurePresentation::PolicyDenied(_)
        ),
        "{refused:?}"
    );
    let still = session
        .run_query("SELECT count(*) FROM items", &control)
        .await
        .expect("count");
    assert_eq!(still.rows.iter().next().map(|row| row[0].clone()), Some(Value::Int(3)));
}
