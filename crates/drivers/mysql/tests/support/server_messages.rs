#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{MAX_SERVER_MESSAGES, MessageSeverity, OperationControl, Value};

use crate::{connect, start_mariadb, start_mysql};

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(60))
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
