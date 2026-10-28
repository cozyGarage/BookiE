use thiserror::Error;

use crate::driver::DriverCapabilities;
use crate::import::cell::{CsvRowError, row_to_values_for_driver};
use crate::import::csv_import::{CsvImportOptions, CsvSheet};
use crate::query::{ColumnInfo, Value};
use crate::sql_dialect::{
    BuildSqlError, IdentError, build_insert_from_draft_touched, placeholder_for, postgres_array_cast_type,
    postgres_native_text_cast_type, postgres_numeric_cast_type, postgres_temporal_cast_type, quote_ident,
    validate_ident,
};

/// Rows committed per transaction. Small enough that a failure loses
/// little, large enough that the round trips do not dominate.
pub const DEFAULT_IMPORT_BATCH_ROWS: usize = 500;

/// Row failures the user is shown. The rest are counted, not listed.
pub const MAX_REPORTED_ROW_ERRORS: usize = 20;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum PlanError {
    #[error("no column is mapped to a field in the file")]
    NoMappedColumns,

    #[error("the file has no rows to import")]
    NoRows,

    #[error("{0}")]
    Ident(#[from] IdentError),

    #[error("the insert could not be built: {0}")]
    Sql(String),

    #[error("{total} rows could not be read into the table")]
    Rows { total: usize, first: Vec<CsvRowError> },
}

/// Where the rows are going, and which field feeds each column.
pub struct ImportTarget<'a> {
    pub driver_id: &'a str,
    pub capabilities: DriverCapabilities,
    pub schema: Option<&'a str>,
    pub table: &'a str,
    pub columns: &'a [ColumnInfo],
    pub mapping: &'a [Option<usize>],
}

/// One INSERT shape and the bound values for every row, in that shape's
/// column order. Holding one statement for the whole file is what lets a
/// single approval cover it.
#[derive(Debug, Clone, PartialEq)]
pub struct InsertPlan {
    pub statement: String,
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<Value>>,
}

impl InsertPlan {
    pub fn row_count(&self) -> u64 {
        self.rows.len() as u64
    }

    pub fn batches(&self, size: usize) -> impl Iterator<Item = &[Vec<Value>]> {
        self.rows.chunks(size.max(1))
    }
}

/// Build the one statement and every row's values, or refuse with the
/// rows that could not be read. Nothing is executed here.
pub fn build_insert_plan(
    target: &ImportTarget<'_>,
    sheet: &CsvSheet,
    options: &CsvImportOptions,
) -> Result<InsertPlan, PlanError> {
    validate_ident(target.table)?;
    if let Some(schema) = target.schema {
        validate_ident(schema)?;
    }
    if sheet.rows.is_empty() {
        return Err(PlanError::NoRows);
    }
    let (columns, mapping) = insert_columns(target)?;
    let mut rows = bind_rows(sheet, &columns, &mapping, options, target.driver_id)?;
    let mysql_empty_string_columns = if target.driver_id.eq_ignore_ascii_case("mysql") {
        columns
            .iter()
            .enumerate()
            .filter(|(index, column)| {
                mysql_empty_string_type(&column.data_type)
                    && rows
                        .iter()
                        .any(|row| matches!(row.get(*index), Some(Value::Text(text)) if text.is_empty()))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let statement = insert_statement(target, &columns, &mysql_empty_string_columns)?;
    if !mysql_empty_string_columns.is_empty() {
        for row in &mut rows {
            let values = std::mem::take(row);
            let mut params = Vec::with_capacity(values.len() + mysql_empty_string_columns.len());
            for (index, value) in values.into_iter().enumerate() {
                if mysql_empty_string_columns.contains(&index) {
                    let is_empty = matches!(&value, Value::Text(text) if text.is_empty());
                    params.push(Value::Int(i64::from(is_empty)));
                    params.push(if is_empty { Value::Null } else { value });
                } else {
                    params.push(value);
                }
            }
            *row = params;
        }
    }
    Ok(InsertPlan {
        statement,
        columns,
        rows,
    })
}

/// The columns the statement writes: mapped, and not ones the engine
/// fills itself. An unmapped column is left out of the statement
/// entirely, so its own default applies.
fn insert_columns(target: &ImportTarget<'_>) -> Result<(Vec<ColumnInfo>, Vec<Option<usize>>), PlanError> {
    let mut columns = Vec::new();
    let mut mapping = Vec::new();
    for (column, field) in target.columns.iter().zip(target.mapping) {
        let Some(field) = field else {
            continue;
        };
        if column.is_generated
            || (column.is_auto_increment && !target.capabilities.supports_explicit_auto_increment_values)
        {
            continue;
        }
        validate_ident(&column.name)?;
        columns.push(column.clone());
        mapping.push(Some(*field));
    }
    if columns.is_empty() {
        return Err(PlanError::NoMappedColumns);
    }
    Ok((columns, mapping))
}

/// One shape for every row. The statement is built from placeholder
/// values that are never NULL, so no column is dropped from it for one
/// row and kept for the next.
fn insert_statement(
    target: &ImportTarget<'_>,
    columns: &[ColumnInfo],
    mysql_empty_string_columns: &[usize],
) -> Result<String, PlanError> {
    if !mysql_empty_string_columns.is_empty() {
        let mut parameter_index = 0;
        let values = columns
            .iter()
            .enumerate()
            .map(|(column_index, _)| {
                if mysql_empty_string_columns.contains(&column_index) {
                    let empty = placeholder_for(target.driver_id, parameter_index);
                    let value = placeholder_for(target.driver_id, parameter_index + 1);
                    parameter_index += 2;
                    format!("CASE WHEN {empty} = 1 THEN SPACE(0) ELSE {value} END")
                } else {
                    let value = placeholder_for(target.driver_id, parameter_index);
                    parameter_index += 1;
                    value
                }
            })
            .collect::<Vec<_>>();
        let identifiers = columns
            .iter()
            .map(|column| quote_ident(target.driver_id, &column.name))
            .collect::<Vec<_>>();
        let table = match target.schema {
            Some(schema) => format!(
                "{}.{}",
                quote_ident(target.driver_id, schema),
                quote_ident(target.driver_id, target.table)
            ),
            None => quote_ident(target.driver_id, target.table),
        };
        return Ok(format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            identifiers.join(", "),
            values.join(", ")
        ));
    }
    let shape_columns: Vec<ColumnInfo> = columns.to_vec();
    // PostgreSQL temporal CSV values use text parameters with an explicit
    // server cast. This supports exact sentinels such as +/-infinity while
    // keeping the parameter type consistent for every row in the import.
    let shape_values = shape_columns
        .iter()
        .map(|column| {
            if column.enum_type.is_some()
                || postgres_array_cast_type(&column.data_type).is_some()
                || postgres_numeric_cast_type(&column.data_type).is_some()
                || postgres_temporal_cast_type(&column.data_type).is_some()
                || postgres_native_text_cast_type(&column.data_type).is_some()
            {
                Value::Text(String::new())
            } else {
                Value::Int(0)
            }
        })
        .collect::<Vec<_>>();
    let touched = vec![true; shape_columns.len()];
    let (mut statement, _) = build_insert_from_draft_touched(
        target.driver_id,
        target.schema,
        target.table,
        &shape_columns,
        &shape_values,
        &touched,
    )
    .map_err(|error: BuildSqlError| PlanError::Sql(error.to_string()))?;
    if target.driver_id.eq_ignore_ascii_case("postgres") && columns.iter().any(|column| column.is_auto_increment) {
        statement = statement.replacen(" VALUES", " OVERRIDING SYSTEM VALUE VALUES", 1);
    }
    Ok(statement)
}

fn mysql_empty_string_type(data_type: &str) -> bool {
    [
        "enum(",
        "set(",
        "char",
        "varchar",
        "tinytext",
        "text",
        "mediumtext",
        "longtext",
    ]
    .iter()
    .any(|prefix| {
        data_type
            .trim()
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    })
}

fn bind_rows(
    sheet: &CsvSheet,
    columns: &[ColumnInfo],
    mapping: &[Option<usize>],
    options: &CsvImportOptions,
    driver_id: &str,
) -> Result<Vec<Vec<Value>>, PlanError> {
    let header_offset = usize::from(options.has_header);
    let mut rows = Vec::with_capacity(sheet.rows.len());
    let mut failures = Vec::new();
    let mut total_failures = 0;
    for (index, row) in sheet.rows.iter().enumerate() {
        match row_to_values_for_driver(row, mapping, columns, options, index + header_offset + 1, driver_id) {
            Ok(mut values) => {
                if driver_id == "postgres" {
                    for (value, column) in values.iter_mut().zip(columns) {
                        if postgres_temporal_cast_type(&column.data_type).is_some() {
                            match value {
                                Value::Date(date) => *value = Value::Text(date.to_string()),
                                Value::Time(time) => *value = Value::Text(time.to_string()),
                                Value::DateTime(datetime) => *value = Value::Text(datetime.to_string()),
                                Value::TimestampTz(datetime) => *value = Value::Text(datetime.to_rfc3339()),
                                _ => {}
                            }
                        }
                    }
                }
                rows.push(values);
            }
            Err(error) => {
                total_failures += 1;
                if failures.len() < MAX_REPORTED_ROW_ERRORS {
                    failures.push(error);
                }
            }
        }
    }
    if total_failures > 0 {
        return Err(PlanError::Rows {
            total: total_failures,
            first: failures,
        });
    }
    Ok(rows)
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
