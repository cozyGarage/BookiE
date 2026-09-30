use super::{col, parse_input_for_driver};
use chrono::Timelike;
use tablepro_core::Value;

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn value_contract_duckdb_microsecond_grid_types_refuse_submicro_edits() {
    use tablepro_core::{ConnectOptions, DatabaseDriver};

    let connection = drivers_duckdb::DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE temporal_grid (id INTEGER PRIMARY KEY, clock TIME, event_at TIMESTAMP)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO temporal_grid VALUES (1, TIME '12:34:56.123456', TIMESTAMP '2026-09-29 12:34:56.123456')")
        .await
        .unwrap();

    let columns = connection.fetch_columns(None, "temporal_grid").await.unwrap();
    let clock_index = columns.iter().position(|column| column.name == "clock").unwrap();
    let stamp_index = columns.iter().position(|column| column.name == "event_at").unwrap();
    let rejected_clock = parse_input_for_driver("12:34:56.123456789", Some(&columns[clock_index]), "duckdb");
    let rejected_stamp = parse_input_for_driver("2026-09-29 12:34:56.123456789", Some(&columns[stamp_index]), "duckdb");
    assert!(
        rejected_clock.is_err(),
        "TIME grid edits must not lose sub-microsecond digits"
    );
    assert!(
        rejected_stamp.is_err(),
        "TIMESTAMP grid edits must not lose sub-microsecond digits"
    );
    assert_eq!(
        connection
            .query_params(
                "SELECT CAST(CAST(? AS TIME) AS VARCHAR), CAST(CAST(? AS TIMESTAMP) AS VARCHAR)",
                &[
                    Value::Text("12:34:56.123456789".into()),
                    Value::Text("2026-09-29 12:34:56.123456789".into()),
                ],
            )
            .await
            .unwrap()
            .rows,
        vec![vec![
            Value::Text("12:34:56.123456".into()),
            Value::Text("2026-09-29 12:34:56.123456".into())
        ]],
        "DuckDB casts to microsecond TIME/TIMESTAMP truncate unsupported nanoseconds"
    );
    assert_eq!(
        connection
            .query("SELECT CAST(clock AS VARCHAR), CAST(event_at AS VARCHAR) FROM temporal_grid WHERE id = 1")
            .await
            .unwrap()
            .rows,
        vec![vec![
            Value::Text("12:34:56.123456".into()),
            Value::Text("2026-09-29 12:34:56.123456".into())
        ]],
        "refused edits must leave both stored values unchanged"
    );
}

#[test]
fn value_contract_duckdb_time_and_timestamp_parsers_refuse_submicro_edits() {
    for (data_type, input) in [
        ("TIME", "12:34:56.123456789"),
        ("TIMESTAMP", "2026-09-29 12:34:56.123456789"),
    ] {
        assert!(
            parse_input_for_driver(input, Some(&col(data_type, false)), "duckdb").is_err(),
            "{data_type} must refuse a value DuckDB stores at microsecond precision"
        );
    }
    assert!(matches!(
        parse_input_for_driver("12:34:56.123456000", Some(&col("TIME", false)), "duckdb"),
        Ok(Value::Time(time)) if time.nanosecond() == 123_456_000
    ));
    assert_eq!(
        parse_input_for_driver("12:34:56.123456789", Some(&col("TIME_NS", false)), "duckdb").unwrap(),
        Value::Text("12:34:56.123456789".into()),
        "TIME_NS remains exact text instead of being parsed at lower precision"
    );
    let aligned_timestamp = parse_input_for_driver(
        "2026-09-29 12:34:56.123456000",
        Some(&col("TIMESTAMP", false)),
        "duckdb",
    );
    assert!(
        matches!(aligned_timestamp, Ok(Value::DateTime(timestamp)) if timestamp.nanosecond() == 123_456_000),
        "{aligned_timestamp:?}"
    );
    assert!(matches!(
        parse_input_for_driver(
            "2026-09-29 12:34:56.123456789",
            Some(&col("TIMESTAMP_NS", false)),
            "duckdb"
        ),
        Ok(Value::DateTime(timestamp)) if timestamp.nanosecond() == 123_456_789
    ));
    assert_eq!(
        parse_input_for_driver(
            "2026-09-29 12:34:56.123456789",
            Some(&col("TIMESTAMP_S", false)),
            "duckdb"
        )
        .unwrap_err(),
        "DuckDB TIMESTAMP_S precision cannot represent this value exactly"
    );
    assert_eq!(
        parse_input_for_driver(
            "2026-09-29 12:34:56.123456789",
            Some(&col("TIMESTAMP_MS", false)),
            "duckdb"
        )
        .unwrap_err(),
        "DuckDB TIMESTAMP_MS precision cannot represent this value exactly"
    );
    assert!(matches!(
        parse_input_for_driver(
            "2026-09-29 12:34:56.123000000",
            Some(&col("TIMESTAMP_MS", false)),
            "duckdb"
        ),
        Ok(Value::DateTime(timestamp)) if timestamp.nanosecond() == 123_000_000
    ));
}

#[test]
fn value_contract_duckdb_timestamptz_parser_refuses_submicro_edits() {
    let column = col("TIMESTAMP WITH TIME ZONE", false);
    let submicro = "2026-09-29T12:34:56.123456789+00:00";
    assert_eq!(
        parse_input_for_driver("2026-09-29T12:34:56.123456+00:00", Some(&column), "duckdb").unwrap(),
        Value::TimestampTz(
            chrono::DateTime::parse_from_rfc3339("2026-09-29T12:34:56.123456Z")
                .unwrap()
                .to_utc()
        )
    );
    assert_eq!(
        parse_input_for_driver("2026-09-29T12:34:56.123456000+00:00", Some(&column), "duckdb").unwrap(),
        Value::TimestampTz(
            chrono::DateTime::parse_from_rfc3339("2026-09-29T12:34:56.123456Z")
                .unwrap()
                .to_utc()
        ),
        "extra trailing zero digits are still exactly microsecond-aligned"
    );
    assert_eq!(
        parse_input_for_driver(submicro, Some(&column), "duckdb").unwrap_err(),
        "DuckDB TIMESTAMPTZ supports microsecond precision only"
    );
    assert_eq!(
        parse_input_for_driver(submicro, Some(&col("TEXT", false)), "duckdb").unwrap(),
        Value::Text(submicro.into()),
        "ordinary text is not subject to TIMESTAMPTZ precision limits"
    );
    assert!(matches!(
        parse_input_for_driver(submicro, Some(&column), "sqlite").unwrap(),
        Value::TimestampTz(timestamp) if timestamp.timestamp_subsec_nanos() == 123_456_789
    ));
    assert_eq!(
        parse_input_for_driver(submicro, None, "duckdb").unwrap(),
        Value::Text(submicro.into())
    );
}
