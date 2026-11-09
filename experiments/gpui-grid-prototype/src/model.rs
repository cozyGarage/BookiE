use tokio_util::sync::CancellationToken;

pub const ALLOWED_ROWS: [u64; 3] = [150_000, 500_000, 1_000_000];
pub const DEFAULT_ROWS: u64 = 1_000_000;
pub const MAX_FIND_MATCHES: usize = 10_000;
pub const MAX_DIFF_ROWS: usize = 200;
pub const PAGE_ROWS: u64 = 5_000;
pub const TYPED_KINDS: u64 = 12;

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    Date(String),
    Time(String),
    DateTime(String),
    TimestampTz(String),
    Decimal(String),
    Uuid(String),
    Json(String),
    JsonText(String),
    Undecodable(String),
}

impl Cell {
    pub fn kind(&self) -> &'static str {
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

    pub fn display(&self) -> String {
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
            Cell::Text(value)
            | Cell::Date(value)
            | Cell::Time(value)
            | Cell::DateTime(value)
            | Cell::TimestampTz(value)
            | Cell::Decimal(value)
            | Cell::Uuid(value)
            | Cell::Json(value)
            | Cell::JsonText(value) => value.clone(),
        }
    }
}

pub fn typed_of(index: u64) -> Cell {
    match index % TYPED_KINDS {
        0 => Cell::Float(0.1 + 0.2),
        1 => Cell::Bytes(vec![0xde, 0xad, 0xbe, 0xef]),
        2 => Cell::Date("2026-11-04".into()),
        3 => Cell::Time("23:59:59.123456".into()),
        4 => Cell::DateTime("2026-11-04 23:59:59.123456".into()),
        5 => Cell::TimestampTz("2026-11-04T23:59:59.123456+00:00".into()),
        6 => Cell::Decimal("12345678901234567890.12345678".into()),
        7 => Cell::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8".into()),
        8 => Cell::Json(r#"{"a":[1,{"b":null}]}"#.into()),
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
}

pub fn synthetic_columns() -> Vec<ColumnMeta> {
    [("value", 240.0), ("label", 280.0), ("state", 120.0), ("typed", 320.0)]
        .into_iter()
        .map(|(name, width)| ColumnMeta {
            name: name.into(),
            width,
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

pub fn build_diff(total: u64, keyed: bool, cancel: &CancellationToken) -> Result<Diff, &'static str> {
    if cancel.is_cancelled() {
        return Err("comparison cancelled");
    }
    let mut summary = DiffSummary {
        keyed,
        ..DiffSummary::default()
    };
    let mut rows: Vec<DiffRow> = Vec::new();
    let mut capped = false;
    for index in 0..total {
        if index.is_multiple_of(1_024) && cancel.is_cancelled() {
            return Err("comparison cancelled");
        }
        let mut produced: Vec<(&'static str, Vec<Cell>)> = Vec::new();
        let original = label_of(index);
        let edited = format!("{original} (edited)");
        match right_state(index) {
            "same" => summary.equal += 1,
            "dropped" => {
                summary.removed += 1;
                produced.push(("Removed", cells_of(index, &original)));
            }
            _ if keyed => {
                summary.changed += 1;
                produced.push(("Before", cells_of(index, &original)));
                produced.push(("After", cells_of(index, &edited)));
            }
            _ => {
                summary.removed += 1;
                summary.added += 1;
                produced.push(("Removed", cells_of(index, &original)));
                produced.push(("Added", cells_of(index, &edited)));
            }
        }
        for (change, cells) in produced {
            if rows.len() < MAX_DIFF_ROWS {
                rows.push(DiffRow { change, cells });
            } else {
                capped = true;
            }
        }
    }
    Ok(Diff { summary, rows, capped })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pager {
    pub total: u64,
    pub page: u64,
}

impl Pager {
    pub fn new(total: u64) -> Self {
        Pager { total, page: 0 }
    }

    pub fn pages(&self) -> u64 {
        self.total.div_ceil(PAGE_ROWS).max(1)
    }

    pub fn first(&mut self) {
        self.page = 0;
    }

    pub fn last(&mut self) {
        self.page = self.pages() - 1;
    }

    pub fn next(&mut self) {
        self.page = (self.page + 1).min(self.pages() - 1);
    }

    pub fn previous(&mut self) {
        self.page = self.page.saturating_sub(1);
    }

    pub fn range(&self) -> (u64, u64) {
        if self.total == 0 {
            return (0, 0);
        }
        let start = self.page * PAGE_ROWS;
        (start + 1, (start + PAGE_ROWS).min(self.total))
    }

    pub fn contains_row(&mut self, row: u64) {
        self.page = (row / PAGE_ROWS).min(self.pages() - 1);
    }
}

#[derive(Debug, Default)]
pub struct MatchCursor {
    pub matches: Vec<u64>,
    pub current: Option<usize>,
}

impl MatchCursor {
    pub fn set(&mut self, matches: Vec<u64>) {
        self.matches = matches;
        self.current = None;
    }

    pub fn step(&mut self, forward: bool) -> Option<u64> {
        let count = self.matches.len();
        if count == 0 {
            return None;
        }
        let next = match (self.current, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
        };
        self.current = Some(next);
        self.matches.get(next).copied()
    }

    pub fn status(&self, capped: bool) -> String {
        let count = self.matches.len();
        if count == 0 {
            return "No match".into();
        }
        let shown = self.current.map_or(0, |index| index + 1);
        let more = if capped { "+" } else { "" };
        format!("Match {shown} of {count}{more}")
    }
}
