#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{Connection, Value};

pub async fn assert_scalar_contract(connection: &dyn Connection, driver: &str) {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../../testdata/value-contract.json")).unwrap();
    for text in corpus["integers"].as_array().unwrap() {
        assert_scalar(
            connection,
            driver,
            "integer",
            Value::Int(text.as_str().unwrap().parse().unwrap()),
        )
        .await;
    }
    for text in corpus["floats"].as_array().unwrap() {
        assert_scalar(
            connection,
            driver,
            "float",
            Value::Float(text.as_str().unwrap().parse().unwrap()),
        )
        .await;
    }
    for text in corpus["texts"].as_array().unwrap() {
        assert_scalar(connection, driver, "text", Value::Text(text.as_str().unwrap().into())).await;
    }
    assert_long_text(connection, driver, corpus["long_text_bytes"].as_u64().unwrap() as usize).await;
    assert_scalar(connection, driver, "integer", Value::Null).await;
    assert_scalar(connection, driver, "text", Value::Null).await;
    if driver != "sqlite" {
        for text in corpus["decimals"].as_array().unwrap() {
            assert_scalar(
                connection,
                driver,
                "decimal",
                Value::Decimal(text.as_str().unwrap().parse().unwrap()),
            )
            .await;
        }
    }
}

async fn assert_long_text(connection: &dyn Connection, driver: &str, length: usize) {
    let expected = Value::Text("x".repeat(length));
    if driver != "clickhouse" {
        assert_scalar(connection, driver, "text", expected).await;
        return;
    }
    let result = connection
        .query(&format!("SELECT repeat('x', {length}) AS value"))
        .await
        .unwrap();
    assert_eq!(result.rows, vec![vec![expected.clone()]]);
    let bound = connection
        .query_params("SELECT CAST(? AS Nullable(String))", std::slice::from_ref(&expected))
        .await;
    assert!(
        matches!(bound, Err(tablepro_core::DriverError::Query { message, .. }) if message.contains("Max query size exceeded"))
    );
    let literal = tablepro_core::sql_literal::render_sql_literal(driver, &expected).unwrap();
    let exported = connection.query(&format!("SELECT {literal}")).await;
    assert!(
        matches!(exported, Err(tablepro_core::DriverError::Query { message, .. }) if message.contains("Max query size exceeded"))
    );
}

async fn assert_scalar(connection: &dyn Connection, driver: &str, kind: &str, expected: Value) {
    let placeholder = match driver {
        "postgres" => "$1",
        "mssql" => "@P1",
        _ => "?",
    };
    let sql_type = scalar_type(driver, kind);
    let sql = format!("SELECT CAST({placeholder} AS {sql_type}) AS value");
    let bound = connection
        .query_params(&sql, std::slice::from_ref(&expected))
        .await
        .unwrap();
    assert_eq!(bound.rows.len(), 1, "{driver} bound {kind}");
    assert_eq!(bound.rows[0].len(), 1, "{driver} bound {kind}");
    assert_value(&bound.rows[0][0], &expected, driver, &format!("bound {kind}"));
    let literal = tablepro_core::sql_literal::render_sql_literal(driver, &expected).unwrap();
    let exported = connection
        .query(&format!("SELECT CAST({literal} AS {sql_type}) AS value"))
        .await
        .unwrap();
    assert_eq!(exported.rows.len(), 1, "{driver} exported {kind}");
    assert_eq!(exported.rows[0].len(), 1, "{driver} exported {kind}");
    assert_value(&exported.rows[0][0], &expected, driver, &format!("exported {kind}"));
}

fn assert_value(actual: &Value, expected: &Value, driver: &str, kind: &str) {
    match (actual, expected) {
        (Value::Float(actual), Value::Float(expected)) => {
            assert_eq!(actual.to_bits(), expected.to_bits(), "{driver} {kind}")
        }
        (Value::Text(actual), Value::Decimal(expected)) if driver == "duckdb" => {
            assert_eq!(
                actual.parse().map(Value::Decimal).unwrap(),
                Value::Decimal(*expected),
                "{driver} {kind}"
            );
        }
        _ => assert_eq!(actual, expected, "{driver} {kind}"),
    }
}

fn scalar_type(driver: &str, kind: &str) -> &'static str {
    match (driver, kind) {
        ("clickhouse", "integer") => "Nullable(Int64)",
        ("mysql", "integer") => "SIGNED",
        (_, "integer") => "BIGINT",
        ("clickhouse", "float") => "Float64",
        ("postgres", "float") => "DOUBLE PRECISION",
        ("mssql", "float") => "FLOAT",
        (_, "float") => "DOUBLE",
        ("clickhouse", "text") => "Nullable(String)",
        ("mysql", "text") => "CHAR CHARACTER SET utf8mb4",
        ("mssql", "text") => "NVARCHAR(MAX)",
        (_, "text") => "TEXT",
        (_, "decimal") => "DECIMAL(28,8)",
        _ => panic!("unknown value contract kind"),
    }
}
