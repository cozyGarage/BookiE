use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use rust_decimal::Decimal;
use tiberius::{Column, ColumnData, ColumnType, FromSql};

use tablepro_core::{ColumnInfo, Value};

use crate::zoned;

pub(crate) fn col_to_info(c: &Column) -> ColumnInfo {
    ColumnInfo {
        name: c.name().to_string(),
        data_type: column_type_to_string(c.column_type()),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
    }
}

/// rust_decimal::Decimal stores its mantissa in a 96-bit unsigned
/// integer -- 2^96 - 1, the crate's own stable, documented capacity.
/// `Decimal::from_i128_with_scale` panics above this instead of
/// returning a `Result`, so it must be checked before calling.
const MAX_DECIMAL_MANTISSA: u128 = 0x0000_0000_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF;

fn column_data_to_value(cd: &ColumnData<'static>) -> Value {
    match cd {
        ColumnData::Bit(v) => (*v).map(Value::Bool).unwrap_or(Value::Null),
        ColumnData::U8(v) => (*v).map(|n| Value::Int(i64::from(n))).unwrap_or(Value::Null),
        ColumnData::I16(v) => (*v).map(|n| Value::Int(i64::from(n))).unwrap_or(Value::Null),
        ColumnData::I32(v) => (*v).map(|n| Value::Int(i64::from(n))).unwrap_or(Value::Null),
        ColumnData::I64(v) => (*v).map(Value::Int).unwrap_or(Value::Null),
        ColumnData::F32(v) => (*v).map(|n| Value::Float(f64::from(n))).unwrap_or(Value::Null),
        ColumnData::F64(v) => (*v).map(Value::Float).unwrap_or(Value::Null),
        ColumnData::String(v) => v.as_ref().map(|s| Value::Text(s.to_string())).unwrap_or(Value::Null),
        ColumnData::Binary(v) => v.as_ref().map(|b| Value::Bytes(b.to_vec())).unwrap_or(Value::Null),
        ColumnData::Guid(v) => (*v).map(Value::Uuid).unwrap_or(Value::Null),
        // SQL Server NUMERIC carries up to 38 digits of precision;
        // rust_decimal::Decimal's 96-bit mantissa caps out lower than
        // that. Decimal::from_i128_with_scale (which tiberius's own
        // Decimal::from_sql calls) panics rather than erroring on an
        // out-of-range value, so the fit has to be checked before
        // calling it. tiberius's Numeric Display puts a negative sign on
        // the fraction and appends ".0" at scale 0, so a wider value is
        // rendered from its mantissa and scale instead.
        ColumnData::Numeric(raw) => match raw {
            Some(numeric) => {
                let value = numeric.value();
                if u32::from(numeric.scale()) <= Decimal::MAX_SCALE && value.unsigned_abs() <= MAX_DECIMAL_MANTISSA {
                    Value::Decimal(Decimal::from_i128_with_scale(value, u32::from(numeric.scale())))
                } else {
                    Value::Text(numeric_text(value, usize::from(numeric.scale())))
                }
            }
            None => Value::Null,
        },
        ColumnData::Date(_) => decoded_temporal(NaiveDate::from_sql(cd), Value::Date, "date"),
        ColumnData::Time(_) => decoded_temporal(NaiveTime::from_sql(cd), Value::Time, "time"),
        ColumnData::DateTime(_) | ColumnData::SmallDateTime(_) | ColumnData::DateTime2(_) => {
            decoded_temporal(NaiveDateTime::from_sql(cd), Value::DateTime, "datetime")
        }
        ColumnData::DateTimeOffset(stored) => {
            zoned::datetimeoffset_text(*stored).unwrap_or_else(|| undecodable("datetimeoffset"))
        }
        ColumnData::Xml(v) => v.as_ref().map(|x| Value::Text(x.to_string())).unwrap_or(Value::Null),
    }
}

pub(crate) fn column_data_to_value_for_type(cd: &ColumnData<'static>, column_type: ColumnType) -> Value {
    let value = column_data_to_value(cd);
    if value == Value::Null {
        return value;
    }
    match column_type {
        ColumnType::Money => Value::Undecodable("money".into()),
        ColumnType::Money4 => Value::Undecodable("smallmoney".into()),
        _ => value,
    }
}

fn numeric_text(value: i128, scale: usize) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let digits = format!("{:0>width$}", value.unsigned_abs(), width = scale + 1);
    let (whole, fraction) = digits.split_at(digits.len() - scale);
    if fraction.is_empty() {
        return format!("{sign}{whole}");
    }
    format!("{sign}{whole}.{fraction}")
}

/// tiberius decodes a temporal column in two steps: the outer `Result`
/// reports a value it could not convert, the inner `Option` reports SQL
/// NULL. Collapsing both to NULL would present an unreadable timestamp as
/// an editable empty cell and let it be written back as NULL.
fn decoded_temporal<T, E>(decoded: Result<Option<T>, E>, wrap: fn(T) -> Value, type_name: &str) -> Value {
    match decoded {
        Ok(Some(value)) => wrap(value),
        Ok(None) => Value::Null,
        Err(_) => undecodable(type_name),
    }
}

fn undecodable(type_name: &str) -> Value {
    tracing::warn!(
        type_name,
        "sql server column value could not be decoded; showing it as undecodable"
    );
    Value::Undecodable(type_name.to_string())
}

fn column_type_to_string(ct: ColumnType) -> String {
    let name = match ct {
        ColumnType::Null => "null",
        ColumnType::Bit | ColumnType::Bitn => "bit",
        ColumnType::Int1 => "tinyint",
        ColumnType::Int2 => "smallint",
        ColumnType::Int4 => "int",
        ColumnType::Int8 => "bigint",
        ColumnType::Intn => "int",
        ColumnType::Float4 => "real",
        ColumnType::Float8 | ColumnType::Floatn => "float",
        ColumnType::Decimaln | ColumnType::Numericn => "decimal",
        ColumnType::Money => "money",
        ColumnType::Money4 => "smallmoney",
        ColumnType::Datetime | ColumnType::Datetime4 | ColumnType::Datetimen => "datetime",
        ColumnType::Datetime2 => "datetime2",
        ColumnType::DatetimeOffsetn => "datetimeoffset",
        ColumnType::Daten => "date",
        ColumnType::Timen => "time",
        ColumnType::Guid => "uniqueidentifier",
        ColumnType::BigChar | ColumnType::BigVarChar => "varchar",
        ColumnType::NChar | ColumnType::NVarchar => "nvarchar",
        ColumnType::Text => "text",
        ColumnType::NText => "ntext",
        ColumnType::BigBinary | ColumnType::BigVarBin => "varbinary",
        ColumnType::Image => "image",
        ColumnType::Xml => "xml",
        ColumnType::Udt => "udt",
        ColumnType::SSVariant => "sql_variant",
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
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
}
