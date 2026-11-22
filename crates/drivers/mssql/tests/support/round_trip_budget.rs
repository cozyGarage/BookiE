use bookie_core::Value;

use super::{connect, shared_container::start_mssql};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_simple_select_stays_within_its_server_statement_budget() {
    let (_container, options) = start_mssql().await;
    let connection = connect(options).await;
    connection
        .execute("CREATE TABLE dbo.bookie_round_trip_budget (id bigint PRIMARY KEY, value bigint)")
        .await
        .expect("create measured table");
    connection
        .execute("INSERT INTO dbo.bookie_round_trip_budget VALUES (1, 42)")
        .await
        .expect("seed measured table");
    let query = "SELECT value AS bookie_round_trip_budget_mssql_value \
                 FROM dbo.bookie_round_trip_budget WHERE id = 1";
    let result = connection.query(query).await.expect("run measured SELECT");
    assert_eq!(result.rows, vec![vec![Value::Int(42)]]);

    let observed = connection
        .query(
            "SELECT text, execution_count FROM sys.dm_exec_query_stats \
             CROSS APPLY sys.dm_exec_sql_text(sql_handle) WHERE text LIKE N'%bookie_round_trip_budget_mssql_value%'",
        )
        .await
        .expect("read server execution counts");
    let statements = observed.rows.iter().map(|row| format!("{:?}", row)).collect::<Vec<_>>();
    let count = observed
        .rows
        .iter()
        .filter_map(|row| match row.get(1) {
            Some(Value::Int(count)) => Some(*count),
            _ => None,
        })
        .sum::<i64>();
    assert!(
        count <= 1,
        "simple SELECT used {count} server executions (budget 1):\n{}",
        statements.join("\n")
    );
    assert!(
        count > 0,
        "server execution statistics omitted measured query: {statements:#?}"
    );
}
