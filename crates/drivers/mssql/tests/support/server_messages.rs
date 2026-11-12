#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use bookie_core::{MAX_SERVER_MESSAGES, MessageSeverity};

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(60))
}

#[tokio::test]
#[ignore = "requires docker"]
async fn print_and_low_severity_raiserror_reach_the_batch_messages() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;

    let batch = conn
        .query_result_sets_controlled(
            "PRINT 'hello from print'; RAISERROR('low severity', 10, 1); SELECT 1 AS one",
            &[],
            &control(),
        )
        .await
        .unwrap();
    let texts: Vec<&str> = batch.messages.iter().map(|message| message.text.as_str()).collect();
    assert_eq!(texts, ["hello from print", "low severity"]);
    assert!(
        batch
            .messages
            .iter()
            .all(|message| message.severity == MessageSeverity::Info)
    );
    assert_eq!(batch.result_sets[0].rows, vec![vec![Value::Int(1)]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn messages_do_not_leak_into_the_next_statement() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;

    let first = conn
        .query_result_sets_controlled("PRINT 'only once'", &[], &control())
        .await
        .unwrap();
    assert_eq!(first.messages.len(), 1);
    let second = conn
        .query_result_sets_controlled("SELECT 2 AS two", &[], &control())
        .await
        .unwrap();
    assert!(second.messages.is_empty());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_session_reads_its_own_messages_and_not_the_previous_statements() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;
    let mut session = conn.open_session().await.unwrap();

    let first = session
        .query_result_sets_params_controlled("PRINT 'session print'", &[], &control())
        .await
        .unwrap();
    assert_eq!(first.messages.iter().next().unwrap().text, "session print");
    let second = session
        .query_result_sets_params_controlled("SELECT 1 AS one", &[], &control())
        .await
        .unwrap();
    assert!(second.messages.is_empty());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_flood_of_prints_is_capped_with_an_omitted_count() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;
    let total = MAX_SERVER_MESSAGES + 150;

    let batch = conn
        .query_result_sets_controlled(
            &format!("DECLARE @i int = 0; WHILE @i < {total} BEGIN PRINT CONCAT('line ', @i); SET @i += 1; END"),
            &[],
            &control(),
        )
        .await
        .unwrap();
    assert_eq!(batch.messages.len(), MAX_SERVER_MESSAGES);
    assert_eq!(batch.messages.total(), total as u64);
}
