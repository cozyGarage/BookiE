use serde::ser::SerializeMap;
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

pub fn render_row_json(columns: &[ColumnInfo], row: &[Value]) -> String {
    let names = json_field_names(columns);
    raw_json_object(&names, row)
        .and_then(|object| serde_json::to_string_pretty(&object))
        .unwrap_or_else(|_| "{}".to_string())
}

pub fn render_json(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
    let names = json_field_names(columns);
    let values = rows
        .iter()
        .map(|row| raw_json_object(&names, row))
        .collect::<Result<Vec<_>, _>>();
    let Ok(values) = values else {
        return "[]".to_string();
    };
    match serde_json::to_string_pretty(&values) {
        Ok(output) => output,
        Err(_) => "[]".to_string(),
    }
}

struct RawJsonObject(Vec<(String, JsonExportValue)>);

enum JsonExportValue {
    Raw(Box<serde_json::value::RawValue>),
    Value(serde_json::Value),
}

impl serde::Serialize for JsonExportValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Raw(value) => serde::Serialize::serialize(value, serializer),
            Self::Value(value) => serde::Serialize::serialize(value, serializer),
        }
    }
}

impl serde::Serialize for RawJsonObject {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (name, value) in &self.0 {
            map.serialize_entry(name, value)?;
        }
        map.end()
    }
}

fn raw_json_object(names: &[String], row: &[Value]) -> Result<RawJsonObject, serde_json::Error> {
    let mut object = Vec::with_capacity(names.len());
    for (index, name) in names.iter().enumerate() {
        let encoded = match row.get(index) {
            Some(Value::JsonText(source)) => serde_json::value::RawValue::from_string(source.clone())
                .map(JsonExportValue::Raw)
                .unwrap_or_else(|_| JsonExportValue::Value(serde_json::Value::String(source.clone()))),
            Some(value) => JsonExportValue::Value(value_to_json(value)),
            None => JsonExportValue::Value(serde_json::Value::Null),
        };
        object.push((name.clone(), encoded));
    }
    Ok(RawJsonObject(object))
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
        Value::Json(value) => value.as_ref().clone(),
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
        let object = raw_json_object(&self.names, row).map_err(io::Error::from)?;
        serde_json::to_writer(&mut *output, &object).map_err(io::Error::from)?;
        Ok(())
    }

    fn finish(&mut self, output: &mut dyn Write) -> Result<(), ExportError> {
        output.write_all(b"\n]\n")?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
