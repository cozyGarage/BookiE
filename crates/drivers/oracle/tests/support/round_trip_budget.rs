use bookie_core::{DatabaseDriver, Value};
use rust_decimal::Decimal;

use super::{OracleDriver, PASSWORD, options, start_oracle};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_simple_select_stays_within_its_server_statement_budget() {
    let (_container, port) = start_oracle().await;
    let connection = OracleDriver
        .connect(options(port, "SYSTEM", PASSWORD))
        .await
        .expect("connect BookiE");
    let query = "SELECT /* bookie_round_trip_budget_oracle */ 42 AS value FROM dual";
    let result = connection.query(query).await.expect("run measured SELECT");
    assert_eq!(result.rows, vec![vec![Value::Decimal(Decimal::from(42))]]);

    let observed = connection
        .query(&format!(
            "SELECT sql_text, executions FROM v$sql WHERE sql_text = '{query}'"
        ))
        .await
        .expect("read server execution counts");
    let statements = observed.rows.iter().map(|row| format!("{:?}", row)).collect::<Vec<_>>();
    let count = observed
        .rows
        .iter()
        .filter_map(|row| match row.get(1) {
            Some(Value::Decimal(count)) => Some(*count),
            _ => None,
        })
        .sum::<Decimal>();
    assert!(
        count <= Decimal::ONE,
        "simple SELECT used {count} server executions (budget 1):\n{}",
        statements.join("\n")
    );
    assert!(
        count > Decimal::ZERO,
        "server execution statistics omitted measured query: {statements:#?}"
    );
}
