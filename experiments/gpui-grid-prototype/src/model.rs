pub const ALLOWED_ROWS: [u64; 3] = [150_000, 500_000, 1_000_000];
pub const DEFAULT_ROWS: u64 = 1_000_000;
pub const MAX_FIND_MATCHES: usize = 10_000;
pub const MAX_DIFF_ROWS: usize = 200;
pub const PAGE_ROWS: u64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
}

impl Cell {
    pub fn display(&self) -> String {
        match self {
            Cell::Null => "NULL".into(),
            Cell::Bool(value) => value.to_string(),
            Cell::Int(value) => value.to_string(),
            Cell::Text(value) => value.clone(),
        }
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
    if index % 3 == 0 {
        Cell::Null
    } else {
        Cell::Bool(index % 2 == 0)
    }
}

pub fn cells_of(index: u64, label: &str) -> Vec<Cell> {
    vec![
        Cell::Int(value_of(index)),
        Cell::Text(label.to_string()),
        state_of(index),
    ]
}

pub fn row_at(index: u64) -> Vec<Cell> {
    cells_of(index, &label_of(index))
}

pub fn column_text(index: u64, column: Column) -> Option<String> {
    match column {
        Column::Value => Some(value_of(index).to_string()),
        Column::Label => Some(label_of(index)),
        Column::State => match state_of(index) {
            Cell::Bool(flag) => Some(flag.to_string()),
            _ => None,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Value,
    Label,
    State,
}

impl Column {
    pub const ALL: [Column; 3] = [Column::Value, Column::Label, Column::State];

    pub fn parse(name: &str) -> Option<Column> {
        match name {
            "value" => Some(Column::Value),
            "label" => Some(Column::Label),
            "state" => Some(Column::State),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Column::Value => "value",
            Column::Label => "label",
            Column::State => "state",
        }
    }
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
    pub column: Column,
    pub op: Op,
    pub text: String,
}

pub fn parse_predicate(input: &str) -> Result<Predicate, &'static str> {
    let mut parts = input.trim().splitn(3, ' ');
    let column = parts.next().and_then(Column::parse).ok_or("invalid filter")?;
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

pub fn matches_predicate(index: u64, predicate: &Predicate) -> bool {
    let needle = predicate.text.to_lowercase();
    let text = column_text(index, predicate.column).map(|text| text.to_lowercase());
    match predicate.op {
        Op::Contains => text.is_some_and(|text| text.contains(&needle)),
        Op::Equals => text.is_some_and(|text| text == needle),
        Op::NotEquals => text.is_none_or(|text| text != needle),
    }
}

pub fn filter_rows(total: u64, predicate: &Predicate) -> Vec<u32> {
    (0..total)
        .filter(|index| matches_predicate(*index, predicate))
        .map(|index| index as u32)
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindResult {
    pub matches: Vec<u64>,
    pub capped: bool,
    pub scanned: u64,
}

pub fn find_rows(total: u64, query: &str, cap: usize) -> Result<FindResult, &'static str> {
    if cap == 0 || cap > MAX_FIND_MATCHES {
        return Err("match cap exceeds the prototype limit");
    }
    if query.is_empty() || query.len() > 256 {
        return Err("invalid search text");
    }
    let needle = query.to_lowercase();
    let mut matches = Vec::new();
    for index in 0..total {
        let hit = Column::ALL
            .iter()
            .filter_map(|column| column_text(index, *column))
            .any(|text| text.to_lowercase().contains(&needle));
        if !hit {
            continue;
        }
        if matches.len() == cap {
            return Ok(FindResult {
                matches,
                capped: true,
                scanned: index,
            });
        }
        matches.push(index);
    }
    Ok(FindResult {
        matches,
        capped: false,
        scanned: total,
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffSummary {
    pub keyed: bool,
    pub equal: u64,
    pub added: u64,
    pub removed: u64,
    pub changed: u64,
}

#[derive(Debug, PartialEq, Eq)]
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

pub fn build_diff(total: u64, keyed: bool) -> Diff {
    let mut summary = DiffSummary {
        keyed,
        ..DiffSummary::default()
    };
    let mut rows: Vec<DiffRow> = Vec::new();
    let mut capped = false;
    for index in 0..total {
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
    Diff {
        summary,
        rows,
        capped,
    }
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
