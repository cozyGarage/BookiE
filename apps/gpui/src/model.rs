pub const ALLOWED_ROWS: [u64; 3] = [150_000, 500_000, 1_000_000];
pub const DEFAULT_ROWS: u64 = 1_000_000;
pub const MAX_FIND_MATCHES: usize = 10_000;
pub const MAX_DIFF_ROWS: usize = 200;
pub const PAGE_ROWS: u64 = bookie_results::result_page::DEFAULT_PAGE_ROWS;
pub const PAGE_SIZES: [u64; 4] = bookie_results::result_page::PAGE_SIZES;
pub const TYPED_KINDS: u64 = 12;

/// A cell is the core value: one type for the grid, the copy formats and the drivers.
pub type Cell = bookie_core::Value;

/// Display and kind helpers for a cell, kept out of `bookie-core` because they are view text.
pub trait CellText {
    fn kind(&self) -> &'static str;
    fn display(&self) -> String;
}

impl CellText for Cell {
    fn kind(&self) -> &'static str {
        match self {
            Cell::Null => "null",
            Cell::Bool(_) => "boolean",
            Cell::Int(_) => "integer",
            Cell::Float(_) => "float",
            Cell::Text(_) => "text",
            Cell::Bytes(_) => "bytes",
            Cell::Date(_) => "date",
            Cell::Time(_) => "time",
            Cell::DateTime(_) => "datetime",
            Cell::TimestampTz(_) => "timestamptz",
            Cell::Decimal(_) => "decimal",
            Cell::Uuid(_) => "uuid",
            Cell::Json(_) => "json",
            Cell::JsonText(_) => "json_text",
            Cell::Undecodable(_) => "undecodable",
        }
    }

    fn display(&self) -> String {
        match self {
            Cell::Null => "NULL".into(),
            Cell::Bool(value) => value.to_string(),
            Cell::Int(value) => value.to_string(),
            Cell::Float(value) => value.to_string(),
            Cell::Bytes(value) => {
                let hex: String = value.iter().map(|byte| format!("{byte:02x}")).collect();
                format!("\\x{hex}")
            }
            Cell::Undecodable(type_name) => format!("<undecodable {type_name}>"),
            Cell::Date(value) => value.to_string(),
            Cell::Time(value) => value.to_string(),
            Cell::DateTime(value) => value.to_string(),
            Cell::TimestampTz(value) => value.to_rfc3339(),
            Cell::Decimal(value) => value.to_string(),
            Cell::Uuid(value) => value.to_string(),
            Cell::Json(value) => value.to_string(),
            Cell::JsonText(value) | Cell::Text(value) => value.clone(),
        }
    }
}

pub fn typed_of(index: u64) -> Cell {
    fn literal<T: std::str::FromStr>(text: &str) -> T {
        text.parse().unwrap_or_else(|_| panic!("synthetic literal {text}"))
    }
    match index % TYPED_KINDS {
        0 => Cell::Float(0.1 + 0.2),
        1 => Cell::Bytes(vec![0xde, 0xad, 0xbe, 0xef]),
        2 => Cell::Date(literal("2026-11-04")),
        3 => Cell::Time(literal("23:59:59.123456")),
        4 => Cell::DateTime(literal("2026-11-04T23:59:59.123456")),
        5 => Cell::TimestampTz(literal("2026-11-04T23:59:59.123456+00:00")),
        6 => Cell::Decimal(literal("12345678901234567890.12345678")),
        7 => Cell::Uuid(literal("67e55044-10b1-426f-9247-bb680e5fe0c8")),
        8 => Cell::Json(Box::new(
            serde_json::from_str(r#"{"a":[1,{"b":null}]}"#).unwrap_or_default(),
        )),
        9 => Cell::JsonText("{\"raw\":  1,   \"keep\" : 2}".into()),
        10 => Cell::Undecodable("numeric(40,20)".into()),
        _ => Cell::Null,
    }
}

pub fn rows_from(value: Option<&str>) -> u64 {
    value
        .and_then(|text| text.parse().ok())
        .filter(|rows| ALLOWED_ROWS.contains(rows))
        .unwrap_or(DEFAULT_ROWS)
}

pub fn value_of(index: u64) -> i64 {
    9_007_199_254_740_993_i64 + index as i64
}

pub fn label_of(index: u64) -> String {
    format!("Sample {index}")
}

pub fn state_of(index: u64) -> Cell {
    if index.is_multiple_of(3) {
        Cell::Null
    } else {
        Cell::Bool(index.is_multiple_of(2))
    }
}

pub fn cells_of(index: u64, label: &str) -> Vec<Cell> {
    vec![
        Cell::Int(value_of(index)),
        Cell::Text(label.to_string()),
        state_of(index),
        typed_of(index),
    ]
}

pub fn row_at(index: u64) -> Vec<Cell> {
    cells_of(index, &label_of(index))
}

pub fn column_text(index: u64, column: usize) -> Option<String> {
    match column {
        0 => Some(value_of(index).to_string()),
        1 => Some(label_of(index)),
        2 => match state_of(index) {
            Cell::Bool(flag) => Some(flag.to_string()),
            _ => None,
        },
        3 => match typed_of(index) {
            Cell::Null => None,
            cell => Some(cell.display()),
        },
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnMeta {
    pub name: String,
    pub width: f32,
    pub numeric: bool,
    /// The declared type name, parsed the same way as an edit of this column.
    pub data_type: String,
}

pub fn synthetic_columns() -> Vec<ColumnMeta> {
    [
        ("value", 240.0, true, "integer"),
        ("label", 280.0, false, "text"),
        ("state", 120.0, false, "boolean"),
        ("typed", 320.0, false, "text"),
    ]
    .into_iter()
    .map(|(name, width, numeric, data_type)| ColumnMeta {
        name: name.into(),
        width,
        numeric,
        data_type: data_type.into(),
    })
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Contains,
    Equals,
    NotEquals,
}

impl Op {
    pub fn parse(name: &str) -> Option<Op> {
        match name {
            "contains" => Some(Op::Contains),
            "equals" | "=" => Some(Op::Equals),
            "not_equals" | "!=" => Some(Op::NotEquals),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Predicate {
    pub column: usize,
    pub op: Op,
    pub text: String,
}

pub fn parse_predicate(input: &str, columns: &[ColumnMeta]) -> Result<Predicate, &'static str> {
    let mut parts = input.trim().splitn(3, ' ');
    let name = parts.next().ok_or("invalid filter")?;
    let column = columns
        .iter()
        .position(|column| column.name.eq_ignore_ascii_case(name))
        .ok_or("invalid filter")?;
    let op = parts.next().and_then(Op::parse).ok_or("invalid filter")?;
    let text = parts.next().unwrap_or("").trim();
    if text.is_empty() || text.len() > 256 {
        return Err("invalid filter");
    }
    Ok(Predicate {
        column,
        op,
        text: text.to_string(),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindResult {
    pub matches: Vec<u64>,
    pub capped: bool,
    pub scanned: u64,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffSummary {
    pub keyed: bool,
    pub equal: u64,
    pub added: u64,
    pub removed: u64,
    pub changed: u64,
    /// Rows that could not be compared exactly (an unreadable cell on either side).
    pub unknown: u64,
}

#[derive(Debug, PartialEq)]
pub struct DiffRow {
    pub change: &'static str,
    pub cells: Vec<Cell>,
}

#[derive(Debug)]
pub struct Diff {
    pub summary: DiffSummary,
    pub rows: Vec<DiffRow>,
    pub capped: bool,
}

fn right_state(index: u64) -> &'static str {
    match index % 100 {
        1 => "edited",
        2 => "dropped",
        _ => "same",
    }
}

/// The right-hand side of the synthetic comparison: `None` when the row was dropped.
pub fn right_row(index: u64) -> Option<Vec<Cell>> {
    match right_state(index) {
        "dropped" => None,
        "edited" => Some(cells_of(index, &format!("{} (edited)", label_of(index)))),
        _ => Some(row_at(index)),
    }
}

pub use bookie_results::result_page::{MatchCursor, Pager};

