use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableInfo {
    pub schema: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualifiedTypeName {
    pub schema: String,
    pub name: String,
}

/// Secondary-index metadata for a table. `primary` is set on the
/// auto-PK index returned by the catalog query so the UI can render
/// it as read-only (the PK is owned by the column definition, not
/// by an editable index entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_expression_key: bool,
}

/// Foreign-key constraint metadata. `on_delete` / `on_update` carry
/// the referential action as a normalised SQL keyword string
/// ("RESTRICT", "CASCADE", "SET NULL", "SET DEFAULT", "NO ACTION").
/// `None` means the driver returned a value we don't recognise — the
/// UI displays it as a dim-label "—" and the DDL builder omits the
/// clause so the database picks its default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKeyInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: Option<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    /// True for `SERIAL` / `BIGSERIAL` / `IDENTITY` (PG), `AUTO_INCREMENT`
    /// (MySQL), `INTEGER PRIMARY KEY` / `AUTOINCREMENT` (SQLite). Used by
    /// the inline-insert UX to skip these columns from the user-facing
    /// draft form (DB assigns the value on commit).
    #[serde(default)]
    pub is_auto_increment: bool,
    /// Server-side default expression as raw text (e.g. `now()`,
    /// `gen_random_uuid()`, `'pending'`). When the user leaves a cell
    /// empty in a draft row and the column has a default, omit the
    /// column from the INSERT so the server applies its default.
    #[serde(default)]
    pub default_value: Option<String>,
    /// True for `GENERATED ALWAYS AS ...` columns. Always read-only;
    /// excluded from INSERT and UPDATE.
    #[serde(default)]
    pub is_generated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,
    /// The custom PostgreSQL enum type for a scalar enum column, or the
    /// qualified enum/domain element type for an array of custom values.
    #[serde(skip)]
    pub enum_type: Option<QualifiedTypeName>,
    /// The declared PostgreSQL domain type when a column is a domain.
    #[serde(skip)]
    pub domain_type: Option<QualifiedTypeName>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    Date(NaiveDate),
    Time(NaiveTime),
    DateTime(NaiveDateTime),
    TimestampTz(DateTime<Utc>),
    Decimal(Decimal),
    Uuid(Uuid),
    Json(Box<serde_json::Value>),
    JsonText(String),
    /// A non-NULL cell the driver could not decode (e.g. a wide NUMERIC
    /// sqlx cannot represent). Carries the driver's column type name so the
    /// UI can explain the gap instead of showing it as an editable NULL.
    Undecodable(String),
}

/// Soft upper bound on rows materialized by an arbitrary SQL `query` call.
/// When a result hits this limit, drivers set `QueryResult::truncated = true`.
/// Browse pagination via `fetch_rows` uses its caller-supplied `limit` and
/// is not capped here. Prefer streaming export ([`crate::export`]) for
/// large result sets instead of raising this further for GUI queries.
pub const MAX_QUERY_ROWS: usize = 1_000_000;
pub const MAX_QUERY_RESULT_CELLS: usize = 10_000_000;
pub const MAX_QUERY_RESULT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_QUERY_CURSOR_PAGE_ROWS: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueryCursorId(uuid::Uuid);

impl QueryCursorId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for QueryCursorId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct QueryCursorPage {
    pub result: QueryResult,
    pub has_next_page: bool,
}

#[derive(Debug, Default)]
pub struct QueryResultBudget {
    rows: usize,
    cells: usize,
    bytes: usize,
}

impl QueryResultBudget {
    pub fn admit(&mut self, row: &[Value]) -> bool {
        let row_bytes = row_size(row);
        if !self.can_admit(row.len(), row_bytes) {
            return false;
        }
        self.rows += 1;
        self.cells += row.len();
        self.bytes += row_bytes;
        true
    }

    pub fn decode_and_admit_row<E>(
        &mut self,
        cell_count: usize,
        payload_bytes: usize,
        decode: impl FnOnce() -> Result<Vec<Value>, E>,
    ) -> Result<Option<Vec<Value>>, E> {
        if !self.can_admit(cell_count, row_size_from_parts(cell_count, payload_bytes)) {
            return Ok(None);
        }
        let row = decode()?;
        Ok(self.admit(&row).then_some(row))
    }

    fn can_admit(&self, cells: usize, row_bytes: usize) -> bool {
        self.rows < MAX_QUERY_ROWS
            && self.cells.saturating_add(cells) <= MAX_QUERY_RESULT_CELLS
            && self.bytes.saturating_add(row_bytes) <= MAX_QUERY_RESULT_BYTES
    }
}

fn value_size(value: &Value) -> usize {
    match value {
        Value::Text(text) | Value::JsonText(text) | Value::Undecodable(text) => text.capacity(),
        Value::Bytes(bytes) => bytes.capacity(),
        Value::Json(value) => std::mem::size_of::<serde_json::Value>().saturating_add(json_heap_size(value)),
        _ => 0,
    }
}

fn row_size(row: &[Value]) -> usize {
    let payload_bytes = row
        .iter()
        .fold(0usize, |total, value| total.saturating_add(value_size(value)));
    row_size_from_parts(row.len(), payload_bytes)
}

fn row_size_from_parts(cells: usize, payload_bytes: usize) -> usize {
    cells
        .saturating_mul(std::mem::size_of::<Value>())
        .saturating_add(std::mem::size_of::<Vec<Value>>())
        .saturating_add(payload_bytes)
}

fn json_heap_size(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(value) => value.capacity(),
        serde_json::Value::Array(values) => values
            .capacity()
            .saturating_mul(std::mem::size_of::<serde_json::Value>())
            .saturating_add(
                values
                    .iter()
                    .fold(0usize, |total, value| total.saturating_add(json_heap_size(value))),
            ),
        serde_json::Value::Object(values) => values.iter().fold(0usize, |total, (key, value)| {
            total
                .saturating_add(std::mem::size_of::<(String, serde_json::Value)>())
                .saturating_add(key.capacity())
                .saturating_add(json_heap_size(value))
        }),
        _ => 0,
    }
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<Value>>,
    pub truncated: bool,
}

impl QueryResult {
    pub fn estimated_row_bytes(&self) -> usize {
        self.rows
            .iter()
            .fold(0usize, |total, row| total.saturating_add(row_size(row)))
    }
}

/// Ordered row sets returned by one query execution. The truncation flag and
/// query budget apply to the whole batch, not independently to each set.
#[derive(Debug, Clone)]
pub struct QueryResultBatch {
    pub result_sets: Vec<QueryResult>,
    pub truncated: bool,
}

impl QueryResultBatch {
    pub fn single(result: QueryResult) -> Self {
        let truncated = result.truncated;
        Self {
            result_sets: vec![result],
            truncated,
        }
    }

    pub fn into_first(self) -> QueryResult {
        let first = self.result_sets.into_iter().next().unwrap_or(QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            truncated: self.truncated,
        });
        QueryResult {
            truncated: self.truncated || first.truncated,
            ..first
        }
    }

    pub fn retained_rows(&self) -> u64 {
        self.result_sets
            .iter()
            .map(|result_set| result_set.rows.len() as u64)
            .sum()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExecResult {
    pub rows_affected: u64,
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
