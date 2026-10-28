use super::{
    BuildSqlError, build_update, checked_pk_indexes, exact_original::exact_original_predicate, keyed_where_clause,
    placeholder_for, postgres_text_cast_type, qualified_table, quote_ident, text_cast_placeholder,
};
use crate::{ColumnInfo, Value};

pub fn build_keyed_value_select(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    value_column: usize,
    pk_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let pk_indexes = checked_pk_indexes(columns, pk_values)?;
    let value_column = columns.get(value_column).ok_or(BuildSqlError::StaleColumns)?;
    let mut params = Vec::with_capacity(pk_values.len());
    let where_clause = keyed_value_where_clause(driver_id, columns, &pk_indexes, pk_values, &mut params);
    let order = pk_indexes
        .iter()
        .map(|&index| format!("{} ASC", quote_ident(driver_id, &columns[index].name)))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {} FROM {} WHERE {where_clause}{}",
        quote_ident(driver_id, &value_column.name),
        qualified_table(driver_id, schema, table),
        super::build_order_and_pagination(driver_id, Some(&order), 2, 0),
    );
    Ok((sql, params))
}

fn keyed_value_where_clause(
    driver_id: &str,
    columns: &[ColumnInfo],
    pk_indexes: &[usize],
    pk_values: &[Value],
    params: &mut Vec<Value>,
) -> String {
    pk_indexes
        .iter()
        .zip(pk_values)
        .map(|(&index, value)| {
            let column = &columns[index];
            let name = quote_ident(driver_id, &column.name);
            if matches!(value, Value::Null) {
                return format!("{name} IS NULL");
            }
            let placeholder = placeholder_for(driver_id, params.len());
            let typed = if driver_id == "postgres" {
                postgres_text_cast_type(column, value)
                    .map(|type_name| format!("{placeholder}::text::{type_name}"))
                    .unwrap_or(placeholder)
            } else {
                placeholder
            };
            params.push(value.clone());
            format!("{name} = {typed}")
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

pub fn build_keyed_update(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    edits: &[(usize, Value)],
    pk_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let pk_indexes = checked_pk_indexes(columns, pk_values)?;
    if edits.is_empty() {
        return Err(BuildSqlError::NothingToUpdate);
    }
    if edits.iter().any(|(col_idx, _)| *col_idx >= columns.len()) {
        return Err(BuildSqlError::StaleColumns);
    }
    let mut params: Vec<Value> = Vec::with_capacity(edits.len() + pk_values.len());
    let set_clauses: Vec<String> = edits
        .iter()
        .map(|(col_idx, new_value)| {
            let mysql_empty_enum_or_set =
                driver_id == "mysql" && matches!(new_value, Value::Text(value) if value.is_empty()) && {
                    let data_type = columns[*col_idx].data_type.trim().to_ascii_lowercase();
                    data_type.starts_with("enum(") || data_type.starts_with("set(")
                };
            if mysql_empty_enum_or_set {
                return format!("{} = SPACE(0)", quote_ident(driver_id, &columns[*col_idx].name));
            }
            let placeholder = placeholder_for(driver_id, params.len());
            let value_sql = if driver_id == "postgres" {
                postgres_text_cast_type(&columns[*col_idx], new_value)
                    .map(|type_name| format!("{placeholder}::text::{type_name}"))
                    .unwrap_or(placeholder)
            } else {
                placeholder
            };
            let clause = format!("{} = {}", quote_ident(driver_id, &columns[*col_idx].name), value_sql);
            params.push(new_value.clone());
            clause
        })
        .collect();
    let where_clause = keyed_where_clause(driver_id, columns, &pk_indexes, pk_values, &mut params);
    let qualified = qualified_table(driver_id, schema, table);
    let sql = build_update(driver_id, &qualified, &set_clauses.join(", "), &where_clause);
    Ok((sql, params))
}

pub fn build_optimistic_keyed_update(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    edits: &[(usize, Value, Value)],
    pk_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let pk_indexes = checked_pk_indexes(columns, pk_values)?;
    if edits.is_empty() {
        return Err(BuildSqlError::NothingToUpdate);
    }
    if edits.iter().any(|(col_idx, _, _)| *col_idx >= columns.len()) {
        return Err(BuildSqlError::StaleColumns);
    }
    let mut assignments = Vec::with_capacity(edits.len());
    let mut params = Vec::with_capacity(edits.len() * 3 + pk_values.len());
    for (col_idx, _, new_value) in edits {
        let name = quote_ident(driver_id, &columns[*col_idx].name);
        let placeholder = placeholder_for(driver_id, params.len());
        let value_sql = if driver_id == "postgres" {
            postgres_text_cast_type(&columns[*col_idx], new_value)
                .map(|type_name| format!("{placeholder}::text::{type_name}"))
                .unwrap_or(placeholder)
        } else {
            placeholder
        };
        assignments.push(format!("{name} = {value_sql}"));
        params.push(new_value.clone());
    }
    let mut predicates = vec![keyed_where_clause(
        driver_id,
        columns,
        &pk_indexes,
        pk_values,
        &mut params,
    )];
    for (&index, original) in pk_indexes.iter().zip(pk_values) {
        let column = &columns[index];
        let name = quote_ident(driver_id, &column.name);
        if let Some(predicate) = exact_original_predicate(driver_id, &name, column, original, &mut params) {
            predicates.push(predicate);
        }
    }
    for (col_idx, old_value, _) in edits {
        if columns[*col_idx].primary_key {
            continue;
        }
        let name = quote_ident(driver_id, &columns[*col_idx].name);
        predicates.push(optimistic_predicate(
            driver_id,
            &name,
            &columns[*col_idx],
            old_value,
            &mut params,
        )?);
    }
    let qualified = qualified_table(driver_id, schema, table);
    Ok((
        build_update(
            driver_id,
            &qualified,
            &assignments.join(", "),
            &predicates.join(" AND "),
        ),
        params,
    ))
}

fn is_geometric(data_type: &str) -> bool {
    matches!(
        data_type.trim().to_ascii_lowercase().as_str(),
        "point" | "line" | "lseg" | "box" | "path" | "polygon" | "circle"
    )
}

fn optimistic_predicate(
    driver_id: &str,
    name: &str,
    column: &crate::ColumnInfo,
    old_value: &Value,
    params: &mut Vec<Value>,
) -> Result<String, BuildSqlError> {
    if let Some(predicate) = exact_original_predicate(driver_id, name, column, old_value, params) {
        return Ok(predicate);
    }
    let placeholder = placeholder_for(driver_id, params.len());
    let predicate = match driver_id {
        "postgres" if matches!(column.data_type.trim().to_ascii_lowercase().as_str(), "real" | "float4") => {
            format!("{name} IS NOT DISTINCT FROM {placeholder}::real")
        }
        "postgres" if column.data_type.trim().eq_ignore_ascii_case("json") => {
            format!("{name}::jsonb IS NOT DISTINCT FROM {placeholder}::jsonb")
        }
        "postgres" if column.data_type.trim().eq_ignore_ascii_case("xml") => {
            format!("{name}::text IS NOT DISTINCT FROM {placeholder}::text")
        }
        "postgres" if column.data_type.trim().eq_ignore_ascii_case("json[]") => {
            format!("{name}::text IS NOT DISTINCT FROM {placeholder}::text::pg_catalog.json[]::text")
        }
        "postgres" if is_geometric(&column.data_type) => {
            let kind = column.data_type.trim().to_ascii_lowercase();
            format!("{name}::text IS NOT DISTINCT FROM {placeholder}::text::pg_catalog.{kind}::text")
        }
        "postgres" => {
            let value_sql = text_cast_placeholder(column, old_value, placeholder);
            format!("{name} IS NOT DISTINCT FROM {value_sql}")
        }
        "sqlite" | "duckdb" => format!("{name} IS NOT DISTINCT FROM {placeholder}"),
        "mysql" => format!("{name} <=> {placeholder}"),
        "mssql" => {
            let next = placeholder_for(driver_id, params.len() + 1);
            params.push(old_value.clone());
            params.push(old_value.clone());
            return Ok(format!(
                "({name} = {placeholder} OR ({name} IS NULL AND {next} IS NULL))"
            ));
        }
        _ => return Err(BuildSqlError::UnsupportedDriver(driver_id.into())),
    };
    params.push(old_value.clone());
    Ok(predicate)
}

/// Build a MongoDB grid update with compare-and-set predicates for every
/// edited field. MongoDB's update adapter translates equality against BSON
/// NULL to an explicit-null type check, keeping missing fields distinct.
pub fn build_mongodb_keyed_update(
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    edits: &[(usize, Value, Value)],
    pk_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let driver_id = "mongodb";
    let pk_indexes = checked_pk_indexes(columns, pk_values)?;
    if edits.is_empty() {
        return Err(BuildSqlError::NothingToUpdate);
    }
    if edits.iter().any(|(col_idx, _, _)| *col_idx >= columns.len()) {
        return Err(BuildSqlError::StaleColumns);
    }

    let mut params = Vec::with_capacity(edits.len() * 2 + pk_values.len());
    let set_clauses = edits
        .iter()
        .map(|(col_idx, _, new_value)| {
            let placeholder = placeholder_for(driver_id, params.len());
            params.push(new_value.clone());
            format!("{} = {placeholder}", quote_ident(driver_id, &columns[*col_idx].name))
        })
        .collect::<Vec<_>>();
    let mut where_clauses = vec![keyed_where_clause(
        driver_id,
        columns,
        &pk_indexes,
        pk_values,
        &mut params,
    )];
    for (col_idx, original_value, _) in edits {
        if columns[*col_idx].primary_key {
            continue;
        }
        let placeholder = placeholder_for(driver_id, params.len());
        where_clauses.push(format!(
            "{} = {placeholder}",
            quote_ident(driver_id, &columns[*col_idx].name)
        ));
        params.push(original_value.clone());
    }
    let qualified = qualified_table(driver_id, schema, table);
    let sql = build_update(
        driver_id,
        &qualified,
        &set_clauses.join(", "),
        &where_clauses.join(" AND "),
    );
    Ok((sql, params))
}

pub fn build_mongodb_keyed_delete(
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    original_values: &[Value],
    pk_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let driver_id = "mongodb";
    if original_values.len() != columns.len() {
        return Err(BuildSqlError::LengthMismatch {
            expected: columns.len(),
            got: original_values.len(),
        });
    }
    let pk_indexes = checked_pk_indexes(columns, pk_values)?;
    let mut params = Vec::with_capacity(pk_values.len() + columns.len() - pk_indexes.len());
    let mut where_clauses = vec![keyed_where_clause(
        driver_id,
        columns,
        &pk_indexes,
        pk_values,
        &mut params,
    )];
    for (column_index, (column, original_value)) in columns.iter().zip(original_values).enumerate() {
        if pk_indexes.contains(&column_index) {
            continue;
        }
        if matches!(original_value, Value::Undecodable(kind) if kind != "missing BSON field") {
            return Err(BuildSqlError::UnrepresentableValue {
                column: column.name.clone(),
            });
        }
        let placeholder = placeholder_for(driver_id, params.len());
        where_clauses.push(format!("{} = {placeholder}", quote_ident(driver_id, &column.name)));
        params.push(original_value.clone());
    }
    let snapshot_fields = columns
        .iter()
        .zip(original_values)
        .filter(|(_, value)| !matches!(value, Value::Undecodable(kind) if kind == "missing BSON field"))
        .map(|(column, _)| serde_json::Value::String(column.name.clone()))
        .collect();
    where_clauses.push("tablepro_mongodb_exact_field_set(?)".into());
    params.push(Value::Json(Box::new(serde_json::Value::Array(snapshot_fields))));
    let qualified = qualified_table(driver_id, schema, table);
    Ok((
        format!("DELETE FROM {qualified} WHERE {}", where_clauses.join(" AND ")),
        params,
    ))
}

#[cfg(test)]
#[path = "updates_tests.rs"]
mod tests;
