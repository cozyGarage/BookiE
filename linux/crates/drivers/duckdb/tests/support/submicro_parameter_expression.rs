use super::native_connection;
use tablepro_core::{Connection, Value};

#[tokio::test]
async fn value_contract_submicro_text_parameters_keep_precision_after_explicit_casts() {
    let connection = native_connection().await;
    let time = chrono::NaiveTime::from_hms_nano_opt(12, 34, 56, 123_456_789).unwrap();
    let timestamp =
        chrono::NaiveDateTime::parse_from_str("1969-12-31 23:59:59.123456789", "%Y-%m-%d %H:%M:%S%.f").unwrap();
    let timestamp_tz = chrono::DateTime::parse_from_rfc3339("2026-09-27T12:34:56.123456789+05:30")
        .unwrap()
        .to_utc();
    let time_expression = connection
        .query_params(
            "SELECT CAST(? AS TIME_NS) + INTERVAL '1 nanosecond'",
            &[Value::Time(time)],
        )
        .await;
    assert!(
        matches!(
            time_expression,
            Err(tablepro_core::DriverError::Query { message, .. })
                if message.contains("TIME_NS") && message.contains("INTERVAL")
        ),
        "the native TIME_NS/INTERVAL binder refusal must stay visible"
    );
    let values = [
        Value::Time(time),
        Value::Time(time),
        Value::DateTime(timestamp),
        Value::DateTime(timestamp),
        Value::TimestampTz(timestamp_tz),
        Value::TimestampTz(timestamp_tz),
    ];
    let result = connection
        .query_params(
            "SELECT typeof(CAST(? AS TIME_NS)), CAST(? AS TIME_NS) = TIME_NS '12:34:56.123456789', typeof(CAST(? AS TIMESTAMP_NS)), CAST(? AS TIMESTAMP_NS) = TIMESTAMP_NS '1969-12-31 23:59:59.123456789', typeof(?), CAST(? AS VARCHAR)",
            &values,
        )
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Text("TIME_NS".into()),
            Value::Bool(true),
            Value::Text("TIMESTAMP_NS".into()),
            Value::Bool(true),
            Value::Text("VARCHAR".into()),
            Value::Text("2026-09-27T07:04:56.123456789+00:00".into()),
        ]]
    );
}
