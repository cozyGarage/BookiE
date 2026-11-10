use bookie_core::export::{CsvOptions, render_csv, render_in_clause, render_json, render_markdown};
use bookie_core::sql_literal::build_insert_literal;
use bookie_core::{ColumnInfo, Value};

use crate::model::{Cell, ColumnMeta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyFormat {
    Csv,
    Json,
    Markdown,
    InList,
    Insert,
}

pub struct CopyContext<'a> {
    pub columns: &'a [ColumnMeta],
    pub driver_id: &'a str,
    pub table: &'a str,
    pub column: usize,
}

pub struct Copied {
    pub text: String,
    pub note: Option<String>,
}

pub fn render(format: CopyFormat, context: &CopyContext<'_>, rows: &[Vec<Cell>]) -> Result<Copied, String> {
    let infos = infos(context.columns);
    let values: Vec<Vec<Value>> = rows.iter().map(|row| row.to_vec()).collect();
    let plain = |text: String| Copied { text, note: None };
    match format {
        CopyFormat::Csv => Ok(plain(render_csv(&infos, &values, &CsvOptions::default()))),
        CopyFormat::Json => Ok(plain(render_json(&infos, &values))),
        CopyFormat::Markdown => Ok(plain(render_markdown(&infos, &values))),
        CopyFormat::InList => {
            let list = render_in_clause(context.driver_id, &values, context.column);
            if list.sql.is_empty() {
                return Err("no value in this column can be written as a SQL literal".into());
            }
            let note = (list.skipped > 0).then(|| format!("{} values left out", list.skipped));
            Ok(Copied { text: list.sql, note })
        }
        CopyFormat::Insert => {
            let (schema, table) = match context.table.split_once('.') {
                Some((schema, table)) => (Some(schema), table),
                None => (None, context.table),
            };
            let statements: Result<Vec<String>, String> = values
                .iter()
                .map(|row| {
                    build_insert_literal(context.driver_id, schema, table, &infos, row)
                        .map_err(|error| format!("{error:?}"))
                })
                .collect();
            Ok(plain(statements?.join("\n")))
        }
    }
}

fn infos(columns: &[ColumnMeta]) -> Vec<ColumnInfo> {
    columns
        .iter()
        .map(|column| ColumnInfo {
            name: column.name.clone(),
            data_type: String::new(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns() -> Vec<ColumnMeta> {
        ["id", "name"]
            .into_iter()
            .map(|name| ColumnMeta {
                name: name.into(),
                width: 100.0,
                numeric: false,
            })
            .collect()
    }

    fn rows() -> Vec<Vec<Cell>> {
        vec![
            vec![Cell::Int(1), Cell::Text("O'Brien, Pat".into())],
            vec![Cell::Int(2), Cell::Null],
        ]
    }

    fn context(columns: &[ColumnMeta]) -> CopyContext<'_> {
        CopyContext {
            columns,
            driver_id: "postgres",
            table: "public.people",
            column: 1,
        }
    }

    #[test]
    fn csv_quotes_a_comma_and_json_keeps_null() {
        let columns = columns();
        let csv = render(CopyFormat::Csv, &context(&columns), &rows()).unwrap().text;
        assert!(csv.contains("1,\"O'Brien, Pat\""), "{csv}");
        let json = render(CopyFormat::Json, &context(&columns), &rows()).unwrap().text;
        assert!(
            json.contains("\"name\": null") || json.contains("\"name\":null"),
            "{json}"
        );
    }

    #[test]
    fn markdown_has_a_header_row() {
        let columns = columns();
        let markdown = render(CopyFormat::Markdown, &context(&columns), &rows()).unwrap().text;
        assert!(markdown.starts_with("| id | name |"), "{markdown}");
    }

    #[test]
    fn insert_statements_quote_names_and_double_a_quote_in_text() {
        let columns = columns();
        let copied = render(CopyFormat::Insert, &context(&columns), &rows()).unwrap().text;
        let lines: Vec<&str> = copied.lines().collect();
        assert_eq!(
            lines[0],
            "INSERT INTO \"public\".\"people\" (\"id\", \"name\") VALUES (1, 'O''Brien, Pat');"
        );
        assert_eq!(
            lines[1],
            "INSERT INTO \"public\".\"people\" (\"id\", \"name\") VALUES (2, NULL);"
        );
    }

    #[test]
    fn an_in_list_uses_the_chosen_column_and_says_what_it_left_out() {
        let columns = columns();
        let copied = render(CopyFormat::InList, &context(&columns), &rows()).unwrap();
        assert_eq!(copied.text, "('O''Brien, Pat')");
        assert_eq!(copied.note.as_deref(), Some("1 values left out"));
        let none = vec![vec![Cell::Int(1), Cell::Null]];
        assert!(render(CopyFormat::InList, &context(&columns), &none).is_err());
    }
}
