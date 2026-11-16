use std::sync::Arc;

use bookie_core::{ColumnInfo, Connection, DriverError, OperationControl, RoutineLookup, RoutineRef, TableInfo};
use serde_json::{Value as JsonValue, json};
use uuid::Uuid;

use crate::bridge::{McpBridge, validated_identifier};
use crate::errors::McpToolError;
use crate::tokens::McpToken;

pub const SCHEMA_CONTEXT_MAX_TABLES: usize = 200;
pub const SCHEMA_CONTEXT_MAX_COLUMNS_PER_TABLE: usize = 100;
pub const SCHEMA_CONTEXT_MAX_COLUMNS: usize = 5_000;
pub const SCHEMA_CONTEXT_MAX_BYTES: usize = 64 * 1024;
const MAX_ARGUMENTS_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextLimit {
    Tables,
    Columns,
    Bytes,
}

impl ContextLimit {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tables => "tables",
            Self::Columns => "columns",
            Self::Bytes => "bytes",
        }
    }
}

pub(crate) struct ContextBuilder {
    tables: Vec<JsonValue>,
    columns_truncated: bool,
    total_tables: usize,
    columns: usize,
    bytes: usize,
    skipped: usize,
    limit: Option<ContextLimit>,
}

impl ContextBuilder {
    pub(crate) fn new(total_tables: usize) -> Self {
        Self {
            tables: Vec::new(),
            columns_truncated: false,
            total_tables,
            columns: 0,
            bytes: 0,
            skipped: 0,
            limit: None,
        }
    }

    pub(crate) fn skip(&mut self) {
        self.skipped += 1;
    }

    pub(crate) fn push(&mut self, table: &TableInfo, columns: &[ColumnInfo]) -> bool {
        if self.tables.len() >= SCHEMA_CONTEXT_MAX_TABLES {
            self.limit = Some(ContextLimit::Tables);
            return false;
        }
        let shown = &columns[..columns.len().min(SCHEMA_CONTEXT_MAX_COLUMNS_PER_TABLE)];
        if self.columns + shown.len() > SCHEMA_CONTEXT_MAX_COLUMNS {
            self.limit = Some(ContextLimit::Columns);
            return false;
        }
        let rendered = table_json(table, shown, shown.len() < columns.len());
        let size = rendered.to_string().len();
        if self.bytes + size > SCHEMA_CONTEXT_MAX_BYTES {
            self.limit = Some(ContextLimit::Bytes);
            return false;
        }
        self.columns += shown.len();
        self.bytes += size;
        self.columns_truncated |= shown.len() < columns.len();
        self.tables.push(rendered);
        true
    }

    pub(crate) fn finish(self) -> JsonValue {
        json!({
            "tables": self.tables,
            "table_count": self.total_tables,
            "included_tables": self.tables.len(),
            "skipped_tables": self.skipped,
            "truncated": self.limit.is_some() || self.columns_truncated,
            "limit": self.limit.map(ContextLimit::as_str),
            "limits": {
                "max_tables": SCHEMA_CONTEXT_MAX_TABLES,
                "max_columns_per_table": SCHEMA_CONTEXT_MAX_COLUMNS_PER_TABLE,
                "max_columns": SCHEMA_CONTEXT_MAX_COLUMNS,
                "max_bytes": SCHEMA_CONTEXT_MAX_BYTES,
            },
        })
    }
}

fn table_json(table: &TableInfo, columns: &[ColumnInfo], columns_truncated: bool) -> JsonValue {
    json!({
        "schema": table.schema,
        "name": table.name,
        "columns": columns.iter().map(|column| json!({
            "name": column.name,
            "data_type": column.data_type,
            "nullable": column.nullable,
        })).collect::<Vec<_>>(),
        "primary_key": columns.iter().filter(|column| column.primary_key).map(|column| &column.name).collect::<Vec<_>>(),
        "columns_truncated": columns_truncated,
    })
}

fn skippable(error: &DriverError) -> bool {
    matches!(error, DriverError::Query { .. } | DriverError::PolicyDenied(_))
}

async fn collect_schema_context(
    connection: &dyn Connection,
    schema: Option<&str>,
    control: &OperationControl,
) -> Result<JsonValue, String> {
    let mut tables = connection
        .list_tables_controlled(control)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(schema) = schema {
        tables.retain(|table| table.schema.as_deref() == Some(schema));
    }
    tables.sort_by(|left, right| (&left.schema, &left.name).cmp(&(&right.schema, &right.name)));
    let mut builder = ContextBuilder::new(tables.len());
    for table in &tables {
        let columns = connection
            .fetch_columns_controlled(table.schema.as_deref(), &table.name, control)
            .await;
        match columns {
            Ok(columns) if builder.push(table, &columns) => {}
            Ok(_) => break,
            Err(error) if skippable(&error) => builder.skip(),
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(builder.finish())
}

fn routine_json(lookup: RoutineLookup) -> JsonValue {
    match lookup {
        RoutineLookup::Found(found) => json!({
            "found": true,
            "schema": found.schema,
            "name": found.name,
            "kind": found.kind,
            "arguments": found.identity_arguments,
            "definition": found.definition,
            "truncated": found.truncated,
            "total_bytes": found.total_bytes,
        }),
        RoutineLookup::NotFound => json!({"found": false}),
        RoutineLookup::Ambiguous { signatures } => json!({
            "found": false,
            "ambiguous": true,
            "signatures": signatures,
        }),
    }
}

fn optional_arguments(args: &JsonValue) -> Result<Option<String>, String> {
    let Some(raw) = args.get("arguments").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let text = raw.as_str().ok_or("arguments must be a string")?;
    if text.len() > MAX_ARGUMENTS_BYTES {
        return Err(format!("arguments exceed {MAX_ARGUMENTS_BYTES} bytes"));
    }
    if text.chars().any(char::is_control) {
        return Err("arguments must not contain control characters".into());
    }
    Ok(Some(text.to_string()))
}

fn required_identifier(args: &JsonValue, key: &str) -> Result<String, String> {
    let raw = args
        .get(key)
        .and_then(JsonValue::as_str)
        .ok_or_else(|| format!("missing {key}"))?;
    validated_identifier(raw)
}

fn connection_id(args: &JsonValue) -> Result<Uuid, String> {
    let raw = args
        .get("connection_id")
        .and_then(JsonValue::as_str)
        .ok_or("missing connection_id")?;
    Uuid::parse_str(raw).map_err(|error| error.to_string())
}

pub(crate) fn tool_schema(name: &str) -> JsonValue {
    match name {
        "routine_source" => json!({
            "type": "object",
            "properties": {
                "connection_id": {"type": "string"},
                "schema": {"type": "string"},
                "name": {"type": "string"},
                "arguments": {"type": "string", "maxLength": MAX_ARGUMENTS_BYTES},
                "token": {"type": "string"}
            },
            "required": ["connection_id", "schema", "name"]
        }),
        _ => json!({
            "type": "object",
            "properties": {
                "connection_id": {"type": "string"},
                "schema": {"type": "string"},
                "token": {"type": "string"}
            },
            "required": ["connection_id"]
        }),
    }
}

pub(crate) async fn routine_source(
    bridge: &McpBridge,
    token: &McpToken,
    args: &JsonValue,
) -> Result<JsonValue, McpToolError> {
    let id = connection_id(args)?;
    let control = bridge.start_connection_operation(token, id)?;
    let routine = RoutineRef {
        schema: required_identifier(args, "schema")?,
        name: required_identifier(args, "name")?,
        identity_arguments: optional_arguments(args)?,
    };
    let lookup = bridge
        .with_connection_controlled(token, id, control, move |connection: Arc<dyn Connection>, control| {
            Box::pin(async move {
                connection
                    .routine_definition_controlled(&routine, &control)
                    .await
                    .map_err(|error| error.to_string())
            })
        })
        .await?;
    Ok(routine_json(lookup))
}

pub(crate) async fn schema_context(
    bridge: &McpBridge,
    token: &McpToken,
    args: &JsonValue,
) -> Result<JsonValue, McpToolError> {
    let id = connection_id(args)?;
    let control = bridge.start_connection_operation(token, id)?;
    let schema = match args.get("schema").and_then(JsonValue::as_str) {
        Some(schema) => Some(validated_identifier(schema)?),
        None => None,
    };
    let context = bridge
        .with_connection_controlled(token, id, control, move |connection: Arc<dyn Connection>, control| {
            Box::pin(async move { collect_schema_context(connection.as_ref(), schema.as_deref(), &control).await })
        })
        .await?;
    Ok(context)
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
