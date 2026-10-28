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
        enum_type: None,
        domain_type: None,
    }
}

/// rust_decimal::Decimal stores its mantissa in a 96-bit unsigned
/// integer -- 2^96 - 1, the crate's own stable, documented capacity.
/// `Decimal::from_i128_with_scale` panics above this instead of
/// returning a `Result`, so it must be checked before calling.
const MAX_DECIMAL_MANTISSA: u128 = 0x0000_0000_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF;

fn column_data_to_value(cd: &ColumnData<'static>) -> Value {
    if is_inexact_legacy_datetime_payload(cd)
        && let ColumnData::DateTime(Some(value)) = cd
    {
        return legacy_datetime_text(*value)
            .map(Value::Text)
            .unwrap_or_else(|| undecodable("datetime"));
    }
    match cd {
        ColumnData::Bit(v) => (*v).map(Value::Bool).unwrap_or(Value::Null),
        ColumnData::U8(v) => (*v).map(|n| Value::Int(i64::from(n))).unwrap_or(Value::Null),
        ColumnData::I16(v) => (*v).map(|n| Value::Int(i64::from(n))).unwrap_or(Value::Null),
        ColumnData::I32(v) => (*v).map(|n| Value::Int(i64::from(n))).unwrap_or(Value::Null),
        ColumnData::I64(v) => (*v).map(Value::Int).unwrap_or(Value::Null),
        ColumnData::F32(v) => (*v)
            .map(|n| Value::Float(tablepro_core::single_precision_as_f64(n)))
            .unwrap_or(Value::Null),
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

/// Legacy DATETIME uses 1/300-second ticks; every third tick is exact in
/// nanoseconds. `DATETIMN` metadata also covers SMALLDATETIME, so inspect the
/// payload variant instead of refusing based on column metadata.
fn is_inexact_legacy_datetime_payload(cd: &ColumnData<'static>) -> bool {
    matches!(cd, ColumnData::DateTime(Some(value)) if value.seconds_fragments() % 3 != 0)
}

fn legacy_datetime_text(value: tiberius::time::DateTime) -> Option<String> {
    let date =
        NaiveDate::from_ymd_opt(1900, 1, 1)?.checked_add_signed(chrono::Duration::days(i64::from(value.days())))?;
    // SQL Server style 126 rounds 1/300-second ticks to the nearest millisecond.
    let milliseconds = (u64::from(value.seconds_fragments()) * 10 + 1) / 3;
    let seconds = u32::try_from(milliseconds / 1_000).ok()?;
    let time = NaiveTime::from_num_seconds_from_midnight_opt(seconds, 0)?;
    let timestamp = date.and_time(time);
    let base = timestamp.format("%Y-%m-%dT%H:%M:%S");
    let fraction = milliseconds % 1_000;
    Some(if fraction == 0 {
        base.to_string()
    } else {
        format!("{base}.{fraction:03}")
    })
}

pub(crate) fn column_data_to_value_for_type(cd: &ColumnData<'static>, column_type: ColumnType) -> Value {
    let value = column_data_to_value(cd);
    if value == Value::Null {
        return value;
    }
    match column_type {
        ColumnType::SSVariant => Value::Undecodable("sql_variant".into()),
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
#[path = "codec_tests.rs"]
mod tests;
