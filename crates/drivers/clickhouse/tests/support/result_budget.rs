#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{connect, start_clickhouse};
use tablepro_core::Value;

#[tokio::test]
#[ignore = "requires docker"]
async fn query_truncates_when_result_memory_budget_is_exceeded() {
    let (_c, opts) = start_clickhouse().await;
    let conn = connect(opts).await;
    let over = conn
        .query("SELECT number, repeat('x', 1024) FROM numbers(100000)")
        .await
        .unwrap();
    assert!(!over.rows.is_empty());
    assert!(over.rows.len() < 100000);
    let payload = Value::Text("x".repeat(1024));
    for (index, row) in over.rows.iter().enumerate() {
        assert_eq!(row.len(), 2);
        assert_eq!(row[0], Value::Int(index as i64));
        assert_eq!(row[1], payload);
    }
    assert!(over.truncated);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn narrow_queries_stop_at_the_row_limit_and_report_truncation() {
    let (_c, opts) = start_clickhouse().await;
    let conn = connect(opts).await;
    let cap = tablepro_core::MAX_QUERY_ROWS;
    let result = conn
        .query(&format!("SELECT number FROM numbers({})", cap + 1))
        .await
        .unwrap();
    assert_eq!(result.rows.len(), cap);
    assert_eq!(result.rows.first(), Some(&vec![Value::Int(0)]));
    assert_eq!(result.rows.last(), Some(&vec![Value::Int((cap - 1) as i64)]));
    assert!(result.truncated);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_user_format_clause_that_never_ends_a_row_is_refused_with_bounded_memory() {
    let (_c, opts) = start_clickhouse().await;
    let conn = connect(opts).await;

    let error = conn
        .query("SELECT number FROM numbers(30000000) FORMAT Values")
        .await
        .expect_err("a single endless line must not be buffered whole");

    assert!(matches!(error, tablepro_core::DriverError::Unsupported(_)), "{error:?}");
    assert_eq!(conn.query("SELECT 1").await.unwrap().rows, vec![vec![Value::Int(1)]]);
}
