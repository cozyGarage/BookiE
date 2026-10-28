use thiserror::Error;

use crate::{ColumnInfo, Value};

#[cfg(test)]
#[path = "sql_dialect/postgres_enum_tests.rs"]
mod postgres_enum_tests;

mod error;
mod exact_original;
mod inserts;
mod pg_native_text;
#[path = "sql_dialect/updates.rs"]
mod updates;
pub(crate) use pg_native_text::{
    postgres_array_cast_type, postgres_native_text_cast_type, postgres_temporal_cast_type, text_cast_placeholder,
};

pub use error::BuildSqlError;
pub use inserts::{build_insert_from_draft, build_insert_from_draft_touched};
pub use updates::{
    build_keyed_update, build_keyed_value_select, build_mongodb_keyed_delete, build_mongodb_keyed_update,
    build_optimistic_keyed_update,
};

pub const MAX_IDENT_BYTES: usize = 256;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdentError {
    #[error("identifier is empty")]
    Empty,

    #[error("identifier is longer than {MAX_IDENT_BYTES} bytes")]
    TooLong,

    #[error("identifier contains a control character")]
    ControlCharacter,
}

pub fn validate_ident(name: &str) -> Result<(), IdentError> {
    if name.trim().is_empty() {
        return Err(IdentError::Empty);
    }
    if name.len() > MAX_IDENT_BYTES {
        return Err(IdentError::TooLong);
    }
    if name
        .chars()
        .any(|character| character.is_control() || character == '\u{2028}' || character == '\u{2029}')
    {
        return Err(IdentError::ControlCharacter);
    }
    Ok(())
}

pub fn quote_ident(driver_id: &str, name: &str) -> String {
    match driver_id {
        // ClickHouse backtick-quoted identifiers treat a backslash as an
        // escape character, exactly like its string literals (sql_literal.rs);
        // a table named `evil\` would otherwise consume the closing backtick.
        "clickhouse" => format!("`{}`", name.replace('\\', "\\\\").replace('`', "``")),
        "mysql" => format!("`{}`", name.replace('`', "``")),
        "mssql" => format!("[{}]", name.replace(']', "]]")),
        _ => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

pub fn placeholder_for(driver_id: &str, index: usize) -> String {
    match driver_id {
        "postgres" => format!("${}", index + 1),
        "mssql" => format!("@P{}", index + 1),
        _ => "?".to_string(),
    }
}

fn postgres_text_cast_type(column: &ColumnInfo, value: &Value) -> Option<String> {
    if let Some(domain_type) = &column.domain_type
        && matches!(value, Value::Text(_) | Value::Null)
    {
        return Some(format!(
            "{}.{}",
            quote_ident("postgres", &domain_type.schema),
            quote_ident("postgres", &domain_type.name)
        ));
    }
    if let Some(enum_type) = &column.enum_type
        && matches!(value, Value::Text(_) | Value::Null)
    {
        let array_suffix = if column.data_type.trim().ends_with("[]") {
            "[]"
        } else {
            ""
        };
        return Some(format!(
            "{}.{}{}",
            quote_ident("postgres", &enum_type.schema),
            quote_ident("postgres", &enum_type.name),
            array_suffix
        ));
    }
    if !matches!(value, Value::Text(_)) {
        return None;
    }
    postgres_array_cast_type(&column.data_type)
        .or_else(|| postgres_temporal_cast_type(&column.data_type))
        .or_else(|| postgres_numeric_cast_type(&column.data_type))
        .map(str::to_owned)
        .or_else(|| postgres_native_text_cast_type(&column.data_type))
}

pub fn explain_statement(driver_id: &str, sql: &str) -> Option<String> {
    let prefix = match driver_id {
        "postgres" | "mysql" | "clickhouse" | "duckdb" => "EXPLAIN ",
        "sqlite" => "EXPLAIN QUERY PLAN ",
        _ => return None,
    };
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(format!("{prefix}{trimmed}"))
}

/// Render an `UPDATE`. ClickHouse only accepted standard `UPDATE`
/// syntax from 25.7; the spelling that works across every supported
/// release is `ALTER TABLE … UPDATE`, which the server applies as a
/// mutation. `qualified_table`, `set_clause` and `where_clause` are
/// pre-built SQL, not identifiers.
pub fn build_update(driver_id: &str, qualified_table: &str, set_clause: &str, where_clause: &str) -> String {
    match driver_id {
        "clickhouse" => format!("ALTER TABLE {qualified_table} UPDATE {set_clause} WHERE {where_clause}"),
        _ => format!("UPDATE {qualified_table} SET {set_clause} WHERE {where_clause}"),
    }
}

/// Render the `ORDER BY` and row-window tail of a paged `SELECT`,
/// including the leading space. The two clauses are built together
/// because SQL Server couples them: `OFFSET … FETCH` is defined as a
/// suffix of `ORDER BY`, so a paged query with no user sort still
/// needs one. `(SELECT NULL)` is the no-op ordering that satisfies the
/// parser without imposing a sort the user did not ask for.
///
/// `order_by` is pre-quoted SQL (`"name" ASC, "id" DESC`), not an
/// identifier.
pub fn build_order_and_pagination(driver_id: &str, order_by: Option<&str>, limit: u64, offset: u64) -> String {
    let order_by = order_by.map(str::trim).filter(|o| !o.is_empty());
    if driver_id == "mssql" {
        let order = order_by.unwrap_or("(SELECT NULL)");
        return format!(" ORDER BY {order} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY");
    }
    match order_by {
        Some(order) => format!(" ORDER BY {order} LIMIT {limit} OFFSET {offset}"),
        None => format!(" LIMIT {limit} OFFSET {offset}"),
    }
}

pub fn build_single_cell_update(
    driver_id: &str,
    table: &str,
    columns: &[ColumnInfo],
    original_row: &[Value],
    col_index: usize,
    new_value: Value,
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let pk_indexes = collect_pk_indexes(columns);
    if pk_indexes.is_empty() {
        return Err(BuildSqlError::NoPrimaryKey);
    }
    if original_row.len() != columns.len() {
        return Err(BuildSqlError::LengthMismatch {
            expected: columns.len(),
            got: original_row.len(),
        });
    }

    let mut params: Vec<Value> = Vec::with_capacity(1 + pk_indexes.len());
    let mut placeholder_idx = 0;

    let set_clause = format!(
        "{} = {}",
        quote_ident(driver_id, &columns[col_index].name),
        placeholder_for(driver_id, placeholder_idx)
    );
    placeholder_idx += 1;
    params.push(new_value);

    let where_clause = build_where_clause(
        driver_id,
        columns,
        &pk_indexes,
        original_row,
        &mut placeholder_idx,
        &mut params,
    );

    let sql = build_update(driver_id, &quote_ident(driver_id, table), &set_clause, &where_clause);
    Ok((sql, params))
}

pub fn build_full_row_update(
    driver_id: &str,
    table: &str,
    columns: &[ColumnInfo],
    original_row: &[Value],
    new_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let pk_indexes = collect_pk_indexes(columns);
    if pk_indexes.is_empty() {
        return Err(BuildSqlError::NoPrimaryKey);
    }
    if new_values.len() != columns.len() {
        return Err(BuildSqlError::LengthMismatch {
            expected: columns.len(),
            got: new_values.len(),
        });
    }
    if original_row.len() != columns.len() {
        return Err(BuildSqlError::LengthMismatch {
            expected: columns.len(),
            got: original_row.len(),
        });
    }

    let mut params: Vec<Value> = Vec::new();
    let mut placeholder_idx = 0;

    let mut set_clauses = Vec::new();
    for (i, col) in columns.iter().enumerate() {
        if col.primary_key {
            continue;
        }
        set_clauses.push(format!(
            "{} = {}",
            quote_ident(driver_id, &col.name),
            placeholder_for(driver_id, placeholder_idx)
        ));
        placeholder_idx += 1;
        params.push(new_values[i].clone());
    }
    if set_clauses.is_empty() {
        return Err(BuildSqlError::NothingToUpdate);
    }

    let where_clause = build_where_clause(
        driver_id,
        columns,
        &pk_indexes,
        original_row,
        &mut placeholder_idx,
        &mut params,
    );

    let sql = build_update(
        driver_id,
        &quote_ident(driver_id, table),
        &set_clauses.join(", "),
        &where_clause,
    );
    Ok((sql, params))
}

pub(crate) fn postgres_numeric_cast_type(data_type: &str) -> Option<&'static str> {
    if data_type.len() > 64 {
        return None;
    }
    let lowered = data_type.trim().to_ascii_lowercase();
    let (base, modifier) = match lowered.split_once('(') {
        Some((base, modifier)) => (base.trim(), Some(modifier.strip_suffix(')')?)),
        None => (lowered.as_str(), None),
    };
    if !matches!(base, "numeric" | "decimal") {
        return None;
    }
    if let Some(modifier) = modifier {
        let values: Vec<_> = modifier.split(',').map(str::trim).collect();
        if !matches!(values.len(), 1 | 2) || values.iter().any(|value| value.parse::<i32>().is_err()) {
            return None;
        }
    }
    Some("pg_catalog.numeric")
}

pub fn build_keyed_delete(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    pk_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let pk_indexes = checked_pk_indexes(columns, pk_values)?;
    let mut params: Vec<Value> = Vec::with_capacity(pk_values.len());
    let where_clause = keyed_where_clause(driver_id, columns, &pk_indexes, pk_values, &mut params);
    let qualified = qualified_table(driver_id, schema, table);
    Ok((format!("DELETE FROM {qualified} WHERE {where_clause}"), params))
}

pub(super) fn checked_pk_indexes(columns: &[ColumnInfo], pk_values: &[Value]) -> Result<Vec<usize>, BuildSqlError> {
    let pk_indexes = collect_pk_indexes(columns);
    if pk_indexes.is_empty() {
        return Err(BuildSqlError::NoPrimaryKey);
    }
    if pk_values.len() != pk_indexes.len() {
        return Err(BuildSqlError::StaleColumns);
    }
    Ok(pk_indexes)
}

pub(super) fn keyed_where_clause(
    driver_id: &str,
    columns: &[ColumnInfo],
    pk_indexes: &[usize],
    pk_values: &[Value],
    params: &mut Vec<Value>,
) -> String {
    let clauses: Vec<String> = pk_indexes
        .iter()
        .zip(pk_values)
        .map(|(&col_idx, value)| {
            let ident = quote_ident(driver_id, &columns[col_idx].name);
            if matches!(value, Value::Null) {
                return format!("{ident} IS NULL");
            }
            let placeholder = placeholder_for(driver_id, params.len());
            let value_sql = if driver_id == "postgres" {
                text_cast_placeholder(&columns[col_idx], value, placeholder)
            } else {
                placeholder
            };
            params.push(value.clone());
            format!("{ident} = {value_sql}")
        })
        .collect();
    clauses.join(" AND ")
}

pub(super) fn qualified_table(driver_id: &str, schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(s) => format!("{}.{}", quote_ident(driver_id, s), quote_ident(driver_id, table)),
        None => quote_ident(driver_id, table),
    }
}

fn collect_pk_indexes(columns: &[ColumnInfo]) -> Vec<usize> {
    columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.primary_key)
        .map(|(i, _)| i)
        .collect()
}

fn build_where_clause(
    driver_id: &str,
    columns: &[ColumnInfo],
    pk_indexes: &[usize],
    original_row: &[Value],
    placeholder_idx: &mut usize,
    params: &mut Vec<Value>,
) -> String {
    let mut clauses = Vec::with_capacity(pk_indexes.len());
    for pk_col in pk_indexes {
        let ident = quote_ident(driver_id, &columns[*pk_col].name);
        // SQL three-valued logic: `col = NULL` is never true. A nullable
        // PK component holding NULL must use `IS NULL` or the UPDATE /
        // DELETE silently matches zero rows.
        if matches!(original_row[*pk_col], Value::Null) {
            clauses.push(format!("{ident} IS NULL"));
        } else {
            clauses.push(format!("{ident} = {}", placeholder_for(driver_id, *placeholder_idx)));
            *placeholder_idx += 1;
            params.push(original_row[*pk_col].clone());
        }
    }
    clauses.join(" AND ")
}

#[cfg(test)]
#[path = "sql_dialect_tests.rs"]
mod tests;
