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
async fn value_contract_nested_uhugeint_refuses_lossy_consumers() {
    let connection = native_connection().await;
    let expression = "[18446744073709551616::UHUGEINT]";
    let oracle = connection
        .query(&format!("SELECT typeof({expression}), ({expression})[1]::VARCHAR"))
        .await
        .unwrap();
    assert_eq!(
        oracle.rows,
        vec![vec![
            Value::Text("UHUGEINT[]".into()),
            Value::Text("18446744073709551616".into())
        ]]
    );

    let result = connection.query(&format!("SELECT {expression}")).await.unwrap();
    let value = &result.rows[0][0];
    assert!(matches!(value, Value::Undecodable(_)), "{value:?}");
    assert!(tablepro_core::sql_literal::render_sql_literal("duckdb", value).is_err());
    assert!(
        connection
            .query_params("SELECT ?", std::slice::from_ref(value))
            .await
            .is_err()
    );
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

#[tokio::test]
async fn value_contract_decimal_parameters_and_results_stay_exact_numbers() {
    let connection = native_connection().await;
    let decimal = |text: &str| Value::Decimal(text.parse().unwrap());
    for (sql, parameter, expected) in [
        ("SELECT ? * 2", "2.5", "5.0"),
        ("SELECT ? + 1", "1000", "1001"),
        ("SELECT ?", "-0.00000001", "-0.00000001"),
        (
            "SELECT ?",
            "-99999999999999999999.99999999",
            "-99999999999999999999.99999999",
        ),
    ] {
        let result = connection.query_params(sql, &[decimal(parameter)]).await.unwrap();
        assert_eq!(result.rows, vec![vec![decimal(expected)]], "{sql} with {parameter}");
    }
    let result = connection
        .query("SELECT 12.340::DECIMAL(10,3), CAST('-1234567890123456789012345678.0123456789' AS DECIMAL(38,10))")
        .await
        .unwrap();
    assert!(matches!(&result.rows[0][0], Value::Decimal(value) if value.to_string() == "12.340"));
    assert_eq!(
        result.rows[0][1],
        Value::Text("-1234567890123456789012345678.0123456789".into())
    );
}

fn naive(text: &str) -> chrono::NaiveDateTime {
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f").unwrap()
}

fn bound_temporal_corpus() -> Vec<(Value, &'static str)> {
    let date = |text: &str| Value::Date(text.parse().unwrap());
    let time = |text: &str| Value::Time(text.parse().unwrap());
    vec![
        (date("2000-02-29"), "DATE '2000-02-29'"),
        (date("1969-12-31"), "DATE '1969-12-31'"),
        (date("0001-01-01"), "DATE '0001-01-01'"),
        (date("9999-12-31"), "DATE '9999-12-31'"),
        (time("23:59:59.123456"), "TIME '23:59:59.123456'"),
        (time("00:00:00"), "TIME '00:00:00'"),
        (
            Value::DateTime(naive("1969-12-31 23:59:59.999999")),
            "TIMESTAMP '1969-12-31 23:59:59.999999'",
        ),
        (
            Value::DateTime(naive("0001-01-01 00:00:00")),
            "TIMESTAMP '0001-01-01 00:00:00'",
        ),
        (
            Value::DateTime(naive("9999-12-31 23:59:59.999999")),
            "TIMESTAMP '9999-12-31 23:59:59.999999'",
        ),
    ]
}

#[tokio::test]
async fn value_contract_bound_temporals_are_typed_in_expressions() {
    let connection = native_connection().await;
    for (value, literal) in bound_temporal_corpus() {
        let expressions: &[&str] = match &value {
            Value::Date(_) => &["? + 1", "? + INTERVAL 1 DAY", "year(?)", "date_trunc('month', ?)"],
            Value::Time(_) => &["? + INTERVAL 1 SECOND", "hour(?)"],
            _ => &["? + INTERVAL 1 DAY", "year(?)", "date_trunc('month', ?)", "epoch_us(?)"],
        };
        for expression in expressions {
            let bound = connection
                .query_params(&format!("SELECT {expression}"), std::slice::from_ref(&value))
                .await
                .unwrap_or_else(|error| panic!("{expression} with {value:?}: {error}"));
            let inline = connection
                .query(&format!("SELECT {}", expression.replace('?', literal)))
                .await
                .unwrap();
            assert_eq!(bound.rows, inline.rows, "{expression} with {value:?}");
        }
        let echoed = connection
            .query_params("SELECT ?", std::slice::from_ref(&value))
            .await
            .unwrap();
        assert_eq!(echoed.rows, vec![vec![value.clone()]], "SELECT ? with {literal}");
    }
}

#[tokio::test]
async fn value_contract_bound_temporals_insert_and_read_back_exactly() {
    let connection = native_connection().await;
    connection.execute("SET TimeZone = 'Asia/Tokyo'").await.unwrap();
    connection
        .execute("CREATE TABLE stamps (id INTEGER, d DATE, t TIME, ts TIMESTAMP, ns TIMESTAMP_NS, tz TIMESTAMPTZ)")
        .await
        .unwrap();
    let rows = [
        (
            "1969-12-31",
            "23:59:59.999999",
            "1969-12-31 23:59:59.999999",
            "1969-12-31 23:59:59.999999999",
        ),
        (
            "0001-01-01",
            "00:00:00.000001",
            "0001-01-01 00:00:00.000001",
            "1677-09-22 00:00:00.000000001",
        ),
        (
            "9999-12-31",
            "12:00:00",
            "9999-12-31 23:59:59.999999",
            "2262-04-10 23:59:59.999999999",
        ),
        (
            "2026-09-27",
            "12:34:56.5",
            "2026-09-27 12:34:56.123456",
            "2026-09-27 12:34:56.123456",
        ),
    ];
    for (id, (date, time, stamp, nanos)) in rows.iter().enumerate() {
        let values = vec![
            Value::Int(id as i64),
            Value::Date(date.parse().unwrap()),
            Value::Time(time.parse().unwrap()),
            Value::DateTime(naive(stamp)),
            Value::DateTime(naive(nanos)),
            Value::TimestampTz(naive(stamp).and_utc()),
        ];
        connection
            .execute_params("INSERT INTO stamps VALUES (?, ?, ?, ?, ?, ?)", &values)
            .await
            .unwrap();
        let read = connection
            .query_params("SELECT * FROM stamps WHERE id = ?", &[Value::Int(id as i64)])
            .await
            .unwrap();
        assert_eq!(read.rows, vec![values], "row {id}");
    }
    connection
        .execute_params(
            "INSERT INTO stamps (id, d) VALUES (?, ?)",
            &[
                Value::Int(9),
                Value::Date(chrono::NaiveDate::from_ymd_opt(0, 1, 1).unwrap()),
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        connection
            .query("SELECT d FROM stamps WHERE id = 9")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Text("0001-01-01 (BC)".into())]]
    );
}

#[tokio::test]
async fn value_contract_bound_timestamptz_keeps_its_instant_in_any_session_zone() {
    let connection = native_connection().await;
    for zone in ["UTC", "Asia/Tokyo", "America/Los_Angeles"] {
        connection.execute(&format!("SET TimeZone = '{zone}'")).await.unwrap();
        for (text, literal) in [
            (
                "2026-09-27T12:34:56.123456+05:30",
                "TIMESTAMPTZ '2026-09-27 12:34:56.123456+05:30'",
            ),
            (
                "1969-12-31T23:59:59.999999Z",
                "TIMESTAMPTZ '1969-12-31 23:59:59.999999+00'",
            ),
        ] {
            let value = Value::TimestampTz(chrono::DateTime::parse_from_rfc3339(text).unwrap().to_utc());
            let bound = connection
                .query_params(
                    &format!("SELECT CAST(? AS TIMESTAMPTZ) = {literal}, epoch_us(CAST(? AS TIMESTAMPTZ))"),
                    &[value.clone(), value.clone()],
                )
                .await
                .unwrap();
            let inline = connection
                .query(&format!("SELECT true, epoch_us({literal})"))
                .await
                .unwrap();
            assert_eq!(bound.rows, inline.rows, "{zone}: {text}");
        }
    }
}
