use super::*;
use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};

async fn create_db(path: &std::path::Path) -> SqliteConnection {
    let options = SqliteConnectOptions::new().filename(path).create_if_missing(true);
    let mut db = SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query(
        "CREATE TABLE connections (id TEXT PRIMARY KEY, config_json TEXT NOT NULL);
         CREATE TABLE connection_secrets (
             connection_id TEXT NOT NULL,
             key TEXT NOT NULL,
             secret TEXT NOT NULL,
             secret_enc TEXT,
             PRIMARY KEY (connection_id, key)
         )",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db
}

async fn insert(db: &mut SqliteConnection, id: &str, config: &str) {
    sqlx::query("INSERT INTO connections (id, config_json) VALUES (?, ?)")
        .bind(id)
        .bind(config)
        .execute(db)
        .await
        .unwrap();
}

#[tokio::test]
async fn imports_public_connection_fields_without_loading_secret_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dbx.db");
    let mut db = create_db(&path).await;
    insert(
        &mut db,
        "orders",
        r##"{"name":"Orders","db_type":"postgres","host":"db.example.test","port":5433,"username":"ada","database":"orders","ssl":true,"read_only":true,"password":"password-sentinel","ca_cert_path":"/certs/ca.pem","connect_timeout_secs":12,"query_timeout_secs":90}"##,
    )
    .await;
    sqlx::query("INSERT INTO connection_secrets VALUES (?, ?, ?, ?)")
        .bind("orders")
        .bind("password")
        .bind("")
        .bind("secret-sentinel")
        .execute(&mut db)
        .await
        .unwrap();
    db.close().await.unwrap();

    let imported = parse_dbx_connection_database(&path).await.unwrap();

    assert_eq!(imported.drafts.len(), 1);
    let draft = &imported.drafts[0];
    assert_eq!((draft.name.as_str(), draft.driver_id.as_str()), ("Orders", "postgres"));
    assert_eq!(
        (draft.host.as_str(), draft.port, draft.database.as_str()),
        ("db.example.test", 5433, "orders")
    );
    assert_eq!(draft.username, "ada");
    assert!(draft.read_only);
    assert!(draft.use_tls);
    assert_eq!(
        draft.tls_root_cert.as_deref(),
        Some(std::path::Path::new("/certs/ca.pem"))
    );
    assert_eq!(draft.connect_timeout_secs, Some(12));
    assert_eq!(draft.query_timeout_secs, Some(90));
    assert!(!format!("{draft:?}").contains("password-sentinel"));
    assert!(!format!("{draft:?}").contains("secret-sentinel"));
}

#[tokio::test]
async fn unsupported_engine_and_active_transport_are_named_in_the_preview() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dbx.db");
    let mut db = create_db(&path).await;
    insert(
        &mut db,
        "routed",
        r#"{"name":"Routed","db_type":"postgres","host":"db.example.test","port":5432,"transport_layers":[{"type":"ssh","host":"jump.example.test"}]}"#,
    )
    .await;
    insert(
        &mut db,
        "unsupported",
        r#"{"name":"Warehouse","db_type":"hive","host":"hive.example.test","port":10000}"#,
    )
    .await;
    insert(
        &mut db,
        "supported",
        r#"{"name":"Local","db_type":"postgres","host":"localhost","port":5432}"#,
    )
    .await;
    insert(
        &mut db,
        "redis-direct",
        r#"{"name":"Redis","db_type":"redis","host":"redis.example.test","port":6379,"redis_connection_mode":"standalone"}"#,
    )
    .await;
    insert(
        &mut db,
        "production",
        r#"{"name":"Production","db_type":"postgres","host":"prod.example.test","is_production":true}"#,
    )
    .await;
    insert(
        &mut db,
        "filtered",
        r#"{"name":"Filtered","db_type":"postgres","host":"filtered.example.test","visible_databases":["sales"]}"#,
    )
    .await;
    db.close().await.unwrap();

    let imported = parse_dbx_connection_database(&path).await.unwrap();

    assert_eq!(imported.drafts.len(), 3);
    assert!(imported.drafts.iter().any(|draft| draft.driver_id == "redis"));
    assert!(
        imported
            .drafts
            .iter()
            .any(|draft| draft.environment == bookie_core::Environment::Prod)
    );
    assert!(
        imported
            .skipped
            .iter()
            .any(|name| name.contains("Routed") && name.contains("SSH"))
    );
    assert!(
        imported
            .skipped
            .iter()
            .any(|name| name.contains("Warehouse") && name.contains("hive"))
    );
    assert!(
        imported
            .skipped
            .iter()
            .any(|name| name.contains("Filtered") && name.contains("visibility filters"))
    );
}

#[tokio::test]
async fn oversized_connection_config_is_skipped_without_loading_the_full_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dbx.db");
    let mut db = create_db(&path).await;
    insert(
        &mut db,
        "large",
        &format!(
            r#"{{"name":"Large","db_type":"postgres","host":"db.example.test","note":"{}"}}"#,
            "x".repeat(MAX_DBX_CONFIG_BYTES + 32)
        ),
    )
    .await;
    insert(
        &mut db,
        "small",
        r#"{"name":"Small","db_type":"postgres","host":"db.example.test"}"#,
    )
    .await;
    db.close().await.unwrap();

    let imported = parse_dbx_connection_database(&path).await.unwrap();

    assert_eq!(imported.drafts.len(), 1);
    assert!(
        imported
            .skipped
            .iter()
            .any(|name| name.contains("configuration exceeds"))
    );
}

#[tokio::test]
async fn total_connection_config_bytes_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dbx.db");
    let mut db = create_db(&path).await;
    let config = format!(r#"{{"note":"{}"}}"#, "x".repeat(900_000));
    for index in 0..5 {
        insert(&mut db, &format!("connection-{index}"), &config).await;
    }
    db.close().await.unwrap();

    let error = parse_dbx_connection_database(&path).await.unwrap_err();

    assert!(error.to_string().contains("total import size limit"));
}

#[tokio::test]
async fn refuses_a_sqlite_file_without_the_dbx_connection_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.db");
    let mut db = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path).create_if_missing(true))
        .await
        .unwrap();
    sqlx::query("CREATE TABLE unrelated (value TEXT)")
        .execute(&mut db)
        .await
        .unwrap();
    db.close().await.unwrap();

    assert!(parse_dbx_connection_database(&path).await.is_err());
}
