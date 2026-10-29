use super::supports_sql_literals;
use crate::export::value_to_text;
use crate::query::{ColumnInfo, Value};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InClause {
    pub sql: String,
    pub skipped: usize,
}

pub fn render_in_clause(driver_id: &str, rows: &[Vec<Value>], column_index: usize) -> InClause {
    let values = rows.iter().filter_map(|row| row.get(column_index)).collect::<Vec<_>>();
    let literals = values
        .iter()
        .filter_map(|value| in_clause_literal(driver_id, value))
        .collect::<Vec<_>>();
    InClause {
        skipped: values.len() - literals.len(),
        sql: if literals.is_empty() {
            String::new()
        } else {
            format!("({})", literals.join(", "))
        },
    }
}

pub fn render_value_list(driver_id: &str, columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
    if rows.is_empty() || columns.is_empty() {
        return String::new();
    }
    let mut row_tuples = Vec::with_capacity(rows.len());
    for row in rows {
        let mut literals = Vec::with_capacity(columns.len());
        for (i, _) in columns.iter().enumerate() {
            let val = row.get(i).unwrap_or(&Value::Null);
            if matches!(val, Value::Null) {
                literals.push("NULL".to_string());
            } else if let Some(lit) = in_clause_literal(driver_id, val) {
                literals.push(lit);
            } else if let Some(txt) = value_to_text(val) {
                literals.push(format!("'{}'", txt.replace('\'', "''")));
            } else {
                literals.push("NULL".to_string());
            }
        }
        row_tuples.push(format!("({})", literals.join(", ")));
    }
    row_tuples.join(",\n")
}

fn in_clause_literal(driver_id: &str, value: &Value) -> Option<String> {
    if !supports_sql_literals(driver_id) || matches!(value, Value::Null | Value::Bytes(_) | Value::Undecodable(_)) {
        return None;
    }
    if matches!(value, Value::Float(number) if !number.is_finite()) {
        return None;
    }
    if let Value::Bool(value) = value {
        return Some(
            match (driver_id, value) {
                ("mssql", true) => "1",
                ("mssql", false) => "0",
                (_, true) => "TRUE",
                (_, false) => "FALSE",
            }
            .into(),
        );
    }
    crate::sql_literal::render_sql_literal(driver_id, value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::test_support::column;
    use crate::export::value_to_text;

    #[test]
    fn in_clause_renderer_quotes_values_and_reports_unusable_values() {
        let rows = vec![
            vec![Value::Int(1)],
            vec![Value::Text("O'Reilly".into())],
            vec![Value::Bool(true)],
            vec![Value::Null],
            vec![Value::Bytes(vec![0])],
            vec![Value::Undecodable("NUMERIC".into())],
        ];

        assert_eq!(
            render_in_clause("postgres", &rows, 0),
            InClause {
                sql: "(1, 'O''Reilly', TRUE)".into(),
                skipped: 3
            }
        );
        assert_eq!(render_in_clause("postgres", &rows, 1), InClause::default());
    }

    #[test]
    fn in_clause_uses_dialect_escaping_and_preserves_fractional_time() {
        let rows = vec![vec![Value::Text("x\\' OR 1=1 -- ".into())]];
        assert_eq!(render_in_clause("clickhouse", &rows, 0).sql, "('x\\\\'' OR 1=1 -- ')");
        assert_eq!(
            render_in_clause("mysql", &rows, 0).sql,
            "(_utf8mb4 X'785c27204f5220313d31202d2d20')"
        );
        assert_eq!(render_in_clause("postgres", &rows, 0).sql, "(E'x\\\\'' OR 1=1 -- ')");
        assert!(render_in_clause("redis", &rows, 0).sql.is_empty());
        assert_eq!(render_in_clause("redis", &rows, 0).skipped, 1);
        let time = Value::Time("12:34:56.123456".parse().unwrap());
        assert_eq!(value_to_text(&time).as_deref(), Some("12:34:56.123456"));
        assert_eq!(
            render_in_clause("postgres", &[vec![time]], 0).sql,
            "('12:34:56.123456')"
        );
    }

    #[test]
    fn value_list_renderer_formats_sql_tuples_and_handles_nulls() {
        let columns = vec![column("id"), column("title"), column("is_active")];
        let rows = vec![
            vec![Value::Int(1), Value::Text("Book's Title".into()), Value::Bool(true)],
            vec![Value::Int(2), Value::Null, Value::Bool(false)],
        ];

        let sql = render_value_list("postgres", &columns, &rows);
        assert_eq!(sql, "(1, 'Book''s Title', TRUE),\n(2, NULL, FALSE)");
        assert_eq!(render_value_list("postgres", &columns, &[]), "");
        assert_eq!(render_value_list("postgres", &[], &rows), "");
    }

    #[test]
    fn value_list_renderer_formats_dates_and_dialect_specific_literals() {
        let columns = vec![column("id"), column("created"), column("flag")];
        let date = Value::Date(chrono::NaiveDate::from_ymd_opt(2026, 10, 29).unwrap());
        let rows = vec![vec![Value::Int(10), date, Value::Bool(true)]];

        let mssql_sql = render_value_list("mssql", &columns, &rows);
        assert_eq!(mssql_sql, "(10, N'2026-10-29', 1)");

        let sqlite_sql = render_value_list("sqlite", &columns, &rows);
        assert_eq!(sqlite_sql, "(10, '2026-10-29', TRUE)");
    }
}
