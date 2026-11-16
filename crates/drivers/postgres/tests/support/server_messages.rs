#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{MAX_SERVER_MESSAGES, MessageSeverity, OperationControl, Value};

use crate::{connect, start_pg};

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(60))
}

#[tokio::test]
#[ignore = "requires docker"]
async fn raise_notice_and_warning_reach_the_batch_messages() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;

    let batch = conn
        .query_result_sets_controlled(
            "DO $$ BEGIN RAISE NOTICE 'hello %', 'world'; RAISE WARNING 'careful'; END $$",
            &[],
            &control(),
        )
        .await
        .unwrap();
    let messages: Vec<_> = batch.messages.iter().collect();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].severity, MessageSeverity::Notice);
    assert_eq!(messages[0].text, "hello world");
    assert_eq!(messages[0].code.as_deref(), Some("00000"));
    assert_eq!(messages[1].severity, MessageSeverity::Warning);
    assert_eq!(messages[1].text, "careful");
    assert_eq!(messages[1].code.as_deref(), Some("01000"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_function_that_raises_several_notices_keeps_their_order_next_to_its_rows() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    conn.execute(
        "CREATE FUNCTION chatty() RETURNS integer LANGUAGE plpgsql AS $$ \
         BEGIN RAISE NOTICE 'one'; RAISE NOTICE 'two'; RAISE INFO 'three'; RETURN 7; END $$",
    )
    .await
    .unwrap();

    let batch = conn
        .query_result_sets_controlled("SELECT chatty() AS n", &[], &control())
        .await
        .unwrap();
    let texts: Vec<&str> = batch.messages.iter().map(|message| message.text.as_str()).collect();
    assert_eq!(texts, ["one", "two", "three"]);
    assert_eq!(batch.result_sets[0].rows, vec![vec![Value::Int(7)]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_flood_of_notices_is_capped_with_an_omitted_count() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    let total = MAX_SERVER_MESSAGES + 300;

    let batch = conn
        .query_result_sets_controlled(
            &format!("DO $$ BEGIN FOR i IN 1..{total} LOOP RAISE NOTICE 'n %', i; END LOOP; END $$"),
            &[],
            &control(),
        )
        .await
        .unwrap();
    assert_eq!(batch.messages.len(), MAX_SERVER_MESSAGES);
    assert_eq!(batch.messages.total(), total as u64);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn the_next_statement_never_sees_the_previous_statements_notices() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;

    let first = conn
        .query_result_sets_controlled("DO $$ BEGIN RAISE NOTICE 'only once'; END $$", &[], &control())
        .await
        .unwrap();
    assert_eq!(first.messages.len(), 1);
    let second = conn
        .query_result_sets_controlled("SELECT 1 AS one", &[], &control())
        .await
        .unwrap();
    assert!(second.messages.is_empty());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_session_reads_its_own_notices_and_not_the_previous_statements() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    let mut session = conn.open_session().await.unwrap();

    let first = session
        .query_result_sets_params_controlled("DO $$ BEGIN RAISE NOTICE 'session notice'; END $$", &[], &control())
        .await
        .unwrap();
    assert_eq!(first.messages.iter().next().unwrap().text, "session notice");
    let second = session
        .query_result_sets_params_controlled("SELECT 1", &[], &control())
        .await
        .unwrap();
    assert!(second.messages.is_empty());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_failed_statement_does_not_leave_notices_for_the_next_one() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;

    let failed = conn
        .query_result_sets_controlled(
            "DO $$ BEGIN RAISE NOTICE 'before the error'; RAISE EXCEPTION 'boom'; END $$",
            &[],
            &control(),
        )
        .await;
    assert!(failed.is_err());
    let next = conn
        .query_result_sets_controlled("SELECT 1", &[], &control())
        .await
        .unwrap();
    assert!(next.messages.is_empty());
}
