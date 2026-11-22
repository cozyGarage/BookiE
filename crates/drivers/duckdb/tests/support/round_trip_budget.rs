use std::collections::HashSet;

use bookie_core::{ConnectOptions, DatabaseDriver, Value};
use drivers_duckdb::DuckdbDriver;

const SIMPLE_SELECT_QUERY_LOG_CONTEXT_BUDGET: usize = 2;

#[tokio::test]
async fn a_simple_select_stays_within_its_embedded_query_log_context_budget() {
    let connection = DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .expect("open in-memory DuckDB");
    connection
        .execute("CALL enable_logging(['QueryLog'], storage = 'memory')")
        .await
        .expect("enable DuckDB query logging");
    connection
        .execute("CALL truncate_duckdb_logs()")
        .await
        .expect("clear setup log");

    let query = "SELECT 42 AS value /* bookie_round_trip_budget_duckdb */";
    let result = connection.query(query).await.expect("run measured SELECT");
    assert_eq!(result.rows, vec![vec![Value::Int(42)]]);

    let observed = connection
        .query(&format!(
            "SELECT contexts.scope, contexts.connection_id, contexts.transaction_id, \
                 contexts.query_id, contexts.thread_id, logs.message FROM duckdb_logs AS logs \
                 JOIN duckdb_log_contexts() AS contexts USING (context_id) \
                 WHERE logs.type = 'QueryLog' AND logs.message = '{query}'"
        ))
        .await
        .expect("read DuckDB query log");
    let statements = observed.rows.iter().map(|row| format!("{:?}", row)).collect::<Vec<_>>();
    let query_ids = observed
        .rows
        .iter()
        .filter_map(|row| row.get(3).map(|id| format!("{id:?}")))
        .collect::<HashSet<_>>();
    assert!(
        query_ids.len() <= SIMPLE_SELECT_QUERY_LOG_CONTEXT_BUDGET,
        "simple SELECT used {} embedded QueryLog contexts (budget {SIMPLE_SELECT_QUERY_LOG_CONTEXT_BUDGET}):\n{}",
        query_ids.len(),
        statements.join("\n")
    );
    assert!(
        !query_ids.is_empty(),
        "DuckDB QueryLog omitted the embedded SQL statement: {statements:#?}"
    );
    assert!(
        statements[0].contains("bookie_round_trip_budget_duckdb"),
        "DuckDB log omitted measured query: {statements:#?}"
    );
}
