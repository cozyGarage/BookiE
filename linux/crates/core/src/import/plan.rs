use thiserror::Error;

use crate::import::cell::{CsvRowError, row_to_values};
use crate::import::csv_import::{CsvImportOptions, CsvSheet};
use crate::query::{ColumnInfo, Value};
use crate::sql_dialect::{BuildSqlError, IdentError, build_insert_from_draft, validate_ident};

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
    let statement = insert_statement(target, &columns)?;
    let rows = bind_rows(sheet, &columns, &mapping, options)?;
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
        if column.is_auto_increment || column.is_generated {
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
fn insert_statement(target: &ImportTarget<'_>, columns: &[ColumnInfo]) -> Result<String, PlanError> {
    let shape_columns: Vec<ColumnInfo> = columns
        .iter()
        .map(|column| ColumnInfo {
            default_value: None,
            ..column.clone()
        })
        .collect();
    let shape_values = vec![Value::Int(0); shape_columns.len()];
    let (statement, _) = build_insert_from_draft(
        target.driver_id,
        target.schema,
        target.table,
        &shape_columns,
        &shape_values,
    )
    .map_err(|error: BuildSqlError| PlanError::Sql(error.to_string()))?;
    Ok(statement)
}

fn bind_rows(
    sheet: &CsvSheet,
    columns: &[ColumnInfo],
    mapping: &[Option<usize>],
    options: &CsvImportOptions,
) -> Result<Vec<Vec<Value>>, PlanError> {
    let header_offset = usize::from(options.has_header);
    let mut rows = Vec::with_capacity(sheet.rows.len());
    let mut failures = Vec::new();
    let mut total_failures = 0;
    for (index, row) in sheet.rows.iter().enumerate() {
        match row_to_values(row, mapping, columns, options, index + header_offset + 1) {
            Ok(values) => rows.push(values),
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
mod tests {
    use super::*;

    fn column(name: &str, data_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.to_owned(),
            data_type: data_type.to_owned(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        }
    }

    fn sheet(rows: &[&[&str]], headers: &[&str]) -> CsvSheet {
        CsvSheet {
            headers: headers.iter().map(|header| (*header).to_owned()).collect(),
            rows: rows
                .iter()
                .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
                .collect(),
            truncated: false,
        }
    }

    fn target<'a>(columns: &'a [ColumnInfo], mapping: &'a [Option<usize>]) -> ImportTarget<'a> {
        ImportTarget {
            driver_id: "postgres",
            schema: None,
            table: "people",
            columns,
            mapping,
        }
    }

    #[test]
    fn every_row_binds_to_the_same_statement() {
        let columns = vec![column("id", "bigint"), column("name", "text")];
        let mapping = vec![Some(0), Some(1)];
        let sheet = sheet(&[&["1", "ada"], &["2", "grace"]], &["id", "name"]);

        let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

        assert_eq!(
            plan.statement,
            "INSERT INTO \"people\" (\"id\", \"name\") VALUES ($1, $2)"
        );
        assert_eq!(plan.rows.len(), 2);
        assert_eq!(plan.rows[0], vec![Value::Int(1), Value::Text("ada".to_owned())]);
    }

    /// The statement builder drops a column whose value is NULL when the
    /// column has a server default. One approved import must send one
    /// shape, so the plan builds the statement from values that are never
    /// NULL and keeps every mapped column in it.
    #[test]
    fn a_mapped_column_with_a_server_default_stays_in_the_statement_for_every_row() {
        let mut columns = vec![column("id", "bigint"), column("created", "timestamp")];
        columns[1].default_value = Some("now()".into());
        let mapping = vec![Some(0), Some(1)];
        let sheet = sheet(&[&["1", ""], &["2", "2024-05-06 00:00:00"]], &["id", "created"]);

        let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

        assert!(plan.statement.contains("\"created\""));
        assert_eq!(plan.rows[0][1], Value::Null);
    }

    #[test]
    fn an_unmapped_column_is_left_out_so_its_own_default_applies() {
        let columns = vec![column("id", "bigint"), column("created", "timestamp")];
        let mapping = vec![Some(0), None];
        let sheet = sheet(&[&["1"]], &["id"]);

        let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

        assert!(!plan.statement.contains("created"));
        assert_eq!(plan.columns.len(), 1);
    }

    #[test]
    fn a_column_the_engine_fills_itself_is_never_written() {
        let mut columns = vec![column("id", "bigint"), column("name", "text")];
        columns[0].is_auto_increment = true;
        let mapping = vec![Some(0), Some(1)];
        let sheet = sheet(&[&["1", "ada"]], &["id", "name"]);

        let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

        assert_eq!(plan.statement, "INSERT INTO \"people\" (\"name\") VALUES ($1)");
    }

    #[test]
    fn an_identifier_that_carries_a_quote_is_escaped_rather_than_closed() {
        let columns = vec![column("na\"me", "text")];
        let mapping = vec![Some(0)];
        let mut target = target(&columns, &mapping);
        target.table = "peo\"ple\"; DROP TABLE people; --";
        let rows = sheet(&[&["ada"]], &["name"]);

        let plan = build_insert_plan(&target, &rows, &CsvImportOptions::default()).expect("plan");

        assert_eq!(
            plan.statement,
            "INSERT INTO \"peo\"\"ple\"\"; DROP TABLE people; --\" (\"na\"\"me\") VALUES ($1)"
        );
    }

    #[test]
    fn an_identifier_that_is_empty_or_carries_a_control_character_is_refused() {
        let columns = vec![column("id", "bigint")];
        let mapping = vec![Some(0)];
        let rows = sheet(&[&["1"]], &["id"]);
        let mut empty = target(&columns, &mapping);
        empty.table = "   ";
        assert_eq!(
            build_insert_plan(&empty, &rows, &CsvImportOptions::default()),
            Err(PlanError::Ident(IdentError::Empty))
        );

        let broken = vec![column("na\nme", "text")];
        assert_eq!(
            build_insert_plan(&target(&broken, &mapping), &rows, &CsvImportOptions::default()),
            Err(PlanError::Ident(IdentError::ControlCharacter))
        );
    }

    #[test]
    fn rows_that_cannot_be_read_stop_the_plan_and_are_reported_without_their_contents() {
        let columns = vec![column("id", "bigint")];
        let mapping = vec![Some(0)];
        let sheet = sheet(&[&["1"], &["not-a-number"], &["also-bad"]], &["id"]);

        let error =
            build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect_err("refused");

        let PlanError::Rows { total, first } = error else {
            panic!("expected row failures");
        };
        assert_eq!(total, 2);
        assert_eq!(first[0].line, 3);
        assert!(!first.iter().any(|error| error.to_string().contains("not-a-number")));
    }

    #[test]
    fn only_the_first_failures_are_listed_and_the_rest_are_counted() {
        let columns = vec![column("id", "bigint")];
        let mapping = vec![Some(0)];
        let rows: Vec<Vec<String>> = (0..MAX_REPORTED_ROW_ERRORS + 5)
            .map(|_| vec!["bad".to_owned()])
            .collect();
        let sheet = CsvSheet {
            headers: vec!["id".to_owned()],
            rows,
            truncated: false,
        };

        let error =
            build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect_err("refused");

        let PlanError::Rows { total, first } = error else {
            panic!("expected row failures");
        };
        assert_eq!(total, MAX_REPORTED_ROW_ERRORS + 5);
        assert_eq!(first.len(), MAX_REPORTED_ROW_ERRORS);
    }

    #[test]
    fn a_file_with_nothing_mapped_or_nothing_in_it_is_refused() {
        let columns = vec![column("id", "bigint")];
        let filled = sheet(&[&["1"]], &["id"]);
        assert_eq!(
            build_insert_plan(&target(&columns, &[None]), &filled, &CsvImportOptions::default()),
            Err(PlanError::NoMappedColumns)
        );
        assert_eq!(
            build_insert_plan(
                &target(&columns, &[Some(0)]),
                &sheet(&[], &["id"]),
                &CsvImportOptions::default()
            ),
            Err(PlanError::NoRows)
        );
    }

    #[test]
    fn the_batches_a_plan_offers_are_bounded_and_cover_every_row() {
        let columns = vec![column("id", "bigint")];
        let mapping = vec![Some(0)];
        let rows: Vec<Vec<String>> = (0..7).map(|index| vec![index.to_string()]).collect();
        let sheet = CsvSheet {
            headers: vec!["id".to_owned()],
            rows,
            truncated: false,
        };
        let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

        let batches: Vec<usize> = plan.batches(3).map(<[Vec<Value>]>::len).collect();

        assert_eq!(batches, vec![3, 3, 1]);
        assert_eq!(batches.iter().sum::<usize>(), 7);
    }
}
