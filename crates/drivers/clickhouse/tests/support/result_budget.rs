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
async fn a_readonly_user_can_connect_and_a_grid_save_still_waits_for_its_mutation() {
    let (_c, opts) = start_clickhouse().await;
    let admin = connect(opts.clone()).await;
    admin
        .execute("CREATE TABLE default.notes (id UInt32, note String) ENGINE = MergeTree ORDER BY id")
        .await
        .unwrap();
    admin
        .execute("INSERT INTO default.notes VALUES (1, 'a')")
        .await
        .unwrap();
    admin
        .execute("CREATE USER ro IDENTIFIED WITH plaintext_password BY 'ro-secret' SETTINGS readonly = 1")
        .await
        .unwrap();
    admin.execute("GRANT SELECT ON default.* TO ro").await.unwrap();
    let mut read_only = opts.clone();
    read_only.username = "ro".into();
    read_only.password = secrecy::SecretString::new("ro-secret".to_string().into());

    let connection = tablepro_core::DatabaseDriver::connect(&drivers_clickhouse::ClickhouseDriver, read_only).await;

    assert!(
        connection.is_ok(),
        "a readonly=1 user must be able to connect: {:?}",
        connection.err()
    );
    admin
        .execute("ALTER TABLE default.notes UPDATE note = 'b' WHERE id = 1")
        .await
        .unwrap();
    let saved = admin.query("SELECT note FROM default.notes").await.unwrap();
    assert_eq!(saved.rows, vec![vec![Value::Text("b".into())]]);
}
