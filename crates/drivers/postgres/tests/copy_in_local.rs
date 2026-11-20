#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! `COPY ... FROM STDIN` against an already-running Postgres (no Docker required).
//!
//! Ignored by default. Env: SMOKE_PG_HOST, SMOKE_PG_PORT, SMOKE_PG_USER, SMOKE_PG_PASS, SMOKE_PG_DB.
//! Each test creates and drops its own table.

use std::time::Duration;

use bookie_core::{ConnectOptions, Connection, DatabaseDriver, OperationControl, Value};
use drivers_postgres::PgDriver;

fn opts_from_env() -> ConnectOptions {
    let port = std::env::var("SMOKE_PG_PORT").unwrap_or_else(|_| "54329".into());
    ConnectOptions {
        host: std::env::var("SMOKE_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: port.parse().unwrap_or(54329),
        database: std::env::var("SMOKE_PG_DB").unwrap_or_else(|_| "bookie".into()),
        username: std::env::var("SMOKE_PG_USER").unwrap_or_else(|_| "bookie".into()),
        password: secrecy::SecretString::new(
            std::env::var("SMOKE_PG_PASS")
                .unwrap_or_else(|_| "bookie".into())
                .into(),
        ),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

fn control() -> OperationControl {
    OperationControl::with_timeout(Duration::from_secs(20))
}

async fn fresh_table(connection: &dyn Connection, name: &str) {
    connection
        .execute(&format!("DROP TABLE IF EXISTS {name}"))
        .await
        .unwrap();
    connection
        .execute(&format!("CREATE TABLE {name} (id int, note text)"))
        .await
        .unwrap();
}

async fn count(connection: &dyn Connection, name: &str) -> Value {
    connection
        .query(&format!("SELECT count(*) FROM {name}"))
        .await
        .unwrap()
        .rows[0][0]
        .clone()
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn a_text_copy_loads_its_rows_and_a_hostile_data_row_stays_data() {
    let connection = PgDriver.connect(opts_from_env()).await.unwrap();
    fresh_table(&*connection, "claude_copy_text").await;
    let unit = "COPY claude_copy_text (id, note) FROM STDIN;\n1\tpaid; DROP TABLE claude_copy_text\n2\ttab\\there\n3\t\\N\n\\.";
    let batch = connection
        .query_result_sets_controlled(unit, &[], &control())
        .await
        .unwrap();
    assert_eq!(batch.rows_affected, Some(3));
    assert_eq!(
        count(&*connection, "claude_copy_text").await,
        Value::Int(3),
        "the table survived and holds 3 rows"
    );
    let note = connection
        .query("SELECT note FROM claude_copy_text WHERE id = 1")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    assert_eq!(note, Value::Text("paid; DROP TABLE claude_copy_text".into()));
    let null_note = connection
        .query("SELECT note FROM claude_copy_text WHERE id = 3")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    assert_eq!(null_note, Value::Null);
    connection.execute("DROP TABLE claude_copy_text").await.unwrap();
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn a_csv_copy_with_options_loads_quoted_fields() {
    let connection = PgDriver.connect(opts_from_env()).await.unwrap();
    fresh_table(&*connection, "claude_copy_csv").await;
    let unit =
        "COPY claude_copy_csv FROM STDIN WITH (FORMAT csv, HEADER);\nid,note\n1,\"a, b; c\"\n2,\"say \"\"hi\"\"\"\n\\.";
    let result = connection.execute_controlled(unit, &control()).await.unwrap();
    assert_eq!(result.rows_affected, 2);
    let notes = connection
        .query("SELECT note FROM claude_copy_csv ORDER BY id")
        .await
        .unwrap();
    assert_eq!(notes.rows[0][0], Value::Text("a, b; c".into()));
    assert_eq!(notes.rows[1][0], Value::Text("say \"hi\"".into()));
    connection.execute("DROP TABLE claude_copy_csv").await.unwrap();
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn a_bad_row_loads_nothing_and_the_connection_keeps_working() {
    let connection = PgDriver.connect(opts_from_env()).await.unwrap();
    fresh_table(&*connection, "claude_copy_bad").await;
    let unit = "COPY claude_copy_bad FROM STDIN;\n1\tfine\nnot-a-number\toops\n\\.";
    let error = connection.query_controlled(unit, &control()).await.unwrap_err();
    assert!(error.to_string().to_lowercase().contains("integer"), "{error}");
    assert_eq!(
        count(&*connection, "claude_copy_bad").await,
        Value::Int(0),
        "a failed COPY is atomic"
    );
    assert_eq!(connection.query("SELECT 1").await.unwrap().rows[0][0], Value::Int(1));
    connection.execute("DROP TABLE claude_copy_bad").await.unwrap();
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn data_without_its_terminator_is_refused_before_anything_is_sent() {
    let connection = PgDriver.connect(opts_from_env()).await.unwrap();
    fresh_table(&*connection, "claude_copy_open").await;
    let error = connection
        .query_controlled("COPY claude_copy_open FROM STDIN;\n1\tone\n2\ttwo\n", &control())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("must end with a line"), "{error}");
    assert_eq!(count(&*connection, "claude_copy_open").await, Value::Int(0));
    connection.execute("DROP TABLE claude_copy_open").await.unwrap();
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn a_session_loads_a_copy_and_stays_usable() {
    let connection = PgDriver.connect(opts_from_env()).await.unwrap();
    fresh_table(&*connection, "claude_copy_session").await;
    let mut session = connection.open_session().await.unwrap();
    let batch = session
        .query_result_sets_params_controlled("COPY claude_copy_session FROM STDIN;\n1\ta\n2\tb\n\\.", &[], &control())
        .await
        .unwrap();
    assert_eq!(batch.rows_affected, Some(2));
    let seen = session
        .query_params_controlled("SELECT count(*) FROM claude_copy_session", &[], &control())
        .await
        .unwrap();
    assert_eq!(seen.rows[0][0], Value::Int(2));
    session.close().await.unwrap();
    connection.execute("DROP TABLE claude_copy_session").await.unwrap();
}
