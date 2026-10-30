#[test]
fn a_real_keeps_the_digits_sql_server_shows() {
    assert_eq!(column_data_to_value(&ColumnData::F32(Some(0.1))), Value::Float(0.1));
    assert_eq!(
        column_data_to_value(&ColumnData::F32(Some(3.4e38))),
        Value::Float(3.4e38)
    );
}

use super::*;

#[test]
fn value_contract_wide_numeric_text_keeps_sign_scale_and_every_digit() {
    for (value, scale, expected) in [
        (i128::MAX, 0, "170141183460469231731687303715884105727"),
        (-i128::MAX, 0, "-170141183460469231731687303715884105727"),
        (
            -123_456_789_012_345_678_901_234_567_891,
            30,
            "-0.123456789012345678901234567891",
        ),
        (
            -50_000_000_000_000_000_000_000_000_000,
            30,
            "-0.050000000000000000000000000000",
        ),
        (
            -123_456_789_012_345_678_901_234_567_895,
            1,
            "-12345678901234567890123456789.5",
        ),
        (
            1_500_000_000_000_000_000_000_000_000_000,
            30,
            "1.500000000000000000000000000000",
        ),
        (0, 30, "0.000000000000000000000000000000"),
    ] {
        let numeric = tiberius::numeric::Numeric::new_with_scale(value, scale);
        let column_data = ColumnData::Numeric(Some(numeric));
        assert_eq!(column_data_to_value(&column_data), Value::Text(expected.into()));
    }
}

#[test]
fn a_null_numeric_column_stays_null() {
    let column_data = ColumnData::Numeric(None);
    assert_eq!(column_data_to_value(&column_data), Value::Null);
}

#[test]
fn a_numeric_value_within_decimals_range_still_decodes_as_decimal() {
    let numeric = tiberius::numeric::Numeric::new_with_scale(1234, 2);
    let column_data = ColumnData::Numeric(Some(numeric));
    assert_eq!(
        column_data_to_value(&column_data),
        Value::Decimal(rust_decimal::Decimal::new(1234, 2))
    );
}

#[test]
fn a_numeric_value_at_decimals_maximum_scale_still_decodes_as_decimal() {
    let numeric = tiberius::numeric::Numeric::new_with_scale(1, Decimal::MAX_SCALE as u8);
    let column_data = ColumnData::Numeric(Some(numeric));
    assert_eq!(
        column_data_to_value(&column_data),
        Value::Decimal(Decimal::new(1, Decimal::MAX_SCALE))
    );
}

#[test]
fn a_temporal_column_the_driver_cannot_convert_is_not_reported_as_null() {
    let failed: Result<Option<NaiveDate>, &str> = Err("out of range");
    assert_eq!(
        decoded_temporal(failed, Value::Date, "date"),
        Value::Undecodable("date".to_string())
    );
}

#[test]
fn a_temporal_column_holding_sql_null_stays_null() {
    let stored_null: Result<Option<NaiveDate>, &str> = Ok(None);
    assert_eq!(decoded_temporal(stored_null, Value::Date, "date"), Value::Null);
}

#[test]
fn a_temporal_column_that_converts_keeps_its_value() {
    let date = NaiveDate::from_ymd_opt(2026, 9, 18).expect("a valid date");
    let decoded: Result<Option<NaiveDate>, &str> = Ok(Some(date));
    assert_eq!(decoded_temporal(decoded, Value::Date, "date"), Value::Date(date));
}

#[test]
fn column_type_names_are_human_readable() {
    assert_eq!(column_type_to_string(ColumnType::Int4), "int");
    assert_eq!(column_type_to_string(ColumnType::NVarchar), "nvarchar");
    assert_eq!(column_type_to_string(ColumnType::Datetime2), "datetime2");
    assert_eq!(column_type_to_string(ColumnType::Money4), "smallmoney");
    assert_eq!(column_type_to_string(ColumnType::Guid), "uniqueidentifier");
    assert_eq!(column_type_to_string(ColumnType::Bit), "bit");
}

#[test]
fn money_columns_refuse_float_decoding_but_preserve_null() {
    assert_eq!(
        column_data_to_value_for_type(&ColumnData::F64(Some(123.45)), ColumnType::Money),
        Value::Undecodable("money".into())
    );
    assert_eq!(
        column_data_to_value_for_type(&ColumnData::F64(None), ColumnType::Money),
        Value::Null
    );
    assert_eq!(
        column_data_to_value_for_type(&ColumnData::F32(Some(-12.34)), ColumnType::Money4),
        Value::Undecodable("smallmoney".into())
    );
    assert_eq!(
        column_data_to_value_for_type(&ColumnData::F32(None), ColumnType::Money4),
        Value::Null
    );
}

#[test]
fn sql_variant_columns_refuse_untyped_values_but_preserve_null() {
    assert_eq!(
        column_data_to_value_for_type(&ColumnData::I64(Some(9_007_199_254_740_993)), ColumnType::SSVariant),
        Value::Undecodable("sql_variant".into())
    );
    assert_eq!(
        column_data_to_value_for_type(&ColumnData::I64(None), ColumnType::SSVariant),
        Value::Null
    );
}

#[test]
fn legacy_datetime_uses_exact_text_for_inexact_ticks_and_keeps_null_and_datetime2() {
    let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let days_since_1900 = (date - chrono::NaiveDate::from_ymd_opt(1900, 1, 1).unwrap()).num_days() as i32;
    let legacy = tiberius::time::DateTime::new(days_since_1900, 2);
    for column_type in [ColumnType::Datetime, ColumnType::Datetimen] {
        assert_eq!(
            column_data_to_value_for_type(&ColumnData::DateTime(Some(legacy)), column_type),
            Value::Text("2024-01-02T00:00:00.007".into())
        );
        assert_eq!(
            column_data_to_value_for_type(
                &ColumnData::DateTime(Some(tiberius::time::DateTime::new(days_since_1900, 3))),
                column_type
            ),
            Value::DateTime(date.and_hms_milli_opt(0, 0, 0, 10).unwrap())
        );
        assert_eq!(
            column_data_to_value_for_type(&ColumnData::DateTime(None), column_type),
            Value::Null
        );
    }
    let days_since_year_one = (date - chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap()).num_days() as u32;
    let datetime2 = tiberius::time::DateTime2::new(
        tiberius::time::Date::new(days_since_year_one),
        tiberius::time::Time::new(110_450_066_666, 7),
    );
    let smalldatetime = tiberius::time::SmallDateTime::new(days_since_1900 as u16, 3 * 60 + 4);
    assert_eq!(
        column_data_to_value_for_type(&ColumnData::DateTime2(Some(datetime2)), ColumnType::Datetime2),
        Value::DateTime(date.and_hms_nano_opt(3, 4, 5, 6_666_600).unwrap())
    );
    assert_eq!(
        column_data_to_value_for_type(&ColumnData::SmallDateTime(Some(smalldatetime)), ColumnType::Datetimen),
        Value::DateTime(date.and_hms_opt(3, 4, 0).unwrap())
    );
}

#[test]
fn legacy_datetime_refuses_inexact_ticks_outside_one_day() {
    let value = tiberius::time::DateTime::new(0, 25_920_002);
    assert_eq!(
        column_data_to_value(&ColumnData::DateTime(Some(value))),
        Value::Undecodable("datetime".into())
    );
}
