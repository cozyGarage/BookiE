#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{Connection, Value};

pub async fn assert_grid_edit(connection: &dyn Connection) {
    const VALUE: &str = "1234567890123456789012345678901234567890.1234567890123456789012345678901234567890";

    connection
        .execute("CREATE TABLE wide_numeric_grid (id integer PRIMARY KEY, amount numeric(80, 40))")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO wide_numeric_grid VALUES (1, 0)")
        .await
        .unwrap();
    let mut row = connection.query("SELECT * FROM wide_numeric_grid").await.unwrap();
    row.columns[0].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "wide_numeric_grid",
        &row.columns,
        &[(1, Value::Text(VALUE.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let result = connection
        .query("SELECT amount::text FROM wide_numeric_grid WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(result.rows, vec![vec![Value::Text(VALUE.into())]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_wide_numeric_grid_edit_preserves_exact_value() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    assert_grid_edit(connection.as_ref()).await;
}
