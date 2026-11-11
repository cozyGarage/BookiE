use bookie_core::{ColumnInfo, DriverValueInputCapabilities, EnumInputStyle, Value};
use chrono::{Datelike, Timelike};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueInputError {
    FieldRequired,
    InvalidEnumMetadata,
    UndeclaredEnumMember,
    InvalidBoolean,
    InvalidInteger,
    InvalidNumber,
    InvalidDecimal,
    InvalidUuid,
    InvalidJson(String),
    InvalidTimestamp,
    InvalidTimestampWithTimeZone,
    InvalidDateTime,
    InvalidDateAndTime,
    InvalidDate,
    InvalidTime,
    InvalidSqlServerDateTimeOffsetColumnScale,
    InvalidSqlServerDateTimeOffset,
    SqlServerDateTimeOffsetOutOfRange,
    MySqlSpatialValueNotEditable,
    MySqlIntegerOutOfRange,
    MongoInt32OutOfRange,
    MongoDatePrecision,
    DuckDbTimestampTzPrecision,
    DuckDbTemporalPrecision { kind: &'static str },
    BitWidthOutOfRange,
}

pub struct ParseHooks {
    pub enum_input_style: EnumInputStyle,
    pub capabilities: DriverValueInputCapabilities,
}

impl ParseHooks {
    pub fn without_driver(enum_input_style: EnumInputStyle) -> Self {
        Self::new(enum_input_style, DriverValueInputCapabilities::default())
    }

    pub fn new(enum_input_style: EnumInputStyle, capabilities: DriverValueInputCapabilities) -> Self {
        Self {
            enum_input_style,
            capabilities,
        }
    }
}

pub fn parse_input_for_column(text: &str, col: Option<&ColumnInfo>) -> Result<Value, ValueInputError> {
    let Some(col) = col else {
        return Ok(Value::Text(text.to_string()));
    };
    if text.is_empty() {
        if col.nullable || col.default_value.is_some() {
            return Ok(Value::Null);
        }
        if col.enum_type.is_none() && is_character_type(&col.data_type.to_ascii_lowercase()) {
            return Ok(Value::Text(String::new()));
        }
        return Err(ValueInputError::FieldRequired);
    }
    if col.enum_type.is_some() {
        return Ok(Value::Text(text.to_string()));
    }
    let dt = col.data_type.to_ascii_lowercase();
    let trimmed = text.trim();
    if let Some(width) = mysql_bit_width(&dt) {
        return parse_mysql_bit_value(trimmed, width);
    }
    match classify_type(&dt) {
        TypeKind::Bool => parse_bool_value(trimmed),
        TypeKind::Int => parse_int_value(trimmed),
        TypeKind::Float => parse_float_value(trimmed),
        TypeKind::Decimal => parse_decimal_value(trimmed),
        TypeKind::Uuid => parse_uuid_value(trimmed),
        TypeKind::Json => parse_json_value(trimmed),
        TypeKind::TimestampTz => parse_timestamptz_value(trimmed),
        TypeKind::DateTime => parse_datetime_value(trimmed),
        TypeKind::Date => parse_date_value(trimmed),
        TypeKind::Time => parse_time_value(trimmed),
        TypeKind::Text => Ok(Value::Text(text.to_string())),
    }
}

fn is_character_type(dt: &str) -> bool {
    dt.contains("char") || dt.contains("text") || dt == "string"
}

pub fn mysql_enum_set_labels(data_type: &str) -> Option<(bool, Vec<String>)> {
    let is_set = data_type.trim().to_ascii_lowercase().starts_with("set");
    crate::enum_input::declared_member_labels(data_type.trim()).map(|labels| (is_set, labels))
}

fn parse_enum_input(
    text: &str,
    col: Option<&ColumnInfo>,
    style: EnumInputStyle,
) -> Option<Result<Value, ValueInputError>> {
    crate::enum_input::parse_enum_input(text, col, style).map(|result| {
        result.map_err(|error| match error {
            crate::enum_input::EnumInputError::InvalidMemberDeclaration => ValueInputError::InvalidEnumMetadata,
            crate::enum_input::EnumInputError::UndeclaredMember => ValueInputError::UndeclaredEnumMember,
        })
    })
}

pub fn parse_input_for_driver_with_style(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Result<Value, ValueInputError> {
    if text == "''"
        && let Some(result) = parse_enum_input(text, col, hooks.enum_input_style)
    {
        return result;
    }
    if text.is_empty() {
        return parse_input_for_column(text, col);
    }
    if let Some(result) = parse_enum_input(text, col, hooks.enum_input_style) {
        return result;
    }
    let trimmed = text.trim();
    if hooks.capabilities.integer_range_validation
        && let Some(result) = parse_mysql_integer_input(trimmed, col)
    {
        return result;
    }
    if let Some(result) = parse_special_driver_input(trimmed, col, hooks) {
        return result;
    }
    if hooks.capabilities.numeric_affinity_input
        && col.is_some_and(|column| {
            classify_type(&column.data_type.to_ascii_lowercase()) == TypeKind::Text
                && bookie_core::sqlite_declared_type_has_numeric_affinity(&column.data_type)
        })
    {
        if let Some(value) = bookie_core::sqlite_affinity_decimal(text) {
            return Ok(Value::Decimal(value));
        }
        return if bookie_core::is_numeric_input(text) {
            Err(ValueInputError::InvalidNumber)
        } else {
            Ok(Value::Text(text.to_owned()))
        };
    }
    match parse_input_for_column(text, col) {
        Err(error) => match numeric_affinity_input_fallback(text, col, hooks) {
            Some(value) => Ok(value),
            None => Err(error),
        },
        result => result,
    }
}

fn numeric_affinity_input_fallback(text: &str, col: Option<&ColumnInfo>, hooks: &ParseHooks) -> Option<Value> {
    if !hooks.capabilities.numeric_affinity_input
        || !col.is_some_and(|column| {
            matches!(
                classify_type(&column.data_type.to_ascii_lowercase()),
                TypeKind::Int | TypeKind::Float | TypeKind::Decimal
            )
        })
    {
        return None;
    }
    if let Some(decimal) = bookie_core::sqlite_affinity_decimal(text) {
        return Some(Value::Decimal(decimal));
    }
    (!bookie_core::is_numeric_input(text)).then(|| Value::Text(text.to_owned()))
}

fn parse_special_driver_input(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Option<Result<Value, ValueInputError>> {
    parse_mssql_datetimeoffset_input(text, col, hooks)
        .or_else(|| parse_mongodb_integer_input(text, col, hooks))
        .or_else(|| parse_mysql_spatial_input(col, hooks))
        .or_else(|| {
            (hooks.capabilities.special_numeric_literals
                && col.is_some_and(|column| is_postgres_numeric_type(&column.data_type)))
            .then(|| match text {
                "NaN" | "Infinity" | "-Infinity" => Ok(Value::Text(text.into())),
                _ => match parse_decimal_value(text) {
                    Ok(value) => Ok(value),
                    Err(_) if is_postgres_numeric_literal(text) => Ok(Value::Text(text.into())),
                    Err(error) => Err(error),
                },
            })
        })
        .or_else(|| parse_extended_calendar_temporals_input(text, col, hooks))
        .or_else(|| parse_duckdb_date_input(text, col, hooks))
        .or_else(|| parse_mongodb_decimal_input(text, col, hooks))
        .or_else(|| parse_mongodb_date_input(text, col, hooks))
        .or_else(|| parse_timestamp_with_timezone_input(text, col, hooks))
        .or_else(|| parse_duckdb_temporal_input(text, col, hooks))
}

fn parse_mssql_datetimeoffset_input(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Option<Result<Value, ValueInputError>> {
    if !hooks.capabilities.datetimeoffset_input {
        return None;
    }
    let column = col?;
    let data_type = column.data_type.trim().to_ascii_lowercase();
    let suffix = data_type.strip_prefix("datetimeoffset")?;
    let scale = if suffix.is_empty() {
        Some(7)
    } else {
        suffix
            .strip_prefix('(')
            .and_then(|scale| scale.strip_suffix(')'))
            .and_then(|scale| scale.parse::<u8>().ok())
            .filter(|scale| *scale <= 7)
    };
    let Some(scale) = scale else {
        return Some(Err(ValueInputError::InvalidSqlServerDateTimeOffsetColumnScale));
    };
    let parsed = chrono::DateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f %:z");
    Some(parsed.map_or_else(
        |_| Err(ValueInputError::InvalidSqlServerDateTimeOffset),
        |value| {
            let local = text.rsplit_once(' ').map_or(text, |(local, _)| local);
            let fraction_digits = local.split_once('.').map_or(0, |(_, fraction)| fraction.len());
            let offset_seconds = value.offset().local_minus_utc().abs();
            let local_year = value.date_naive().year();
            let utc_year = value.naive_utc().date().year();
            if fraction_digits > usize::from(scale)
                || offset_seconds > 14 * 60 * 60
                || !(1..=9999).contains(&local_year)
                || !(1..=9999).contains(&utc_year)
            {
                return Err(ValueInputError::SqlServerDateTimeOffsetOutOfRange);
            }
            Ok(Value::Text(text.to_owned()))
        },
    ))
}

fn unchanged_empty_value(col: Option<&ColumnInfo>, current_value: Option<&Value>) -> Option<Value> {
    match current_value? {
        Value::Text(text)
            if text.is_empty()
                && col.is_some_and(|column| {
                    column.enum_type.is_none() && is_character_type(&column.data_type.to_ascii_lowercase())
                }) =>
        {
            Some(Value::Text(String::new()))
        }
        Value::Bytes(bytes) if bytes.is_empty() => Some(Value::Bytes(Vec::new())),
        _ => None,
    }
}

pub fn parse_input_for_grid_cell_with_style(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
    current_value: Option<&Value>,
) -> Result<Value, ValueInputError> {
    if text.is_empty()
        && let Some(unchanged) = unchanged_empty_value(col, current_value)
    {
        return Ok(unchanged);
    }
    if text.is_empty()
        && hooks.capabilities.heterogeneous_cell_values
        && col.is_some_and(|column| column.data_type.trim().eq_ignore_ascii_case("any"))
        && matches!(current_value, Some(Value::Text(_)))
    {
        return Ok(Value::Text(String::new()));
    }
    if !text.is_empty()
        && let Some(result) = parse_enum_input(text, col, hooks.enum_input_style)
    {
        return result;
    }
    if !text.is_empty()
        && hooks.capabilities.heterogeneous_cell_values
        && col.is_some_and(|column| column.data_type.trim().eq_ignore_ascii_case("any"))
    {
        match current_value {
            Some(Value::Int(_)) => return parse_int_value(text.trim()),
            Some(Value::Float(_)) => return parse_float_value(text.trim()),
            Some(Value::Text(_)) => return Ok(Value::Text(text.to_owned())),
            _ => {}
        }
    }
    parse_input_for_driver_with_style(text, col, hooks)
}

pub fn parse_grid_cell_input_with_style(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
    current_value: Option<&Value>,
) -> Result<Value, ValueInputError> {
    parse_input_for_grid_cell_with_style(text, col, hooks, current_value)
}

pub fn parse_pasted_cell_with_style(
    text: Option<&str>,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
    current_value: Option<&Value>,
) -> Result<Value, ValueInputError> {
    match text {
        None if col.is_none_or(|column| column.nullable) => Ok(Value::Null),
        None => Err(ValueInputError::FieldRequired),
        Some("")
            if col.is_none_or(|column| {
                column.enum_type.is_none() && is_character_type(&column.data_type.to_ascii_lowercase())
            }) =>
        {
            let required = col.map(|column| ColumnInfo {
                nullable: false,
                default_value: None,
                ..column.clone()
            });
            parse_grid_cell_input_with_style("", required.as_ref(), hooks, current_value)
        }
        Some(text) => parse_grid_cell_input_with_style(text, col, hooks, current_value),
    }
}

fn parse_extended_calendar_temporals_input(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Option<Result<Value, ValueInputError>> {
    let column = col?;
    if !hooks.capabilities.extended_calendar_temporals {
        return None;
    }
    let kind = column.data_type.trim().to_ascii_lowercase();
    if kind == "date" {
        if parse_date_value(text).is_ok() {
            return None;
        }
        return Some(if extended_duckdb_date(text).is_some() {
            Ok(Value::Text(text.to_owned()))
        } else {
            Err(ValueInputError::InvalidDate)
        });
    }
    if matches!(kind.as_str(), "timestamp" | "timestamp without time zone") {
        if parse_datetime_value(text).is_ok() {
            return None;
        }
        return Some(match extended_duckdb_timestamp_nanos(text) {
            Some(nanos) if nanos % 1_000 == 0 => Ok(Value::Text(text.replace('T', " "))),
            _ => Err(ValueInputError::InvalidDateTime),
        });
    }
    if matches!(kind.as_str(), "timestamptz" | "timestamp with time zone") {
        if parse_timestamptz_value(text).is_ok() {
            return None;
        }
        return Some(
            bookie_core::import::duckdb_extended_timestamptz_nanos(text)
                .filter(|nanos| nanos % 1_000 == 0)
                .map(|_| Value::Text(text.replace('T', " ")))
                .ok_or(ValueInputError::InvalidTimestampWithTimeZone),
        );
    }
    None
}

fn parse_mysql_spatial_input(col: Option<&ColumnInfo>, hooks: &ParseHooks) -> Option<Result<Value, ValueInputError>> {
    if !hooks.capabilities.spatial_edit_requires_native_value {
        return None;
    }
    let data_type = col?.data_type.trim().to_ascii_lowercase();
    let base = data_type.split('(').next()?.trim();
    matches!(
        base,
        "geometry"
            | "point"
            | "linestring"
            | "polygon"
            | "multipoint"
            | "multilinestring"
            | "multipolygon"
            | "geometrycollection"
    )
    .then(|| Err(ValueInputError::MySqlSpatialValueNotEditable))
}

fn parse_mysql_integer_input(text: &str, col: Option<&ColumnInfo>) -> Option<Result<Value, ValueInputError>> {
    let data_type = col?.data_type.trim().to_ascii_lowercase();
    let zerofill = data_type.strip_suffix(" zerofill");
    let data_type = zerofill.unwrap_or(&data_type);
    let unsigned = data_type.ends_with(" unsigned") || zerofill.is_some();
    let base = data_type
        .strip_suffix(" unsigned")
        .unwrap_or(data_type)
        .split('(')
        .next()?;

    if !unsigned && base == "tinyint" && data_type.contains("(1)") {
        return None;
    }

    let (minimum, maximum) = match (base, unsigned) {
        ("tinyint", false) => (i128::from(i8::MIN), i128::from(i8::MAX)),
        ("tinyint", true) => (0, i128::from(u8::MAX)),
        ("smallint", false) => (i128::from(i16::MIN), i128::from(i16::MAX)),
        ("smallint", true) => (0, i128::from(u16::MAX)),
        ("mediumint", false) => (-8_388_608, 8_388_607),
        ("mediumint", true) => (0, 16_777_215),
        ("int" | "integer", false) => (i128::from(i32::MIN), i128::from(i32::MAX)),
        ("int" | "integer", true) => (0, i128::from(u32::MAX)),
        ("bigint", true) => (0, i128::from(u64::MAX)),
        _ => return None,
    };
    Some((|| {
        let value = text.parse::<i128>().map_err(|_| ValueInputError::InvalidInteger)?;
        if !(minimum..=maximum).contains(&value) {
            return Err(ValueInputError::MySqlIntegerOutOfRange);
        }
        Ok(i64::try_from(value).map_or_else(|_| Value::Text(value.to_string()), Value::Int))
    })())
}

fn parse_mongodb_integer_input(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Option<Result<Value, ValueInputError>> {
    let column = col?;
    if !hooks.capabilities.typed_integer_widths {
        return None;
    }
    match column.data_type.trim().to_ascii_lowercase().as_str() {
        "int" => Some(text.parse::<i32>().map_or_else(
            |_| Err(ValueInputError::MongoInt32OutOfRange),
            |value| {
                Ok(Value::Json(Box::new(
                    serde_json::json!({"$numberInt": value.to_string()}),
                )))
            },
        )),
        "long" => Some(parse_int_value(text)),
        _ => None,
    }
}

fn parse_mongodb_decimal_input(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Option<Result<Value, ValueInputError>> {
    let column = col?;
    if hooks.capabilities.canonical_decimal_input.is_none() || !column.data_type.trim().eq_ignore_ascii_case("decimal")
    {
        return None;
    }
    if let Ok(value) = parse_decimal_value(text) {
        return Some(Ok(value));
    }
    Some(
        hooks.capabilities.canonical_decimal_input?(text)
            .map(|canonical| Value::Json(Box::new(serde_json::json!({"$numberDecimal": canonical}))))
            .ok_or(ValueInputError::InvalidDecimal),
    )
}

fn parse_mongodb_date_input(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Option<Result<Value, ValueInputError>> {
    let column = col?;
    if !hooks.capabilities.millisecond_date_precision || !column.data_type.trim().eq_ignore_ascii_case("date") {
        return None;
    }
    let timestamp = chrono::DateTime::parse_from_rfc3339(text).ok()?;
    Some(if timestamp.timestamp_subsec_nanos() % 1_000_000 == 0 {
        Ok(Value::TimestampTz(timestamp.to_utc()))
    } else {
        Err(ValueInputError::MongoDatePrecision)
    })
}

fn is_postgres_numeric_type(data_type: &str) -> bool {
    let data_type = data_type.trim().to_ascii_lowercase();
    matches!(data_type.as_str(), "numeric" | "decimal")
        || data_type.starts_with("numeric(")
        || data_type.starts_with("decimal(")
}

fn is_timestamp_with_timezone_type(data_type: &str) -> bool {
    matches!(
        data_type.trim().to_ascii_lowercase().as_str(),
        "timestamptz" | "timestamp with time zone"
    )
}

fn parse_timestamp_with_timezone_input(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Option<Result<Value, ValueInputError>> {
    let column = col?;
    if !hooks.capabilities.timestamp_with_timezone || !is_timestamp_with_timezone_type(&column.data_type) {
        return None;
    }
    Some((|| {
        if let Ok(Value::TimestampTz(timestamp)) = parse_timestamptz_value(text) {
            if timestamp.timestamp_subsec_nanos() % 1_000 != 0 {
                return Err(ValueInputError::DuckDbTimestampTzPrecision);
            }
            return Ok(Value::TimestampTz(timestamp));
        }
        let Some(nanos) = bookie_core::import::duckdb_extended_timestamptz_nanos(text) else {
            return Err(ValueInputError::InvalidTimestampWithTimeZone);
        };
        if nanos % 1_000 != 0 {
            return Err(ValueInputError::DuckDbTimestampTzPrecision);
        }
        Ok(Value::Text(text.to_owned()))
    })())
}

fn parse_duckdb_temporal_input(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Option<Result<Value, ValueInputError>> {
    let column = col?;
    if !hooks.capabilities.temporal_fraction_precision {
        return None;
    }
    let data_type = column.data_type.trim().to_ascii_lowercase();
    let (quantum_ns, kind) = match data_type.as_str() {
        "time" | "time without time zone" => (1_000, "TIME"),
        "timestamp" | "timestamp without time zone" => (1_000, "TIMESTAMP"),
        "timestamp_s" => (1_000_000_000, "TIMESTAMP_S"),
        "timestamp_ms" => (1_000_000, "TIMESTAMP_MS"),
        _ => return None,
    };
    Some((|| {
        let (value, nanos) = if kind == "TIME" {
            match parse_time_value(text)? {
                Value::Time(value) => {
                    let nanos = value.nanosecond();
                    (Value::Time(value), nanos)
                }
                _ => return Err(ValueInputError::InvalidTime),
            }
        } else if let Ok(Value::DateTime(value)) = parse_datetime_value(text) {
            let nanos = value.nanosecond();
            (Value::DateTime(value), nanos)
        } else if let Some(nanos) = extended_duckdb_timestamp_nanos(text) {
            (Value::Text(text.to_owned()), nanos)
        } else {
            return Err(ValueInputError::InvalidDateAndTime);
        };
        if nanos % quantum_ns != 0 {
            return Err(ValueInputError::DuckDbTemporalPrecision { kind });
        }
        Ok(value)
    })())
}

fn parse_duckdb_date_input(
    text: &str,
    col: Option<&ColumnInfo>,
    hooks: &ParseHooks,
) -> Option<Result<Value, ValueInputError>> {
    if !hooks.capabilities.extended_calendar_dates || !col?.data_type.trim().eq_ignore_ascii_case("date") {
        return None;
    }
    if parse_date_value(text).is_ok() {
        return None;
    }
    Some(if extended_duckdb_date(text).is_some() {
        Ok(Value::Text(text.to_owned()))
    } else {
        Err(ValueInputError::InvalidDate)
    })
}

fn extended_duckdb_timestamp_nanos(text: &str) -> Option<u32> {
    let (date, time) = if let Some((date, time)) = text.split_once(" (BC) ") {
        (format!("{date} (BC)"), time)
    } else {
        let (date, time) = text.split_once(' ')?;
        (date.to_owned(), time)
    };
    extended_duckdb_date(&date)?;
    let Value::Time(time) = parse_time_value(time).ok()? else {
        return None;
    };
    Some(time.nanosecond())
}

fn extended_duckdb_date(text: &str) -> Option<(i64, u32, u32, bool)> {
    let (date, bc) = text.strip_suffix(" (BC)").map_or((text, false), |date| (date, true));
    let mut parts = date.split('-');
    let year_text = parts.next()?;
    if year_text.len() < 4 || !year_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year = year_text.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || year == 0 || month == 0 || month > 12 {
        return None;
    }
    let astronomical_year = if bc { 1 - year } else { year };
    let leap = astronomical_year.rem_euclid(4) == 0
        && (astronomical_year.rem_euclid(100) != 0 || astronomical_year.rem_euclid(400) == 0);
    let days_in_month = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (day > 0 && day <= days_in_month && (bc || year > 9999)).then_some((year, month, day, bc))
}

fn is_postgres_numeric_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut cursor = usize::from(bytes.first().is_some_and(|byte| matches!(*byte, b'+' | b'-')));
    let integer_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let has_integer = cursor > integer_start;
    let mut has_fraction = false;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        has_fraction = cursor > fraction_start;
    }
    if !has_integer && !has_fraction {
        return false;
    }
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == exponent_start {
            return false;
        }
    }
    cursor == bytes.len()
}

fn mysql_bit_width(data_type: &str) -> Option<u32> {
    data_type
        .strip_prefix("bit(")?
        .strip_suffix(')')?
        .parse::<u32>()
        .ok()
        .filter(|width| (1..=64).contains(width))
}

fn parse_mysql_bit_value(text: &str, width: u32) -> Result<Value, ValueInputError> {
    if width == 1 {
        return parse_bool_value(text);
    }
    let number = text.parse::<u64>().map_err(|_| ValueInputError::InvalidInteger)?;
    let maximum = if width == 64 {
        i64::MAX as u64
    } else {
        (1u64 << width) - 1
    };
    if number > maximum {
        return Err(ValueInputError::BitWidthOutOfRange);
    }
    Ok(Value::Int(number as i64))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Bool,
    Int,
    Float,
    Decimal,
    Uuid,
    Json,
    TimestampTz,
    DateTime,
    Date,
    Time,
    Text,
}

pub fn classify_type(dt: &str) -> TypeKind {
    if dt.trim_end().ends_with("[]") {
        return TypeKind::Text;
    }
    if let Some(kind) = classify_bool_or_bit(dt) {
        return kind;
    }
    if dt.contains("uuid") {
        return TypeKind::Uuid;
    }
    if is_json_like(dt) {
        return TypeKind::Json;
    }
    if let Some(kind) = classify_temporal(dt) {
        return kind;
    }
    classify_numeric(dt).unwrap_or(TypeKind::Text)
}

fn classify_bool_or_bit(dt: &str) -> Option<TypeKind> {
    if matches!(dt, "bool" | "boolean" | "bit" | "bit(1)" | "tinyint(1)") {
        return Some(TypeKind::Bool);
    }
    if dt.starts_with("bit(") {
        return Some(TypeKind::Int);
    }
    None
}

fn is_json_like(dt: &str) -> bool {
    dt.contains("json")
        || dt.starts_with("bindata-subtype-")
        || matches!(
            dt,
            "object"
                | "array"
                | "objectid"
                | "bsontimestamp"
                | "regex"
                | "javascript"
                | "javascriptwithscope"
                | "symbol"
                | "undefined"
                | "dbpointer"
                | "minkey"
                | "maxkey"
        )
}

fn classify_temporal(dt: &str) -> Option<TypeKind> {
    if dt.contains("timestamptz") || dt.contains("with time zone") {
        return Some(TypeKind::TimestampTz);
    }
    if dt.contains("timestamp") || dt.contains("datetime") {
        return Some(TypeKind::DateTime);
    }
    if dt == "date" || (dt.starts_with("date") && !dt.contains("datetime") && !dt.contains("time")) {
        return Some(TypeKind::Date);
    }
    if dt == "time" || dt.starts_with("time(") || dt == "time without time zone" {
        return Some(TypeKind::Time);
    }
    None
}

fn classify_numeric(dt: &str) -> Option<TypeKind> {
    if matches!(dt, "decimal" | "numeric" | "money") || dt.starts_with("decimal(") || dt.starts_with("numeric(") {
        return Some(TypeKind::Decimal);
    }
    if matches!(dt, "float" | "double" | "real" | "double precision") || dt.starts_with("float(") {
        return Some(TypeKind::Float);
    }
    if is_integer_type(dt) {
        return Some(TypeKind::Int);
    }
    None
}

fn is_integer_type(dt: &str) -> bool {
    matches!(
        dt,
        "int"
            | "int2"
            | "int4"
            | "int8"
            | "integer"
            | "smallint"
            | "bigint"
            | "tinyint"
            | "mediumint"
            | "serial"
            | "bigserial"
            | "smallserial"
    ) || dt.starts_with("int(")
        || dt.starts_with("integer(")
        || dt.starts_with("smallint(")
        || dt.starts_with("bigint(")
        || dt.starts_with("tinyint(")
        || dt.starts_with("mediumint(")
}

pub fn parse_bool_value(text: &str) -> Result<Value, ValueInputError> {
    match text.to_ascii_lowercase().as_str() {
        "true" | "t" | "1" | "yes" | "y" | "on" => Ok(Value::Bool(true)),
        "false" | "f" | "0" | "no" | "n" | "off" => Ok(Value::Bool(false)),
        _ => Err(ValueInputError::InvalidBoolean),
    }
}

pub fn parse_int_value(text: &str) -> Result<Value, ValueInputError> {
    text.parse::<i64>()
        .map(Value::Int)
        .map_err(|_| ValueInputError::InvalidInteger)
}

pub fn parse_float_value(text: &str) -> Result<Value, ValueInputError> {
    bookie_core::parse_float_input(text)
        .map(Value::Float)
        .map_err(|_| ValueInputError::InvalidNumber)
}

pub fn parse_decimal_value(text: &str) -> Result<Value, ValueInputError> {
    rust_decimal::Decimal::from_str_exact(text)
        .map(Value::Decimal)
        .map_err(|_| ValueInputError::InvalidDecimal)
}

pub fn parse_uuid_value(text: &str) -> Result<Value, ValueInputError> {
    uuid::Uuid::parse_str(text)
        .map(Value::Uuid)
        .map_err(|_| ValueInputError::InvalidUuid)
}

pub fn parse_json_value(text: &str) -> Result<Value, ValueInputError> {
    serde_json::from_str::<serde_json::Value>(text)
        .map(Box::new)
        .map(Value::Json)
        .map_err(|e| ValueInputError::InvalidJson(e.to_string()))
}

pub fn parse_timestamptz_value(text: &str) -> Result<Value, ValueInputError> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(text) {
        return Ok(Value::TimestampTz(dt.with_timezone(&chrono::Utc)));
    }
    Err(ValueInputError::InvalidTimestamp)
}

pub fn parse_datetime_value(text: &str) -> Result<Value, ValueInputError> {
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ];
    for fmt in &formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(text, fmt) {
            return Ok(Value::DateTime(dt));
        }
    }
    Err(ValueInputError::InvalidDateTime)
}

pub fn parse_date_value(text: &str) -> Result<Value, ValueInputError> {
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map(Value::Date)
        .map_err(|_| ValueInputError::InvalidDate)
}

pub fn parse_time_value(text: &str) -> Result<Value, ValueInputError> {
    let formats = ["%H:%M:%S", "%H:%M:%S%.f", "%H:%M"];
    for fmt in &formats {
        if let Ok(t) = chrono::NaiveTime::parse_from_str(text, fmt) {
            return Ok(Value::Time(t));
        }
    }
    Err(ValueInputError::InvalidTime)
}
