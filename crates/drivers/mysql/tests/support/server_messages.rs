#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{MAX_SERVER_MESSAGES, MessageSeverity, OperationControl, Value};
use secrecy::ExposeSecret;
use sqlx::Connection as _;

use crate::{connect, start_mariadb, start_mysql};

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(60))
}

async fn assert_multi_statement_warning_limit(opts: bookie_core::ConnectOptions) {
    let raw_options = sqlx::mysql::MySqlConnectOptions::new()
        .host(&opts.host)
        .port(opts.port)
        .database(&opts.database)
        .username(&opts.username)
        .password(opts.password.expose_secret())
        .ssl_mode(sqlx::mysql::MySqlSslMode::Disabled);
    let mut raw = sqlx::MySqlConnection::connect_with(&raw_options).await.unwrap();
    sqlx::raw_sql("CREATE PROCEDURE warning_test() BEGIN SELECT CAST('first_bad' AS SIGNED); SELECT CAST('second_bad' AS SIGNED); END")
        .execute(&mut raw)
        .await
        .unwrap();
    raw.close().await.unwrap();
    let conn = connect(opts).await;
    let batch = conn
        .query_result_sets_controlled("CALL warning_test()", &[], &control())
        .await
        .unwrap();
    assert!(
        batch
            .messages
            .iter()
            .any(|message| message.text.contains("warning details may be incomplete"))
    );
    assert_eq!(batch.result_sets.len(), 2);
    let next = conn
        .query_result_sets_controlled("SELECT 1", &[], &control())
        .await
        .unwrap();
    assert!(next.messages.is_empty(), "the notice must not leak into the next query");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_multi_statement_warnings_are_accounted_for() {
    let (_db, opts) = start_mysql().await;
    assert_multi_statement_warning_limit(opts).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_multi_statement_warnings_are_accounted_for() {
    let (_db, opts) = start_mariadb().await;
    assert_multi_statement_warning_limit(opts).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_mysql_conversion_warning_reaches_the_batch_messages() {
    let (_db, opts) = start_mysql().await;
    let conn = connect(opts).await;

    let batch = conn
        .query_result_sets_controlled("SELECT CAST('abc' AS SIGNED) AS n", &[], &control())
        .await
        .unwrap();
    let message = batch.messages.iter().next().expect("a warning");
    assert_eq!(message.severity, MessageSeverity::Warning);
    assert_eq!(message.code.as_deref(), Some("1292"));
    assert!(message.text.contains("abc"), "{}", message.text);
    assert_eq!(batch.result_sets[0].rows, vec![vec![Value::Int(0)]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_mariadb_conversion_warning_reaches_the_batch_messages() {
    let (_db, opts) = start_mariadb().await;
    let conn = connect(opts).await;

    let batch = conn
        .query_result_sets_controlled("SELECT CAST('abc' AS SIGNED) AS n", &[], &control())
        .await
        .unwrap();
    let message = batch.messages.iter().next().expect("a warning");
    assert_eq!(message.severity, MessageSeverity::Warning);
    assert_eq!(message.code.as_deref(), Some("1292"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn warnings_do_not_leak_into_the_next_statement_on_a_session() {
    let (_db, opts) = start_mysql().await;
    let conn = connect(opts).await;
    let mut session = conn.open_session().await.unwrap();

    let first = session
        .query_result_sets_params_controlled("SELECT CAST('abc' AS SIGNED) AS n", &[], &control())
        .await
        .unwrap();
    assert!(!first.messages.is_empty());
    let second = session
        .query_result_sets_params_controlled("SELECT 1 AS one", &[], &control())
        .await
        .unwrap();
    assert!(second.messages.is_empty());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_statement_without_warnings_has_no_messages() {
    let (_db, opts) = start_mysql().await;
    let conn = connect(opts).await;

    let batch = conn
        .query_result_sets_controlled("SELECT 1 AS one", &[], &control())
        .await
        .unwrap();
    assert!(batch.messages.is_empty());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn many_warnings_are_capped_with_an_omitted_count() {
    let (_db, opts) = start_mysql().await;
    let conn = connect(opts).await;
    let total = MAX_SERVER_MESSAGES + 50;
    let values = vec!["CAST('x' AS SIGNED)"; total].join(", ");

    let batch = conn
        .query_result_sets_controlled(&format!("SELECT {values}"), &[], &control())
        .await
        .unwrap();
    assert_eq!(batch.messages.len(), MAX_SERVER_MESSAGES);
    assert_eq!(batch.messages.total(), total as u64);
}

async fn assert_each_statement_reports_its_own_warnings(opts: bookie_core::ConnectOptions) {
    let conn = connect(opts).await;
    let first = conn
        .query_result_sets_controlled("SELECT CAST('first_bad' AS SIGNED)", &[], &control())
        .await
        .unwrap();
    let second = conn
        .query_result_sets_controlled("SELECT CAST('second_bad' AS SIGNED)", &[], &control())
        .await
        .unwrap();
    let texts =
        |batch: &bookie_core::QueryResultBatch| batch.messages.iter().map(|m| m.text.clone()).collect::<Vec<_>>();
    assert!(
        texts(&first).iter().all(|t| t.contains("first_bad")),
        "{:?}",
        texts(&first)
    );
    assert!(
        texts(&second).iter().all(|t| t.contains("second_bad")),
        "{:?}",
        texts(&second)
    );
    assert!(!texts(&first).is_empty() && !texts(&second).is_empty());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_statements_run_one_after_another_keep_their_own_warnings() {
    let (_db, opts) = start_mysql().await;
    assert_each_statement_reports_its_own_warnings(opts).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_statements_run_one_after_another_keep_their_own_warnings() {
    let (_db, opts) = start_mariadb().await;
    assert_each_statement_reports_its_own_warnings(opts).await;
}
