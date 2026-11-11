use bookie_core::export::{CsvOptions, render_csv, render_in_clause, render_json, render_markdown, render_tsv};
use bookie_core::sql_literal::build_insert_literal;
use bookie_core::{ColumnInfo, QueryRows, TableInfo, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyFormat {
    Csv,
    Json,
    Markdown,
    InList,
    Insert,
}

pub struct CopyContext<'a> {
    pub columns: &'a [ColumnInfo],
    pub driver_id: &'a str,
    pub target: Option<TableInfo>,
    pub column: usize,
}

pub struct Copied {
    pub text: String,
    pub note: Option<String>,
}

pub fn render(format: CopyFormat, context: &CopyContext<'_>, rows: &[Vec<Value>]) -> Result<Copied, String> {
    let infos = context.columns;
    let values = QueryRows::from_rows(rows.to_vec());
    let plain = |text: String| Copied { text, note: None };
    match format {
        CopyFormat::Csv => Ok(plain(render_csv(infos, &values, &CsvOptions::default()))),
        CopyFormat::Json => Ok(plain(render_json(infos, &values))),
        CopyFormat::Markdown => Ok(plain(render_markdown(infos, &values))),
        CopyFormat::InList => {
            let list = render_in_clause(context.driver_id, &values, context.column);
            if list.sql.is_empty() {
                return Err("no value in this column can be written as a SQL literal".into());
            }
            let note = (list.skipped > 0).then(|| format!("{} values left out", list.skipped));
            Ok(Copied { text: list.sql, note })
        }
        CopyFormat::Insert => {
            let target = context.target.as_ref().ok_or("no table target is available")?;
            let statements: Result<Vec<String>, String> = values
                .iter()
                .map(|row| {
                    build_insert_literal(context.driver_id, target.schema.as_deref(), &target.name, infos, row)
                        .map_err(|error| format!("{error:?}"))
                })
                .collect();
            Ok(plain(statements?.join("\n")))
        }
    }
}

pub fn tsv(columns: &[ColumnInfo], rows: Vec<Vec<Value>>, with_headers: bool) -> String {
    render_tsv(columns, &QueryRows::from_rows(rows), with_headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns() -> Vec<ColumnInfo> {
        ["id", "name"].into_iter().map(info).collect()
    }

    fn info(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
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
        }
    }

    fn rows() -> Vec<Vec<Value>> {
        vec![
            vec![Value::Int(1), Value::Text("O'Brien, Pat".into())],
            vec![Value::Int(2), Value::Null],
        ]
    }

    fn context(columns: &[ColumnInfo]) -> CopyContext<'_> {
        CopyContext {
            columns,
            driver_id: "postgres",
            target: Some(TableInfo {
                schema: Some("public".into()),
                name: "people".into(),
            }),
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
    fn insert_target_components_keep_dots_inside_each_identifier() {
        let columns = columns();
        let context = CopyContext {
            columns: &columns,
            driver_id: "postgres",
            target: Some(TableInfo {
                schema: Some("schema.with.dot\"quote".into()),
                name: "table.name\"with quote".into(),
            }),
            column: 0,
        };
        let copied = render(CopyFormat::Insert, &context, &rows()).unwrap().text;
        assert!(
            copied.starts_with(
                "INSERT INTO \"schema.with.dot\"\"quote\".\"table.name\"\"with quote\" (\"id\", \"name\") VALUES (1,"
            ),
            "{copied}"
        );
    }

    #[test]
    fn duplicate_names_and_empty_names_keep_their_exact_target_identity() {
        let columns = columns();
        let first = CopyContext {
            columns: &columns,
            driver_id: "postgres",
            target: Some(TableInfo {
                schema: Some("sales".into()),
                name: "orders".into(),
            }),
            column: 0,
        };
        let second = CopyContext {
            columns: &columns,
            driver_id: "postgres",
            target: Some(TableInfo {
                schema: Some("archive".into()),
                name: "orders".into(),
            }),
            column: 0,
        };
        let empty = CopyContext {
            columns: &columns,
            driver_id: "postgres",
            target: Some(TableInfo {
                schema: Some("sales".into()),
                name: String::new(),
            }),
            column: 0,
        };
        assert!(
            render(CopyFormat::Insert, &first, &rows())
                .unwrap()
                .text
                .starts_with("INSERT INTO \"sales\".\"orders\"")
        );
        assert!(
            render(CopyFormat::Insert, &second, &rows())
                .unwrap()
                .text
                .starts_with("INSERT INTO \"archive\".\"orders\"")
        );
        assert!(
            render(CopyFormat::Insert, &empty, &rows())
                .unwrap()
                .text
                .starts_with("INSERT INTO \"sales\".\"\"")
        );
    }

    #[test]
    fn an_in_list_uses_the_chosen_column_and_says_what_it_left_out() {
        let columns = columns();
        let copied = render(CopyFormat::InList, &context(&columns), &rows()).unwrap();
        assert_eq!(copied.text, "('O''Brien, Pat')");
        assert_eq!(copied.note.as_deref(), Some("1 values left out"));
        let none = vec![vec![Value::Int(1), Value::Null]];
        assert!(render(CopyFormat::InList, &context(&columns), &none).is_err());
    }

    #[test]
    fn insert_without_a_structured_target_is_refused() {
        let columns = columns();
        let context = CopyContext {
            columns: &columns,
            driver_id: "postgres",
            target: None,
            column: 0,
        };
        assert!(render(CopyFormat::Insert, &context, &rows()).is_err());
    }
}
