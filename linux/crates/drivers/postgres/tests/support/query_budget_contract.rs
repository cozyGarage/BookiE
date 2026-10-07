use tablepro_core::Value;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_query_truncated_at_the_shared_byte_budget_preserves_order_and_pool_reuse() {
    let (_container, opts) = super::start_pg().await;
    let connection = super::connect(opts).await;
    let payload = "x".repeat(4 * 1024);
    let result = connection
        .query(&format!(
            "SELECT i, repeat('x', {}) AS payload \
             FROM generate_series(1, 20000) AS t(i) ORDER BY i",
            payload.len()
        ))
        .await
        .expect("bounded query returns its admitted rows");

    assert!(result.truncated, "omitted server rows must be reported");
    assert!(!result.rows.is_empty());
    assert!(result.rows.len() < 20_000);
    assert!(result.rows.len() * payload.len() <= tablepro_core::MAX_QUERY_RESULT_BYTES);
    assert!(result.rows.iter().enumerate().all(|(index, row)| {
        row.first() == Some(&Value::Int(index as i64 + 1))
            && matches!(row.get(1), Some(Value::Text(value)) if value == &payload)
    }));

    let next = connection
        .query("SELECT 42::bigint AS usable")
        .await
        .expect("pool connection is reusable after truncation");
    assert_eq!(next.rows, vec![vec![Value::Int(42)]]);
}
