#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_app_services::data_dictionary::load;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, OperationControl, Value};
use drivers_postgres::PgDriver;
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

async fn fixture() -> (impl Drop, Box<dyn Connection>) {
    let container = Postgres::default()
        .with_tag("18-alpine")
        .start()
        .await
        .expect("start PostgreSQL fixture");
    let options = ConnectOptions {
        host: container.get_host().await.expect("container host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("container port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: SecretString::from("postgres"),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    };
    let connection = PgDriver.connect(options).await.expect("connect to PostgreSQL");
    (container, connection)
}

fn find_row<'a>(result: &'a bookie_core::QueryResult, kind: &str, name: &str) -> &'a [Value] {
    result
        .rows
        .iter()
        .find(|row| matches!((&row[2], &row[3]), (Value::Text(row_kind), Value::Text(row_name)) if row_kind == kind && row_name == name))
        .expect("dictionary row")
}

#[tokio::test]
#[ignore = "requires Docker; run through the app-server test layer"]
async fn postgres_dictionary_matches_catalog_types_comments_and_keys_in_stable_order() {
    let (_container, connection) = fixture().await;
    for statement in [
        "CREATE SCHEMA dictionary_test",
        "CREATE TABLE dictionary_test.parent (id bigint PRIMARY KEY, label text NOT NULL DEFAULT 'active')",
        "COMMENT ON COLUMN dictionary_test.parent.label IS 'display label'",
        "CREATE TABLE dictionary_test.child (id bigint PRIMARY KEY, parent_id bigint NOT NULL REFERENCES dictionary_test.parent(id))",
        "CREATE INDEX child_parent_lookup ON dictionary_test.child(parent_id)",
    ] {
        connection.execute(statement).await.expect(statement);
    }
    let tables = connection
        .list_tables()
        .await
        .expect("list tables")
        .into_iter()
        .filter(|table| table.schema.as_deref() == Some("dictionary_test"))
        .collect::<Vec<_>>();
    let first_control = OperationControl::with_timeout(std::time::Duration::from_secs(20));
    let first = load(
        connection.as_ref(),
        Some("dictionary_test"),
        &tables,
        PgDriver.capabilities(),
        &first_control,
    )
    .await
    .expect("complete dictionary");
    let second_control = OperationControl::with_timeout(std::time::Duration::from_secs(20));
    let second = load(
        connection.as_ref(),
        Some("dictionary_test"),
        &tables,
        PgDriver.capabilities(),
        &second_control,
    )
    .await
    .expect("repeat dictionary");

    let label = find_row(&first, "column", "label");
    assert_eq!(label[0], Value::Text("dictionary_test".into()));
    assert_eq!(label[1], Value::Text("parent".into()));
    assert_eq!(label[4], Value::Text("text".into()));
    assert_eq!(label[5], Value::Bool(false));
    assert!(matches!(&label[6], Value::Text(value) if value.contains("active")));
    assert_eq!(label[8], Value::Text("display label".into()));
    assert_eq!(find_row(&first, "column", "id")[7], Value::Bool(true));
    assert!(first.rows.iter().any(|row| {
        matches!((&row[1], &row[2], &row[3]), (Value::Text(table), Value::Text(kind), Value::Text(name))
            if table == "child" && kind == "index" && name == "child_parent_lookup")
    }));
    assert!(first.rows.iter().any(|row| {
        matches!((&row[1], &row[2]), (Value::Text(table), Value::Text(kind)) if table == "child" && kind == "foreign_key")
    }));
    assert!(!first.truncated);
    assert_eq!(first.rows.len(), second.rows.len());
    for (left, right) in first.rows.iter().zip(second.rows.iter()) {
        assert_eq!(left, right);
    }
}
