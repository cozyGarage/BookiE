#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use drivers_sqlite::SqliteDriver;
use tablepro_core::{AttachedDatabase, ConnectOptions, DatabaseDriver, Value};
use tempfile::TempDir;

#[tokio::test]
async fn multi_database_attachment_and_schema_listing() {
    let dir = TempDir::new().unwrap();
    let main_path = dir.path().join("main.db").to_string_lossy().to_string();
    let aux_path = dir.path().join("aux.db").to_string_lossy().to_string();

    let aux_conn = SqliteDriver
        .connect(ConnectOptions {
            database: aux_path.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    aux_conn
        .execute("CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER, amount REAL)")
        .await
        .unwrap();
    aux_conn
        .execute("CREATE VIEW high_orders AS SELECT * FROM orders WHERE amount > 100.0")
        .await
        .unwrap();
    drop(aux_conn);

    let main_conn = SqliteDriver
        .connect(ConnectOptions {
            database: main_path,
            attached_databases: vec![AttachedDatabase::new("aux", aux_path)],
            ..Default::default()
        })
        .await
        .unwrap();

    main_conn
        .execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    main_conn
        .execute("CREATE VIEW user_names AS SELECT name FROM users")
        .await
        .unwrap();

    let dbs = main_conn.list_databases().await.unwrap();
    assert!(dbs.contains(&"main".to_string()));
    assert!(dbs.contains(&"aux".to_string()));

    let tables = main_conn.list_tables().await.unwrap();
    assert!(tables.iter().any(|t| t.schema.is_none() && t.name == "users"));
    assert!(
        tables
            .iter()
            .any(|t| t.schema.as_deref() == Some("aux") && t.name == "orders")
    );

    let views = main_conn.list_views().await.unwrap();
    assert!(views.iter().any(|v| v.schema.is_none() && v.name == "user_names"));
    assert!(
        views
            .iter()
            .any(|v| v.schema.as_deref() == Some("aux") && v.name == "high_orders")
    );
}

#[tokio::test]
async fn cross_schema_query_and_row_fetching() {
    let dir = TempDir::new().unwrap();
    let main_path = dir.path().join("main.db").to_string_lossy().to_string();
    let aux_path = dir.path().join("aux.db").to_string_lossy().to_string();

    let aux_conn = SqliteDriver
        .connect(ConnectOptions {
            database: aux_path.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    aux_conn
        .execute("CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER, amount REAL)")
        .await
        .unwrap();
    aux_conn
        .execute("INSERT INTO orders VALUES (1, 10, 42.50), (2, 20, 150.00)")
        .await
        .unwrap();
    drop(aux_conn);

    let main_conn = SqliteDriver
        .connect(ConnectOptions {
            database: main_path,
            attached_databases: vec![AttachedDatabase::new("aux", aux_path)],
            ..Default::default()
        })
        .await
        .unwrap();

    main_conn
        .execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    main_conn
        .execute("INSERT INTO users VALUES (10, 'Alice'), (20, 'Bob')")
        .await
        .unwrap();

    let cross_result = main_conn
        .query("SELECT u.name, o.amount FROM users u JOIN aux.orders o ON u.id = o.user_id ORDER BY u.id")
        .await
        .unwrap();
    assert_eq!(cross_result.rows.len(), 2);
    assert_eq!(cross_result.rows[0][0], Value::Text("Alice".into()));
    assert_eq!(cross_result.rows[0][1], Value::Float(42.50));
    assert_eq!(cross_result.rows[1][0], Value::Text("Bob".into()));
    assert_eq!(cross_result.rows[1][1], Value::Float(150.00));

    let rows_aux = main_conn.fetch_rows(Some("aux"), "orders", 0, 10).await.unwrap();
    assert_eq!(rows_aux.rows.len(), 2);

    let rows_main = main_conn.fetch_rows(None, "users", 0, 10).await.unwrap();
    assert_eq!(rows_main.rows.len(), 2);
}

#[tokio::test]
async fn attached_schema_metadata() {
    let dir = TempDir::new().unwrap();
    let main_path = dir.path().join("main.db").to_string_lossy().to_string();
    let aux_path = dir.path().join("aux.db").to_string_lossy().to_string();

    let aux_conn = SqliteDriver
        .connect(ConnectOptions {
            database: aux_path.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    aux_conn
        .execute("CREATE TABLE parent (id INTEGER PRIMARY KEY, code TEXT UNIQUE)")
        .await
        .unwrap();
    aux_conn
        .execute("CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER, FOREIGN KEY (parent_id) REFERENCES parent(id))")
        .await
        .unwrap();
    aux_conn
        .execute("CREATE INDEX idx_child_parent ON child(parent_id)")
        .await
        .unwrap();
    drop(aux_conn);

    let main_conn = SqliteDriver
        .connect(ConnectOptions {
            database: main_path,
            attached_databases: vec![AttachedDatabase::new("aux", aux_path)],
            ..Default::default()
        })
        .await
        .unwrap();

    let columns = main_conn.fetch_columns(Some("aux"), "parent").await.unwrap();
    assert_eq!(columns.len(), 2);
    assert_eq!(columns[0].name, "id");
    assert!(columns[0].primary_key);
    assert!(columns[0].is_auto_increment);
    assert_eq!(columns[1].name, "code");

    let indexes = main_conn.fetch_indexes(Some("aux"), "child").await.unwrap();
    assert!(indexes.iter().any(|idx| idx.name == "idx_child_parent"));

    let foreign_keys = main_conn.fetch_foreign_keys(Some("aux"), "child").await.unwrap();
    assert_eq!(foreign_keys.len(), 1);
    assert_eq!(foreign_keys[0].ref_table, "parent");
    assert_eq!(foreign_keys[0].ref_schema.as_deref(), Some("aux"));
    assert_eq!(foreign_keys[0].columns, vec!["parent_id".to_string()]);
    assert_eq!(foreign_keys[0].ref_columns, vec!["id".to_string()]);
}

#[tokio::test]
async fn session_inherits_attached_databases() {
    let dir = TempDir::new().unwrap();
    let main_path = dir.path().join("main.db").to_string_lossy().to_string();
    let aux_path = dir.path().join("aux.db").to_string_lossy().to_string();

    let aux_conn = SqliteDriver
        .connect(ConnectOptions {
            database: aux_path.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    aux_conn
        .execute("CREATE TABLE items (id INTEGER PRIMARY KEY, sku TEXT)")
        .await
        .unwrap();
    aux_conn
        .execute("INSERT INTO items VALUES (1, 'BOOKIE-01')")
        .await
        .unwrap();
    drop(aux_conn);

    let main_conn = SqliteDriver
        .connect(ConnectOptions {
            database: main_path,
            attached_databases: vec![AttachedDatabase::new("aux", aux_path)],
            ..Default::default()
        })
        .await
        .unwrap();

    let mut session = main_conn.open_session().await.unwrap();
    let control = tablepro_core::OperationControl::with_timeout(std::time::Duration::from_secs(5));
    let result = session
        .query_params_controlled("SELECT sku FROM aux.items", &[], &control)
        .await
        .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Text("BOOKIE-01".into()));
}

#[tokio::test]
async fn missing_attached_database_skipped_without_error() {
    let dir = TempDir::new().unwrap();
    let main_path = dir.path().join("main.db").to_string_lossy().to_string();
    let missing_path = dir.path().join("does_not_exist.db").to_string_lossy().to_string();

    let conn = SqliteDriver
        .connect(ConnectOptions {
            database: main_path,
            attached_databases: vec![AttachedDatabase::new("missing", missing_path)],
            ..Default::default()
        })
        .await
        .unwrap();

    conn.execute("CREATE TABLE local_data (val TEXT)").await.unwrap();
    conn.execute("INSERT INTO local_data VALUES ('ok')").await.unwrap();

    let dbs = conn.list_databases().await.unwrap();
    assert_eq!(dbs, vec!["main".to_string()]);

    let res = conn.query("SELECT val FROM local_data").await.unwrap();
    assert_eq!(res.rows[0][0], Value::Text("ok".into()));
}
