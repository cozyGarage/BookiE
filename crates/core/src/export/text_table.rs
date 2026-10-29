use crate::export::value_to_text;
use crate::query::{ColumnInfo, Value};

fn is_numeric(value: &Value) -> bool {
    matches!(value, Value::Int(_) | Value::Float(_) | Value::Decimal(_))
}

fn format_value_for_table(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Decimal(d) => d.to_string(),
        other => value_to_text(other).unwrap_or_else(|| "NULL".to_string()),
    }
}

fn build_table_separator(widths: &[usize]) -> String {
    format!(
        "+{}+",
        widths.iter().map(|w| "-".repeat(w + 2)).collect::<Vec<_>>().join("+")
    )
}

fn collect_table_data(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> (Vec<usize>, Vec<bool>, Vec<Vec<String>>) {
    let col_count = columns.len();
    let mut widths: Vec<usize> = columns.iter().map(|c| c.name.chars().count()).collect();
    let mut formatted_rows = Vec::with_capacity(rows.len());
    let mut numeric_cols = vec![true; col_count];

    for row in rows {
        let mut formatted_row = Vec::with_capacity(col_count);
        for (i, col_width) in widths.iter_mut().enumerate().take(col_count) {
            let val = row.get(i).unwrap_or(&Value::Null);
            if !is_numeric(val) && !matches!(val, Value::Null) {
                numeric_cols[i] = false;
            }
            let s = format_value_for_table(val);
            let char_len = s.chars().count();
            if char_len > *col_width {
                *col_width = char_len;
            }
            formatted_row.push(s);
        }
        formatted_rows.push(formatted_row);
    }
    (widths, numeric_cols, formatted_rows)
}

pub fn render_text_table(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
    if columns.is_empty() {
        return String::new();
    }
    let (widths, numeric_cols, formatted_rows) = collect_table_data(columns, rows);
    let separator = build_table_separator(&widths);

    let mut out = String::new();
    out.push_str(&separator);
    out.push('\n');

    out.push('|');
    for (i, col) in columns.iter().enumerate() {
        let w = widths[i];
        out.push_str(&format!(" {:<width$} |", col.name, width = w));
    }
    out.push('\n');
    out.push_str(&separator);
    out.push('\n');

    for row in formatted_rows {
        out.push('|');
        for (i, cell) in row.into_iter().enumerate() {
            let w = widths[i];
            if numeric_cols[i] {
                out.push_str(&format!(" {:>width$} |", cell, width = w));
            } else {
                out.push_str(&format!(" {:<width$} |", cell, width = w));
            }
        }
        out.push('\n');
    }

    out.push_str(&separator);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::test_support::column;

    #[test]
    fn text_table_formats_borders_and_right_aligns_numbers() {
        let columns = vec![column("id"), column("name"), column("score")];
        let rows = vec![
            vec![
                Value::Int(1),
                Value::Text("Alice".into()),
                Value::Decimal(rust_decimal::Decimal::new(955, 1)),
            ],
            vec![Value::Int(2), Value::Text("Bob".into()), Value::Null],
        ];

        let table = render_text_table(&columns, &rows);
        let expected = "+----+-------+-------+\n| id | name  | score |\n+----+-------+-------+\n|  1 | Alice |  95.5 |\n|  2 | Bob   |  NULL |\n+----+-------+-------+";
        assert_eq!(table, expected);
    }

    #[test]
    fn text_table_handles_empty_rows_and_empty_columns() {
        assert_eq!(render_text_table(&[], &[]), "");

        let columns = vec![column("id"), column("title")];
        let table = render_text_table(&columns, &[]);
        let expected = "+----+-------+\n| id | title |\n+----+-------+\n+----+-------+";
        assert_eq!(table, expected);
    }

    #[test]
    fn text_table_formats_boolean_floats_and_unicode_text() {
        let columns = vec![column("flag"), column("ratio"), column("city")];
        let rows = vec![
            vec![Value::Bool(true), Value::Float(2.5), Value::Text("Tokyo".into())],
            vec![Value::Bool(false), Value::Float(0.0), Value::Text("Zürich".into())],
        ];

        let table = render_text_table(&columns, &rows);
        let expected = "+-------+-------+--------+\n| flag  | ratio | city   |\n+-------+-------+--------+\n| true  |   2.5 | Tokyo  |\n| false |     0 | Zürich |\n+-------+-------+--------+";
        assert_eq!(table, expected);
    }
}
