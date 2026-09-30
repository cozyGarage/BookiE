use chrono::NaiveDate;
use tablepro_core::Value;

#[tokio::test]
#[ignore = "requires docker"]
async fn second_precision_datetime64_beyond_nanosecond_limit_round_trips() {
    let (_container, options) = super::start_clickhouse().await;
    let connection = super::connect(options).await;
    let stamp = NaiveDate::from_ymd_opt(2299, 12, 31)
        .unwrap()
        .and_hms_opt(23, 59, 59)
        .unwrap();
    let expected = vec![vec![
        Value::DateTime(stamp),
        Value::Int(stamp.and_utc().timestamp_millis()),
    ]];

    let native = connection
        .query(
            "SELECT toDateTime64('2299-12-31 23:59:59', 0), \
                    toUnixTimestamp64Milli(toDateTime64('2299-12-31 23:59:59', 0))",
        )
        .await
        .expect("pinned server accepts the legal DateTime64(0) endpoint");
    assert_eq!(native.rows, expected, "server value and epoch are exact");

    let values = [Value::DateTime(stamp), Value::DateTime(stamp)];
    let bound = connection
        .query_params(
            "SELECT CAST(? AS DateTime64(0)), \
                    toUnixTimestamp64Milli(CAST(? AS DateTime64(0)))",
            &values,
        )
        .await
        .expect("exact second-precision parameter remains in the wider native range");
    assert_eq!(bound.rows, expected, "bound value preserves the native epoch");

    let literal = tablepro_core::sql_literal::render_sql_literal("clickhouse", &Value::DateTime(stamp))
        .expect("exact second-precision SQL literal remains in the wider native range");
    let exported = connection
        .query(&format!("SELECT {literal}, toUnixTimestamp64Milli({literal})"))
        .await
        .expect("re-import exact SQL literal");
    assert_eq!(exported.rows, expected, "literal preserves the native epoch");
}
