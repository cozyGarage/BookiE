use std::path::Path;

use bookie_core::DriverCapabilities;
use bookie_core::import::{
    CsvImportOptions, ImportTarget, PlanError, build_insert_plan, detect_format, is_xlsx, read_csv, read_import_file,
    read_xlsx, suggest_mapping,
};

use crate::cell_save_plan::SaveTarget;
use crate::change_tracker::EditStatement;
use crate::edit_preview::preview_text;

pub const PREVIEWED_ROWS: usize = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportChoices {
    pub delimiter: Option<bookie_core::export::CsvDelimiter>,
    pub null_marker: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsvImportPlan {
    pub statements: Vec<EditStatement>,
    pub rows: usize,
    pub imported_columns: Vec<String>,
    pub ignored_fields: Vec<String>,
    pub filled_by_database: Vec<String>,
    pub truncated: bool,
    pub approvable: bool,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CsvImportError {
    #[error("{0}")]
    File(String),
    #[error("no column of this table matches a column name in the file's header")]
    NoMatchingColumns,
    #[error("{0}")]
    Rows(String),
    #[error("{0}")]
    Build(String),
}

pub fn plan_csv_import(
    target: SaveTarget<'_>,
    capabilities: DriverCapabilities,
    path: &Path,
    row_cap: usize,
    choices: &ImportChoices,
) -> Result<CsvImportPlan, CsvImportError> {
    let bytes = read_import_file(path).map_err(|error| CsvImportError::File(error.to_string()))?;
    let (sheet, options, truncated) = if is_xlsx(&bytes) {
        let sheet = read_xlsx(&bytes, row_cap).map_err(|error| CsvImportError::File(error.to_string()))?;
        let truncated = sheet.truncated;
        let options = CsvImportOptions {
            null_marker: choices.null_marker.clone(),
            ..Default::default()
        };
        (sheet, options, truncated)
    } else {
        let mut options = CsvImportOptions::from(detect_format(&bytes));
        if let Some(delimiter) = choices.delimiter {
            options.delimiter = delimiter;
        }
        options.null_marker = choices.null_marker.clone();
        let mut sheet = read_csv(&bytes, &options, Some(row_cap.saturating_add(1)))
            .map_err(|error| CsvImportError::File(error.to_string()))?;
        let truncated = sheet.rows.len() > row_cap;
        sheet.rows.truncate(row_cap);
        (sheet, options, truncated)
    };
    let mapping = suggest_mapping(&sheet.headers, target.columns);
    if mapping.iter().all(Option::is_none) {
        return Err(CsvImportError::NoMatchingColumns);
    }
    let import_target = ImportTarget {
        driver_id: target.driver_id,
        capabilities,
        schema: target.schema,
        table: target.table,
        columns: target.columns,
        mapping: &mapping,
    };
    let plan = build_insert_plan(&import_target, &sheet, &options).map_err(describe)?;
    let (ignored_fields, filled_by_database) = classify_fields(&sheet.headers, target.columns, &mapping, &plan.columns);
    let statements: Vec<EditStatement> = plan
        .rows
        .iter()
        .map(|row| (plan.statement.clone(), row.clone()))
        .collect();
    let joined = statements
        .iter()
        .map(|(sql, _)| sql.trim().chars().count() + 3)
        .sum::<usize>();
    let approvable = joined <= bookie_policy::MAX_APPROVAL_SQL_CHARS + 3;
    let shown = &statements[..statements.len().min(PREVIEWED_ROWS)];
    let mut preview = preview_text(target.driver_id, shown);
    if statements.len() > shown.len() {
        preview.push_str(&format!(
            "\n\n… and {} more rows, each bound to the same statement",
            statements.len() - shown.len()
        ));
    }
    Ok(CsvImportPlan {
        rows: statements.len(),
        imported_columns: plan.columns.iter().map(|column| column.name.clone()).collect(),
        statements,
        ignored_fields,
        filled_by_database,
        truncated,
        approvable,
        preview,
    })
}

fn classify_fields(
    headers: &[String],
    table_columns: &[bookie_core::ColumnInfo],
    mapping: &[Option<usize>],
    kept: &[bookie_core::ColumnInfo],
) -> (Vec<String>, Vec<String>) {
    let mut used = Vec::new();
    let mut filled = Vec::new();
    for (column, field) in table_columns.iter().zip(mapping) {
        let Some(field) = field else { continue };
        used.push(*field);
        if !kept.iter().any(|k| k.name == column.name) {
            filled.push(headers[*field].clone());
        }
    }
    let ignored = headers
        .iter()
        .enumerate()
        .filter(|(index, _)| !used.contains(index))
        .map(|(_, header)| header.clone())
        .collect();
    (ignored, filled)
}

fn describe(error: PlanError) -> CsvImportError {
    match error {
        PlanError::Rows { total, first } => {
            let lines: Vec<String> = first
                .iter()
                .take(3)
                .map(|row| format!("line {}, column {}: {}", row.line, row.column, row.reason))
                .collect();
            CsvImportError::Rows(format!("{total} row(s) do not fit the table. {}", lines.join("; ")))
        }
        PlanError::NoMappedColumns => CsvImportError::NoMatchingColumns,
        other => CsvImportError::Build(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::ColumnInfo;

    fn column(name: &str, data_type: &str, key: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            nullable: !key,
            primary_key: key,
            is_auto_increment: key,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    fn orders() -> Vec<ColumnInfo> {
        vec![
            column("id", "INTEGER", true),
            column("customer", "TEXT", false),
            column("total", "REAL", false),
        ]
    }

    fn write(directory: &tempfile::TempDir, text: &str) -> std::path::PathBuf {
        let path = directory.path().join("import.csv");
        std::fs::write(&path, text).unwrap();
        path
    }

    fn plan(columns: &[ColumnInfo], path: &Path, cap: usize) -> Result<CsvImportPlan, CsvImportError> {
        plan_for("sqlite", columns, path, cap)
    }

    fn plan_for(
        driver_id: &str,
        columns: &[ColumnInfo],
        path: &Path,
        cap: usize,
    ) -> Result<CsvImportPlan, CsvImportError> {
        let target = SaveTarget {
            driver_id,
            schema: None,
            table: "orders",
            columns,
        };
        plan_csv_import(
            target,
            DriverCapabilities::default(),
            path,
            cap,
            &ImportChoices::default(),
        )
    }

    #[test]
    fn a_chosen_separator_and_null_text_override_what_the_file_looks_like() {
        let directory = tempfile::tempdir().unwrap();
        let path = write(&directory, "customer|total\nada|NULL\nbob|2.5\n");
        let target = SaveTarget {
            driver_id: "sqlite",
            schema: None,
            table: "orders",
            columns: &orders(),
        };
        let choices = ImportChoices {
            delimiter: Some(bookie_core::export::CsvDelimiter::Pipe),
            null_marker: "NULL".into(),
        };
        let result = plan_csv_import(target, DriverCapabilities::default(), &path, 10, &choices).unwrap();
        assert_eq!(result.imported_columns, ["customer", "total"]);
        let (_, values) = &result.statements[0];
        assert_eq!(values[1], bookie_core::Value::Null, "the chosen NULL text becomes NULL");
    }

    #[test]
    fn an_excel_import_uses_worksheet_columns_even_when_a_csv_delimiter_is_selected() {
        use bookie_core::export::{CsvDelimiter, ResultExport, ResultFormat, write_result_file};
        use bookie_core::{QueryResult, Value};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("orders.xlsx");
        let result = QueryResult {
            columns: orders().into_iter().skip(1).collect(),
            rows: vec![
                vec![Value::Text("a,b|c".into()), Value::Float(2.5)],
                vec![Value::Text("next".into()), Value::Float(3.5)],
            ]
            .into(),
            truncated: false,
        };
        write_result_file(
            &path,
            &result,
            &ResultExport {
                format: ResultFormat::Xlsx,
                csv: &Default::default(),
                sql: None,
            },
            || false,
            |_| (),
        )
        .unwrap();
        let target = SaveTarget {
            driver_id: "sqlite",
            schema: None,
            table: "orders",
            columns: &orders(),
        };
        let choices = ImportChoices {
            delimiter: Some(CsvDelimiter::Pipe),
            ..Default::default()
        };
        let plan = plan_csv_import(target, DriverCapabilities::default(), &path, 1, &choices).unwrap();
        assert_eq!(plan.imported_columns, ["customer", "total"]);
        assert_eq!(plan.statements[0].1, [Value::Text("a,b|c".into()), Value::Float(2.5)]);
        assert_eq!(plan.rows, 1);
        assert!(plan.truncated);
    }

    #[test]
    fn a_file_whose_headers_match_becomes_one_insert_per_row_and_reports_what_it_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let path = write(&directory, "customer,total,notes\nada,1.5,x\nbob,2.5,y\ncy,3.5,z\n");
        let result = plan(&orders(), &path, 100).unwrap();

        assert_eq!(result.rows, 3);
        assert_eq!(result.imported_columns, ["customer", "total"]);
        assert_eq!(
            result.ignored_fields,
            ["notes"],
            "a field with no matching column is reported, not guessed"
        );
        assert!(!result.truncated);
        let (sql, values) = &result.statements[0];
        assert!(sql.starts_with("INSERT INTO"), "{sql}");
        assert_eq!(values.len(), 2);
        assert!(result.preview.contains("and 1 more rows"), "{}", result.preview);
    }

    #[test]
    fn a_file_longer_than_the_cap_is_cut_and_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let path = write(&directory, "customer,total\na,1\nb,2\nc,3\nd,4\ne,5\n");
        let result = plan(&orders(), &path, 2).unwrap();
        assert_eq!(result.rows, 2);
        assert!(result.truncated);
    }

    #[test]
    fn headers_that_match_no_column_and_unreadable_values_and_missing_files_are_refused_clearly() {
        let directory = tempfile::tempdir().unwrap();
        let none = write(&directory, "alpha,beta\n1,2\n");
        assert_eq!(plan(&orders(), &none, 10), Err(CsvImportError::NoMatchingColumns));

        let bad = write(&directory, "customer,qty\nada,not a number\n");
        let counted = vec![
            column("id", "INTEGER", true),
            column("customer", "TEXT", false),
            column("qty", "INTEGER", false),
        ];
        match plan_for("postgres", &counted, &bad, 10) {
            Err(CsvImportError::Rows(text)) => assert!(text.contains("qty"), "{text}"),
            other => panic!("expected a row error, got {other:?}"),
        }

        let missing = directory.path().join("absent.csv");
        assert!(matches!(plan(&orders(), &missing, 10), Err(CsvImportError::File(_))));
    }

    #[test]
    fn an_import_too_long_for_one_approval_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let mut text = String::from("customer,total\n");
        for row in 0..600 {
            text.push_str(&format!("c{row},1\n"));
        }
        let path = write(&directory, &text);
        assert!(!plan(&orders(), &path, 1_000).unwrap().approvable);
        assert!(plan(&orders(), &path, 5).unwrap().approvable);
    }
}
