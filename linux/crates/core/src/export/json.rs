use std::collections::HashSet;
use std::io::{self, Write};

use super::error::ExportError;
use super::file::ResultWriter;
use super::value_to_text;
use crate::query::{ColumnInfo, Value};

pub fn json_field_names(columns: &[ColumnInfo]) -> Vec<String> {
    let mut reserved = columns.iter().map(|column| column.name.clone()).collect::<HashSet<_>>();
    let mut emitted = HashSet::with_capacity(columns.len());
    let mut names = Vec::with_capacity(columns.len());
    for column in columns {
        let mut name = column.name.clone();
        if !emitted.insert(name.clone()) {
            let mut suffix = 2;
            loop {
                let candidate = format!("{}_{suffix}", column.name);
                if !reserved.contains(&candidate) && emitted.insert(candidate.clone()) {
                    name = candidate;
                    break;
                }
                suffix += 1;
            }
            reserved.insert(name.clone());
        }
        names.push(name);
    }
    names
}

pub fn row_to_json(columns: &[ColumnInfo], row: &[Value]) -> serde_json::Value {
    row_to_json_object(&json_field_names(columns), row)
}

pub fn render_json(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
    let names = json_field_names(columns);
    let values = rows
        .iter()
        .map(|row| row_to_json_object(&names, row))
        .collect::<Vec<_>>();
    match serde_json::to_string_pretty(&values) {
        Ok(output) => output,
        Err(_) => "[]".to_string(),
    }
}

pub(crate) fn row_to_json_object(names: &[String], row: &[Value]) -> serde_json::Value {
    let mut object = serde_json::Map::with_capacity(names.len());
    for (index, name) in names.iter().enumerate() {
        let value = match row.get(index) {
            Some(value) => value_to_json(value),
            None => serde_json::Value::Null,
        };
        object.insert(name.clone(), value);
    }
    serde_json::Value::Object(object)
}

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(value) => serde_json::Value::Bool(*value),
        Value::Int(value) => serde_json::Value::Number((*value).into()),
        Value::Float(value) => serde_json::Number::from_f64(*value).map_or_else(
            || serde_json::Value::String(value.to_string()),
            serde_json::Value::Number,
        ),
        Value::Decimal(value) => match value.to_string().parse() {
            Ok(value) => serde_json::Value::Number(value),
            Err(_) => serde_json::Value::String(value.to_string()),
        },
        Value::Json(value) => value.clone(),
        _ => value_to_text(value).map_or(serde_json::Value::Null, serde_json::Value::String),
    }
}

pub(crate) struct JsonWriter {
    names: Vec<String>,
}

impl JsonWriter {
    pub(crate) fn new() -> Self {
        Self { names: Vec::new() }
    }
}

impl ResultWriter for JsonWriter {
    fn begin(&mut self, output: &mut dyn Write, columns: &[ColumnInfo]) -> Result<(), ExportError> {
        self.names = json_field_names(columns);
        output.write_all(b"[\n")?;
        Ok(())
    }

    fn write_row(&mut self, output: &mut dyn Write, index: usize, row: &[Value]) -> Result<(), ExportError> {
        if index != 0 {
            output.write_all(b",\n")?;
        }
        serde_json::to_writer(&mut *output, &row_to_json_object(&self.names, row)).map_err(io::Error::from)?;
        Ok(())
    }

    fn finish(&mut self, output: &mut dyn Write) -> Result<(), ExportError> {
        output.write_all(b"\n]\n")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::test_support::column;

    #[test]
    fn nonfinite_numbers_remain_distinct_from_sql_null() {
        let columns = vec![column("value")];
        let rows = vec![
            vec![Value::Float(f64::NAN)],
            vec![Value::Float(f64::INFINITY)],
            vec![Value::Float(f64::NEG_INFINITY)],
            vec![Value::Null],
        ];
        let output: serde_json::Value = serde_json::from_str(&render_json(&columns, &rows)).unwrap();
        assert_eq!(
            output,
            serde_json::json!([{"value": "NaN"}, {"value": "inf"}, {"value": "-inf"}, {"value": null}])
        );
    }

    #[test]
    fn json_renderer_keeps_value_types_duplicate_columns_and_missing_cells() {
        let columns = vec![column("id"), column("id"), column("id_2"), column("payload")];
        let rows = vec![
            vec![
                Value::Int(7),
                Value::Float(1.5),
                Value::Decimal("12.30".parse().unwrap()),
                Value::Json(serde_json::json!({"ok": true})),
            ],
            vec![Value::Int(8)],
        ];

        assert_eq!(json_field_names(&columns), vec!["id", "id_3", "id_2", "payload"]);
        assert_eq!(
            render_json(&columns, &rows),
            "[\n  {\n    \"id\": 7,\n    \"id_3\": 1.5,\n    \"id_2\": 12.30,\n    \"payload\": {\n      \"ok\": true\n    }\n  },\n  {\n    \"id\": 8,\n    \"id_3\": null,\n    \"id_2\": null,\n    \"payload\": null\n  }\n]"
        );
    }
}
