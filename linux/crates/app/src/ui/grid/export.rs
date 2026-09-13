use tablepro_core::{ColumnInfo, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InClause {
    pub sql: String,
    pub skipped: usize,
}

pub(super) fn value_to_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(value) => Some(value.to_string()),
        Value::Int(value) => Some(value.to_string()),
        Value::Float(value) => Some(value.to_string()),
        Value::Text(value) => Some(value.clone()),
        Value::Bytes(value) => Some(format!(
            "0x{}",
            value.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        )),
        Value::Date(value) => Some(value.format("%Y-%m-%d").to_string()),
        Value::Time(value) => Some(value.format("%H:%M:%S").to_string()),
        Value::DateTime(value) => Some(value.format("%Y-%m-%d %H:%M:%S").to_string()),
        Value::TimestampTz(value) => Some(value.to_rfc3339()),
        Value::Decimal(value) => Some(value.to_string()),
        Value::Uuid(value) => Some(value.to_string()),
        Value::Json(value) => Some(value.to_string()),
    }
}

pub(super) fn render_tsv(columns: &[ColumnInfo], rows: &[Vec<Value>], with_headers: bool) -> String {
    let mut lines = Vec::with_capacity(rows.len() + usize::from(with_headers));
    if with_headers {
        lines.push(
            columns
                .iter()
                .map(|column| tsv_field(&column.name))
                .collect::<Vec<_>>()
                .join("\t"),
        );
    }
    lines.extend(rows.iter().map(|row| {
        row.iter()
            .map(|value| tsv_field(&value_to_text(value).unwrap_or_else(|| "NULL".to_string())))
            .collect::<Vec<_>>()
            .join("\t")
    }));
    lines.join("\n")
}

pub(super) fn render_csv(columns: &[ColumnInfo], rows: &[Vec<Value>], with_headers: bool) -> String {
    let mut lines = Vec::with_capacity(rows.len() + usize::from(with_headers));
    if with_headers {
        lines.push(
            columns
                .iter()
                .map(|column| csv_field(&column.name))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    lines.extend(rows.iter().map(|row| {
        row.iter()
            .map(|value| csv_field(&value_to_text(value).unwrap_or_default()))
            .collect::<Vec<_>>()
            .join(",")
    }));
    lines.join("\n")
}

pub(super) fn render_json(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
    let names = json_field_names(columns);
    let rows = rows
        .iter()
        .map(|row| row_to_json_with_names(&names, row))
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".to_string())
}

pub(super) fn row_to_json(columns: &[ColumnInfo], row: &[Value]) -> serde_json::Value {
    row_to_json_with_names(&json_field_names(columns), row)
}

pub(super) fn render_markdown(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
    let mut lines = Vec::with_capacity(rows.len() + 2);
    lines.push(format!(
        "| {} |",
        columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    lines.push(format!(
        "| {} |",
        columns.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")
    ));
    lines.extend(rows.iter().map(|row| {
        let cells = row
            .iter()
            .map(|value| {
                value_to_text(value)
                    .unwrap_or_else(|| "NULL".to_string())
                    .replace('|', "\\|")
                    .replace("\r\n", "<br>")
                    .replace(['\r', '\n'], "<br>")
            })
            .collect::<Vec<_>>();
        format!("| {} |", cells.join(" | "))
    }));
    lines.join("\n")
}

pub(super) fn render_in_clause(rows: &[Vec<Value>], column_index: usize) -> InClause {
    let values = rows.iter().filter_map(|row| row.get(column_index)).collect::<Vec<_>>();
    let literals = values
        .iter()
        .filter_map(|value| in_clause_literal(value))
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

fn tsv_field(value: &str) -> String {
    if value.contains(['\t', '\n', '\r', '"']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn json_field_names(columns: &[ColumnInfo]) -> Vec<String> {
    let mut names = Vec::with_capacity(columns.len());
    for column in columns {
        let mut name = column.name.clone();
        let mut suffix = 2;
        while names.contains(&name) {
            name = format!("{}_{}", column.name, suffix);
            suffix += 1;
        }
        names.push(name);
    }
    names
}

fn row_to_json_with_names(names: &[String], row: &[Value]) -> serde_json::Value {
    let values = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            (
                name.clone(),
                row.get(index).map(value_to_json).unwrap_or(serde_json::Value::Null),
            )
        })
        .collect();
    serde_json::Value::Object(values)
}

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(value) => serde_json::Value::Bool(*value),
        Value::Int(value) => serde_json::Value::Number((*value).into()),
        Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Decimal(value) => value
            .to_string()
            .parse()
            .map(serde_json::Value::Number)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
        Value::Json(value) => value.clone(),
        value => value_to_text(value)
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    }
}

fn in_clause_literal(value: &Value) -> Option<String> {
    match value {
        Value::Null | Value::Bytes(_) => None,
        Value::Bool(value) => Some(if *value {
            "TRUE".to_string()
        } else {
            "FALSE".to_string()
        }),
        Value::Int(_) | Value::Float(_) | Value::Decimal(_) => value_to_text(value),
        value => value_to_text(value).map(|value| format!("'{}'", value.replace('\'', "''"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.to_string(),
            data_type: "text".to_string(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        }
    }

    #[test]
    fn renderers_preserve_headers_and_escape_values() {
        let columns = vec![column("a"), column("b")];
        let rows = vec![vec![
            Value::Text("first\tvalue".to_string()),
            Value::Text("a,b".to_string()),
        ]];
        assert_eq!(render_tsv(&columns, &rows, true), "a\tb\n\"first\tvalue\"\ta,b");
        assert_eq!(render_csv(&columns, &rows, true), "a,b\nfirst\tvalue,\"a,b\"");
    }

    #[test]
    fn json_and_in_clause_handle_null_and_binary_values() {
        let columns = vec![column("id"), column("id")];
        let rows = vec![
            vec![Value::Int(7), Value::Null],
            vec![Value::Bytes(vec![1]), Value::Text("O'Brien".to_string())],
        ];
        assert_eq!(
            render_json(&columns, &rows),
            "[\n  {\n    \"id\": 7,\n    \"id_2\": null\n  },\n  {\n    \"id\": \"0x01\",\n    \"id_2\": \"O'Brien\"\n  }\n]"
        );
        assert_eq!(
            render_in_clause(&rows, 0),
            InClause {
                sql: "(7)".to_string(),
                skipped: 1
            }
        );
    }
}
