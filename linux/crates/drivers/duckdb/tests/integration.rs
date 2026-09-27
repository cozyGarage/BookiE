#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use drivers_duckdb::DuckdbDriver;
use tablepro_core::{ConnectOptions, Connection, DatabaseDriver, Value};

#[path = "../../../core/tests/support/value_contract.rs"]
mod value_contract;

#[tokio::test]
async fn value_contract_preserves_scalar_boundaries_through_parameters_and_exports() {
    let connection = DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    value_contract::assert_scalar_contract(connection.as_ref(), "duckdb").await;
}

async fn native_connection() -> Box<dyn Connection> {
    DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn value_contract_native_temporals_round_trip_parameters_and_sql() {
    let connection = native_connection().await;
    for (kind, text) in [
        ("DATE", "1969-12-31"),
        ("DATE", "2000-02-29"),
        ("DATE", "10000-01-01"),
        ("DATE", "0001-01-01 (BC)"),
        ("DATE", "0002-12-31 (BC)"),
        ("TIME", "24:00:00"),
        ("TIME", "23:59:59.123456"),
        ("TIME", "00:00:00"),
        ("TIMESTAMP_S", "1969-12-31 23:59:59"),
        ("TIMESTAMP_MS", "1969-12-31 23:59:59.999"),
        ("TIMESTAMP", "1969-12-31 23:59:59.999999"),
        ("TIMESTAMP_NS", "1969-12-31 23:59:59.999999999"),
        ("TIMESTAMP_NS", "2026-09-27 12:34:56.123456789"),
        ("TIMESTAMPTZ", "2026-09-27 12:34:56.123456+05:30"),
        ("DATE", "infinity"),
        ("DATE", "-infinity"),
        ("TIMESTAMP", "infinity"),
        ("TIMESTAMP", "-infinity"),
    ] {
        let source = format!("CAST('{text}' AS {kind})");
        let result = connection.query(&format!("SELECT {source}")).await.unwrap();
        let value = &result.rows[0][0];
        assert!(!matches!(value, Value::Undecodable(_)), "{kind}: {value:?}");
        if kind == "TIMESTAMPTZ" {
            assert!(matches!(value, Value::TimestampTz(_)));
        }
        let literal = tablepro_core::sql_literal::render_sql_literal("duckdb", value).unwrap();
        let round_trip = connection
            .query(&format!(
                "SELECT CAST(CAST({literal} AS {kind}) AS VARCHAR) = CAST({source} AS VARCHAR)"
            ))
            .await
            .unwrap();
        assert_eq!(round_trip.rows, vec![vec![Value::Bool(true)]], "{kind}: {value:?}");
        let bound = connection
            .query_params(
                &format!("SELECT CAST(CAST(? AS {kind}) AS VARCHAR) = CAST({source} AS VARCHAR)"),
                std::slice::from_ref(value),
            )
            .await
            .unwrap();
        assert_eq!(bound.rows, vec![vec![Value::Bool(true)]], "bound {kind}: {value:?}");
    }
}

#[tokio::test]
async fn value_contract_enum_labels_and_unsupported_collections_are_explicit() {
    let connection = native_connection().await;
    connection
        .execute("CREATE TYPE mood AS ENUM ('', 'NULL', '東京')")
        .await
        .unwrap();
    let result = connection
        .query("SELECT ''::mood, 'NULL'::mood, '東京'::mood, NULL::mood")
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Text("".into()),
            Value::Text("NULL".into()),
            Value::Text("東京".into()),
            Value::Null
        ]]
    );
    for expression in [
        "INTERVAL '2 months -3 days 1 microsecond'",
        "[1, NULL, 3]",
        "[1, 2]::INTEGER[2]",
        "{'a': 1}",
        "MAP(['a'], [1])",
        "union_value(a := 1)",
        "DATE '1000000-01-01'",
        "TIMESTAMP '10000-01-01 00:00:00'",
        "TIMESTAMP '0001-01-01 (BC) 00:00:00'",
    ] {
        let result = connection.query(&format!("SELECT {expression}")).await.unwrap();
        let value = &result.rows[0][0];
        assert!(matches!(value, Value::Undecodable(_)), "{expression}: {value:?}");
        assert!(tablepro_core::sql_literal::render_sql_literal("duckdb", value).is_err());
        assert!(
            connection
                .query_params("SELECT ?", std::slice::from_ref(value))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn value_contract_native_temporal_nulls_and_json_preserve_precision() {
    let connection = native_connection().await;
    for kind in [
        "DATE",
        "TIME",
        "TIMESTAMP_S",
        "TIMESTAMP_MS",
        "TIMESTAMP",
        "TIMESTAMP_NS",
        "TIMESTAMPTZ",
        "INTERVAL",
        "INTEGER[]",
    ] {
        assert_eq!(
            connection.query(&format!("SELECT NULL::{kind}")).await.unwrap().rows,
            vec![vec![Value::Null]]
        );
    }
    let result = connection
        .query("SELECT TIMESTAMP_NS '1969-12-31 23:59:59.999999999' AS stamp, TIME '23:59:59.123456' AS clock")
        .await
        .unwrap();
    let json = tablepro_core::export::render_json(&result.columns, &result.rows);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed,
        serde_json::json!([{"stamp": "1969-12-31 23:59:59.999999999", "clock": "23:59:59.123456"}])
    );
}
