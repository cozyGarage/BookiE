use super::{ClickhouseDriver, clickhouse::start_clickhouse};
use bookie_core::{DatabaseDriver, Value};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_simple_select_uses_one_server_statement() {
    let (_container, options) = start_clickhouse().await;
    let connection = ClickhouseDriver.connect(options).await.expect("connect BookiE");
    let query = "SELECT 42 AS value /* bookie_round_trip_budget_clickhouse */";
    let result = connection.query(query).await.expect("run measured SELECT");
    assert_eq!(result.rows, vec![vec![Value::Int(42)]]);

    connection.execute("SYSTEM FLUSH LOGS").await.expect("flush query log");
    let observed = connection
        .query(&format!(
            "SELECT query, type, query_id FROM system.query_log \
             WHERE query = '{query}' AND type = 'QueryFinish'"
        ))
        .await
        .expect("read server query log");
    let statements = observed.rows.iter().map(|row| format!("{:?}", row)).collect::<Vec<_>>();
    assert_eq!(
        statements.len(),
        1,
        "simple SELECT expected one server statement, observed:\n{}",
        statements.join("\n")
    );
    assert!(
        statements[0].contains("bookie_round_trip_budget_clickhouse"),
        "server trace omitted measured query: {statements:#?}"
    );
}
