use crate::query::{ColumnInfo, Value};
use crate::sql_dialect::{BuildSqlError, quote_ident};
use chrono::{Datelike, Timelike};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LiteralError {
    #[error("the value could not be decoded")]
    Undecodable,
    #[error("non-finite numbers cannot be exported as SQL literals")]
    NonFinite,
    #[error("this engine does not support this SQL literal")]
    Unsupported,
}

/// Select the coarsest `DateTime64` scale that represents a timestamp exactly
/// and fits ClickHouse's Int64 tick storage and calendar bounds.
pub fn clickhouse_datetime64_precision(stamp: chrono::NaiveDateTime) -> Option<u32> {
    (0..=9).find(|precision| clickhouse_datetime64_fits_precision(stamp, *precision))
}

/// Render a timestamp as a precision-preserving ClickHouse DateTime64 literal.
/// An optional timezone is included in the native expression.
pub fn clickhouse_datetime64_literal(stamp: chrono::NaiveDateTime, timezone: Option<&str>) -> Option<String> {
    let precision = clickhouse_datetime64_precision(stamp)?;
    let value = if precision == 0 {
        stamp.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        let divisor = 10_u32.pow(9 - precision);
        let fractional = stamp.nanosecond() / divisor;
        format!(
            "{}.{fractional:0width$}",
            stamp.format("%Y-%m-%d %H:%M:%S"),
            width = precision as usize
        )
    };
    match timezone {
        Some(zone) => Some(format!("toDateTime64('{value}', {precision}, '{zone}')")),
        None => Some(format!("toDateTime64('{value}', {precision})")),
    }
}

fn clickhouse_datetime64_fits_precision(stamp: chrono::NaiveDateTime, precision: u32) -> bool {
    let Some(minimum) = chrono::NaiveDate::from_ymd_opt(1900, 1, 1).and_then(|date| date.and_hms_opt(0, 0, 0)) else {
        return false;
    };
    let Some(maximum) =
        chrono::NaiveDate::from_ymd_opt(2299, 12, 31).and_then(|date| date.and_hms_nano_opt(23, 59, 59, 999_999_999))
    else {
        return false;
    };
    if !(minimum..=maximum).contains(&stamp) || precision > 9 {
        return false;
    }
    let fraction_divisor = 10_u32.pow(9 - precision);
    if !stamp.nanosecond().is_multiple_of(fraction_divisor) {
        return false;
    }
    let scale = 10_i128.pow(precision);
    let ticks = i128::from(stamp.and_utc().timestamp()) * scale + i128::from(stamp.nanosecond() / fraction_divisor);
    (i128::from(i64::MIN)..=i128::from(i64::MAX)).contains(&ticks)
}

/// Render `value` as a SQL literal for `driver_id`.
///
/// Escaping is dialect-specific and getting it wrong is not cosmetic.
/// MySQL and ClickHouse treat a backslash as an escape character inside
/// a string literal, so a value ending in one would consume the closing
/// quote and let the rest of the value parse as SQL. PostgreSQL's regular
/// string behavior depends on `standard_conforming_strings`; backslash-bearing
/// literals use explicit `E''` syntax so their value is independent of session
/// settings. SQLite and SQL Server treat backslash as an ordinary character.
///
/// Row values come from the database, which the project treats as
/// untrusted input.
pub fn render_sql_literal(driver_id: &str, value: &Value) -> Result<String, LiteralError> {
    if !crate::export::supports_sql_literals(driver_id) {
        return Err(LiteralError::Unsupported);
    }
    if driver_id == "postgres"
        && let Some(text) = postgres_extended_date(value)
    {
        return Ok(string_literal(driver_id, &text));
    }
    Ok(match value {
        Value::Null => "NULL".into(),
        Value::Bool(value) if driver_id == "mssql" => u8::from(*value).to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) if !value.is_finite() => return Err(LiteralError::NonFinite),
        Value::Float(value) if driver_id == "sqlite" && *value == 0.0 && value.is_sign_negative() => {
            return Err(LiteralError::Unsupported);
        }
        Value::Float(value) => format!("{value:e}"),
        Value::Decimal(value) if driver_id == "duckdb" => {
            let precision = value
                .mantissa()
                .unsigned_abs()
                .to_string()
                .len()
                .max(value.scale() as usize)
                .max(1);
            format!("CAST({value} AS DECIMAL({precision}, {}))", value.scale())
        }
        Value::Decimal(value) if driver_id == "clickhouse" => format!("toDecimal128('{value}', {})", value.scale()),
        Value::Decimal(value) => value.to_string(),
        Value::Text(text) => string_literal(driver_id, text),
        Value::Bytes(bytes) => binary_literal(driver_id, bytes)?,
        Value::Date(date) => string_literal(driver_id, &date.format("%Y-%m-%d").to_string()),
        Value::Time(time) => string_literal(driver_id, &time.to_string()),
        Value::DateTime(stamp) if driver_id == "clickhouse" => {
            clickhouse_datetime64_literal(*stamp, None).ok_or(LiteralError::Unsupported)?
        }
        Value::DateTime(stamp) if driver_id == "mssql" => {
            format!("CAST({} AS datetime2)", string_literal(driver_id, &stamp.to_string()))
        }
        Value::TimestampTz(stamp) if driver_id == "clickhouse" => {
            clickhouse_datetime64_literal(stamp.naive_utc(), Some("UTC")).ok_or(LiteralError::Unsupported)?
        }
        Value::DateTime(stamp) => string_literal(driver_id, &stamp.to_string()),
        Value::TimestampTz(stamp) => string_literal(driver_id, &stamp.to_rfc3339()),
        Value::Uuid(id) => string_literal(driver_id, &id.to_string()),
        Value::Json(_) if driver_id == "clickhouse" => return Err(LiteralError::Unsupported),
        Value::Json(json) => string_literal(driver_id, &json.to_string()),
        Value::JsonText(json) => string_literal(driver_id, json),
        Value::Undecodable(_) => return Err(LiteralError::Undecodable),
    })
}

fn postgres_extended_date(value: &Value) -> Option<String> {
    let date = match value {
        Value::Date(date) => *date,
        Value::DateTime(stamp) => stamp.date(),
        Value::TimestampTz(stamp) => stamp.date_naive(),
        _ => return None,
    };
    if (1..=9999).contains(&date.year()) {
        return None;
    }
    postgres_temporal_text(value)
}

pub fn postgres_temporal_text(value: &Value) -> Option<String> {
    let (date, time, zone) = match value {
        Value::Date(date) => (*date, None, ""),
        Value::DateTime(stamp) => (stamp.date(), Some(stamp.time()), ""),
        Value::TimestampTz(stamp) => (stamp.date_naive(), Some(stamp.time()), "+00:00"),
        _ => return None,
    };
    let (common_era, year) = date.year_ce();
    let mut text = format!("{year:04}-{:02}-{:02}", date.month(), date.day());
    if let Some(time) = time {
        text.push_str(&format!(" {time}{zone}"));
    }
    if !common_era {
        text.push_str(" BC");
    }
    Some(text)
}

fn string_literal(driver_id: &str, text: &str) -> String {
    // MySQL reads a backslash as an escape unless sql_mode has NO_BACKSLASH_ESCAPES, and a
    // generated INSERT cannot know the mode of the server it will run on. A charset-tagged
    // hex literal means the same text in both modes, where any quoted form breaks one of them.
    if driver_id == "mysql" && text.contains(['\\', '\0']) {
        return format!("_utf8mb4 X'{}'", crate::export::hex_encode(text.as_bytes()));
    }
    if driver_id == "sqlite" && text.contains('\0') {
        return format!("CAST(X'{}' AS TEXT)", crate::export::hex_encode(text.as_bytes()));
    }
    if driver_id == "postgres" && text.contains('\\') {
        let escaped = text.replace('\\', "\\\\").replace('\'', "''");
        return format!("E'{escaped}'");
    }
    quote_literal(driver_id, text)
}

fn binary_literal(driver_id: &str, bytes: &[u8]) -> Result<String, LiteralError> {
    let hex = crate::export::hex_encode(bytes);
    match driver_id {
        "postgres" => Ok(format!("decode('{hex}', 'hex')")),
        "mysql" | "sqlite" => Ok(format!("X'{hex}'")),
        "mssql" => Ok(format!("0x{hex}")),
        "clickhouse" => Ok(format!("unhex('{hex}')")),
        "duckdb" => Ok(format!("from_hex('{hex}')")),
        _ => Err(LiteralError::Unsupported),
    }
}

pub(crate) fn quote_literal(driver_id: &str, text: &str) -> String {
    let escaped = match driver_id {
        "mysql" | "clickhouse" => text.replace('\\', "\\\\").replace('\'', "''"),
        _ => text.replace('\'', "''"),
    };
    let prefix = if driver_id == "mssql" { "N" } else { "" };
    format!("{prefix}'{escaped}'")
}

/// Render a complete `INSERT` for one result row, for the user to paste
/// and run.
///
/// Generated columns are left out because the database computes them.
/// PostgreSQL and SQL Server identities are also left out so the target
/// database assigns a fresh key; MySQL's explicit auto-increment copy stays.
pub fn build_insert_literal(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    row: &[Value],
) -> Result<String, BuildSqlError> {
    if columns.len() != row.len() {
        return Err(BuildSqlError::LengthMismatch {
            expected: columns.len(),
            got: row.len(),
        });
    }
    let mut names: Vec<String> = Vec::with_capacity(columns.len());
    let mut values: Vec<String> = Vec::with_capacity(columns.len());
    for (column, value) in columns.iter().zip(row) {
        if column.is_generated || (column.is_auto_increment && matches!(driver_id, "postgres" | "mssql")) {
            continue;
        }
        names.push(quote_ident(driver_id, &column.name));
        values.push(column_literal(driver_id, column, value)?);
    }
    let qualified = qualified_table_name(driver_id, schema, table);
    if names.is_empty() {
        return if matches!(driver_id, "postgres" | "mssql") {
            Ok(format!("INSERT INTO {qualified} DEFAULT VALUES;"))
        } else {
            Err(BuildSqlError::NothingToUpdate)
        };
    }
    Ok(format!(
        "INSERT INTO {} ({}) VALUES ({});",
        qualified,
        names.join(", "),
        values.join(", ")
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowStatement {
    Insert,
    Update,
    Delete,
}

impl RowStatement {
    pub fn build(
        self,
        driver_id: &str,
        schema: Option<&str>,
        table: &str,
        columns: &[ColumnInfo],
        row: &[Value],
    ) -> Result<String, BuildSqlError> {
        match self {
            Self::Insert => build_insert_literal(driver_id, schema, table, columns, row),
            Self::Update => build_update_literal(driver_id, schema, table, columns, row),
            Self::Delete => build_delete_literal(driver_id, schema, table, columns, row),
        }
    }
}

fn qualified_table_name(driver_id: &str, schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(schema) => format!("{}.{}", quote_ident(driver_id, schema), quote_ident(driver_id, table)),
        None => quote_ident(driver_id, table),
    }
}

fn column_literal(driver_id: &str, column: &ColumnInfo, value: &Value) -> Result<String, BuildSqlError> {
    if driver_id == "mysql" && matches!(value, Value::Text(text) if text.is_empty()) {
        // MariaDB's EMPTY_STRING_IS_NULL mode turns an empty SQL string
        // literal into NULL. SPACE(0) stays an empty value for text,
        // ENUM, and SET destinations during SQL-file replay.
        return Ok("SPACE(0)".to_owned());
    }
    render_sql_literal(driver_id, value).map_err(|_| BuildSqlError::UnrepresentableValue {
        column: column.name.clone(),
    })
}

fn primary_key_predicate(driver_id: &str, columns: &[ColumnInfo], row: &[Value]) -> Result<String, BuildSqlError> {
    if columns.len() != row.len() {
        return Err(BuildSqlError::LengthMismatch {
            expected: columns.len(),
            got: row.len(),
        });
    }
    let mut terms = Vec::new();
    for (column, value) in columns.iter().zip(row).filter(|(column, _)| column.primary_key) {
        if matches!(value, Value::Null) {
            return Err(BuildSqlError::UnrepresentableValue {
                column: column.name.clone(),
            });
        }
        terms.push(format!(
            "{} = {}",
            quote_ident(driver_id, &column.name),
            column_literal(driver_id, column, value)?
        ));
    }
    if terms.is_empty() {
        return Err(BuildSqlError::NoPrimaryKey);
    }
    Ok(terms.join(" AND "))
}

pub fn build_update_literal(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    row: &[Value],
) -> Result<String, BuildSqlError> {
    let predicate = primary_key_predicate(driver_id, columns, row)?;
    let mut assignments = Vec::new();
    for (column, value) in columns.iter().zip(row) {
        if column.primary_key || column.is_generated {
            continue;
        }
        assignments.push(format!(
            "{} = {}",
            quote_ident(driver_id, &column.name),
            column_literal(driver_id, column, value)?
        ));
    }
    if assignments.is_empty() {
        return Err(BuildSqlError::NothingToUpdate);
    }
    Ok(format!(
        "UPDATE {} SET {} WHERE {};",
        qualified_table_name(driver_id, schema, table),
        assignments.join(", "),
        predicate
    ))
}

pub fn build_delete_literal(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    row: &[Value],
) -> Result<String, BuildSqlError> {
    let predicate = primary_key_predicate(driver_id, columns, row)?;
    Ok(format!(
        "DELETE FROM {} WHERE {};",
        qualified_table_name(driver_id, schema, table),
        predicate
    ))
}

#[cfg(test)]
#[path = "sql_literal_tests.rs"]
mod tests;
