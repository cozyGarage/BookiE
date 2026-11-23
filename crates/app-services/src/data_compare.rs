use std::cmp::Ordering;

use async_trait::async_trait;
use bookie_core::sql_dialect::{BuildSqlError, placeholder_for, qualified_table, quote_ident};
use bookie_core::sql_literal::{build_delete_literal, build_insert_literal, build_update_literal, render_sql_literal};
use bookie_core::{ColumnInfo, Connection, DriverError, OperationControl, Value, check_pre_dispatch};

pub const PAGE_ROWS: usize = 1_000;
pub const SAMPLE_ROWS: usize = 200;
pub const MAX_PLAN_ROWS: usize = 50_000;
const SAMPLE_VALUE_CHARS: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialect {
    Postgres,
    Mysql,
    Sqlite,
    Mssql,
}

impl Dialect {
    fn from_id(driver_id: &str) -> Option<Self> {
        match driver_id {
            "postgres" => Some(Self::Postgres),
            "mysql" => Some(Self::Mysql),
            "sqlite" => Some(Self::Sqlite),
            "mssql" => Some(Self::Mssql),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Source,
    Target,
}

#[derive(Debug, thiserror::Error)]
pub enum DataCompareError {
    #[error("data compare does not support this engine")]
    Unsupported,
    #[error("the tables share no primary key; choose key columns that are NOT NULL on both sides")]
    NoKey,
    #[error("key column {0} must exist on both sides with the same type and be NOT NULL")]
    BadKey(String),
    #[error("the {0:?} table has a duplicate key")]
    DuplicateKey(Side),
    #[error("the {0:?} table returned keys out of order")]
    KeyOrder(Side),
    #[error("the {0:?} table has a key value that cannot be compared")]
    UncomparableKey(Side),
    #[error("the {0:?} read was truncated")]
    Truncated(Side),
    #[error("{0} differences are too many for one script")]
    TooManyDifferences(u64),
    #[error(transparent)]
    Driver(#[from] DriverError),
    #[error(transparent)]
    Script(#[from] BuildSqlError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub key: Vec<String>,
    pub compared: Vec<String>,
    pub not_compared: Vec<String>,
    columns: Vec<ColumnInfo>,
    identity: bool,
}

impl Layout {
    fn width(&self) -> usize {
        self.columns.len()
    }

    fn key_len(&self) -> usize {
        self.key.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SampleRow {
    pub change: Change,
    pub key: Vec<Value>,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub same: u64,
    pub insert: u64,
    pub update: u64,
    pub delete: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataCompare {
    pub layout: Layout,
    pub counts: Counts,
    pub unknown_cells: u64,
    pub sample: Vec<SampleRow>,
    inserts: Vec<Vec<Value>>,
    updates: Vec<(Vec<usize>, Vec<Value>)>,
    deletes: Vec<Vec<Value>>,
    complete: bool,
}

impl DataCompare {
    fn new(layout: Layout) -> Self {
        Self {
            layout,
            counts: Counts::default(),
            unknown_cells: 0,
            sample: Vec::new(),
            inserts: Vec::new(),
            updates: Vec::new(),
            deletes: Vec::new(),
            complete: true,
        }
    }

    pub fn is_identical(&self) -> bool {
        self.counts.insert == 0 && self.counts.update == 0 && self.counts.delete == 0
    }

    pub fn sample_lines(&self, driver_id: &str) -> Vec<String> {
        self.sample
            .iter()
            .map(|sample| {
                let change = match sample.change {
                    Change::Insert => "INSERT",
                    Change::Update => "UPDATE",
                    Change::Delete => "DELETE",
                };
                let key = sample
                    .key
                    .iter()
                    .map(|value| sample_value(driver_id, value))
                    .collect::<Vec<_>>()
                    .join(", ");
                if sample.columns.is_empty() {
                    format!("{change} ({key})")
                } else {
                    format!("{change} ({key}): {}", sample.columns.join(", "))
                }
            })
            .collect()
    }

    fn differences(&self) -> u64 {
        self.counts.insert + self.counts.update + self.counts.delete
    }

    fn record(&mut self, change: Change, row: Vec<Value>, changed: Vec<usize>) {
        match change {
            Change::Insert => self.counts.insert += 1,
            Change::Update => self.counts.update += 1,
            Change::Delete => self.counts.delete += 1,
        }
        let key_len = self.layout.key_len();
        if self.sample.len() < SAMPLE_ROWS {
            self.sample.push(SampleRow {
                change,
                key: row[..key_len].to_vec(),
                columns: changed
                    .iter()
                    .map(|index| self.layout.columns[*index].name.clone())
                    .collect(),
            });
        }
        if !self.complete || self.differences() > MAX_PLAN_ROWS as u64 {
            self.complete = false;
            return;
        }
        match change {
            Change::Insert => self.inserts.push(row),
            Change::Update => self.updates.push((changed, row)),
            Change::Delete => self.deletes.push(row),
        }
    }

    pub fn script(
        &self,
        driver_id: &str,
        schema: Option<&str>,
        table: &str,
        include_deletes: bool,
    ) -> Result<String, DataCompareError> {
        if !self.complete {
            return Err(DataCompareError::TooManyDifferences(self.differences()));
        }
        let mut out = String::from("-- Data compare: review before running. Nothing has been applied.\n");
        if self.is_identical() {
            out.push_str("-- The tables match, so there is nothing to change.\n");
            return Ok(out);
        }
        let columns = &self.layout.columns;
        let identity =
            self.layout.identity && Dialect::from_id(driver_id) == Some(Dialect::Mssql) && !self.inserts.is_empty();
        let qualified = qualified_table(driver_id, schema, table);
        if !self.inserts.is_empty() {
            out.push_str("\n-- Inserts\n");
            if identity {
                out.push_str(&format!("SET IDENTITY_INSERT {qualified} ON;\n"));
            }
            for row in &self.inserts {
                push_line(&mut out, &build_insert_literal(driver_id, schema, table, columns, row)?);
            }
            if identity {
                out.push_str(&format!("SET IDENTITY_INSERT {qualified} OFF;\n"));
            }
        }
        if !self.updates.is_empty() {
            out.push_str("\n-- Updates\n");
            for (changed, row) in &self.updates {
                let (subset, values) = self.update_subset(changed, row);
                push_line(
                    &mut out,
                    &build_update_literal(driver_id, schema, table, &subset, &values)?,
                );
            }
        }
        if !self.deletes.is_empty() {
            let title = if include_deletes {
                "Deletes"
            } else {
                "Deletes (commented; not executed)"
            };
            out.push_str(&format!("\n-- {title}\n"));
            for row in &self.deletes {
                let statement = build_delete_literal(driver_id, schema, table, columns, row)?;
                if !include_deletes {
                    out.push_str("-- ");
                }
                push_line(&mut out, &statement);
            }
        }
        Ok(out)
    }

    fn update_subset(&self, changed: &[usize], row: &[Value]) -> (Vec<ColumnInfo>, Vec<Value>) {
        (0..self.layout.key_len())
            .chain(changed.iter().copied())
            .map(|index| (self.layout.columns[index].clone(), row[index].clone()))
            .unzip()
    }
}

fn sample_value(driver_id: &str, value: &Value) -> String {
    let text = render_sql_literal(driver_id, value).unwrap_or_else(|_| format!("{value:?}"));
    if text.chars().count() <= SAMPLE_VALUE_CHARS {
        return text;
    }
    let shown: String = text.chars().take(SAMPLE_VALUE_CHARS).collect();
    format!("{shown}\u{2026}")
}

fn push_line(out: &mut String, statement: &str) {
    out.push_str(statement.trim_end_matches(';'));
    out.push_str(";\n");
}

pub fn plan_layout(
    source: &[ColumnInfo],
    target: &[ColumnInfo],
    key: Option<&[String]>,
) -> Result<Layout, DataCompareError> {
    let comparable = |column: &&ColumnInfo| {
        source.iter().any(|other| {
            other.name == column.name
                && other.data_type.eq_ignore_ascii_case(&column.data_type)
                && !other.is_generated
                && !column.is_generated
        })
    };
    let key = match key {
        Some(names) => names.to_vec(),
        None => common_primary_key(source, target)?,
    };
    if key.is_empty() {
        return Err(DataCompareError::NoKey);
    }
    let mut columns = Vec::new();
    for name in &key {
        let usable = target.iter().filter(comparable).find(|column| &column.name == name);
        let source_nullable = source.iter().any(|column| &column.name == name && column.nullable);
        match usable {
            Some(column)
                if !column.nullable && !source_nullable && !columns.iter().any(|c: &ColumnInfo| &c.name == name) =>
            {
                columns.push(column.clone())
            }
            _ => return Err(DataCompareError::BadKey(name.clone())),
        }
    }
    let identity = target.iter().any(|column| column.is_auto_increment);
    columns.extend(
        target
            .iter()
            .filter(comparable)
            .filter(|column| !key.contains(&column.name))
            .cloned(),
    );
    for column in &mut columns {
        column.primary_key = key.contains(&column.name);
        column.is_auto_increment = false;
    }
    let not_compared = source
        .iter()
        .chain(target)
        .map(|column| column.name.clone())
        .filter(|name| !columns.iter().any(|column| &column.name == name))
        .fold(Vec::new(), |mut names, name| {
            if !names.contains(&name) {
                names.push(name);
            }
            names
        });
    Ok(Layout {
        compared: columns[key.len()..].iter().map(|column| column.name.clone()).collect(),
        key,
        not_compared,
        columns,
        identity,
    })
}

fn common_primary_key(source: &[ColumnInfo], target: &[ColumnInfo]) -> Result<Vec<String>, DataCompareError> {
    let names = |columns: &[ColumnInfo]| {
        columns
            .iter()
            .filter(|column| column.primary_key)
            .map(|column| column.name.clone())
            .collect::<Vec<_>>()
    };
    let (left, right) = (names(source), names(target));
    let mut sorted = (left.clone(), right);
    sorted.0.sort();
    sorted.1.sort();
    if left.is_empty() || sorted.0 != sorted.1 {
        return Err(DataCompareError::NoKey);
    }
    Ok(left)
}

#[async_trait]
pub trait RowSource: Send {
    async fn next_page(&mut self) -> Result<Option<Vec<Vec<Value>>>, DataCompareError>;
}

struct Ordered<S> {
    source: S,
    side: Side,
    width: usize,
    key_len: usize,
    page: std::vec::IntoIter<Vec<Value>>,
    last: Option<Vec<Value>>,
    done: bool,
}

impl<S: RowSource> Ordered<S> {
    fn new(source: S, side: Side, layout: &Layout) -> Self {
        Self {
            source,
            side,
            width: layout.width(),
            key_len: layout.key_len(),
            page: Vec::new().into_iter(),
            last: None,
            done: false,
        }
    }

    async fn next(&mut self) -> Result<Option<Vec<Value>>, DataCompareError> {
        loop {
            if let Some(row) = self.page.next() {
                self.check(&row)?;
                return Ok(Some(row));
            }
            if self.done {
                return Ok(None);
            }
            match self.source.next_page().await? {
                Some(page) => self.page = page.into_iter(),
                None => self.done = true,
            }
        }
    }

    fn check(&mut self, row: &[Value]) -> Result<(), DataCompareError> {
        if row.len() != self.width {
            return Err(DataCompareError::Truncated(self.side));
        }
        let key = &row[..self.key_len];
        if key.iter().any(|value| matches!(value, Value::Null)) {
            return Err(DataCompareError::UncomparableKey(self.side));
        }
        if let Some(last) = &self.last {
            match compare_keys(last, key) {
                None => return Err(DataCompareError::UncomparableKey(self.side)),
                Some(Ordering::Equal) => return Err(DataCompareError::DuplicateKey(self.side)),
                Some(Ordering::Greater) => return Err(DataCompareError::KeyOrder(self.side)),
                Some(Ordering::Less) => {}
            }
        }
        self.last = Some(key.to_vec());
        Ok(())
    }
}

fn compare_keys(left: &[Value], right: &[Value]) -> Option<Ordering> {
    for (a, b) in left.iter().zip(right) {
        match a.compare_for_sort(b, true)? {
            Ordering::Equal => continue,
            order => return Some(order),
        }
    }
    Some(Ordering::Equal)
}

enum Cell {
    Same,
    Different,
    Unknown,
}

fn compare_cell(left: &Value, right: &Value) -> Cell {
    let same = match (left, right) {
        (Value::Undecodable(_), _) | (_, Value::Undecodable(_)) => return Cell::Unknown,
        (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
        _ => left == right,
    };
    if same { Cell::Same } else { Cell::Different }
}

pub async fn merge<S: RowSource, T: RowSource>(
    layout: Layout,
    source: S,
    target: T,
) -> Result<DataCompare, DataCompareError> {
    let mut source = Ordered::new(source, Side::Source, &layout);
    let mut target = Ordered::new(target, Side::Target, &layout);
    let mut result = DataCompare::new(layout);
    let (mut left, mut right) = (source.next().await?, target.next().await?);
    loop {
        let order = match (&left, &right) {
            (None, None) => return Ok(result),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => {
                let key_len = result.layout.key_len();
                compare_keys(&a[..key_len], &b[..key_len]).ok_or(DataCompareError::UncomparableKey(Side::Source))?
            }
        };
        match order {
            Ordering::Less => {
                if let Some(row) = left.take() {
                    result.record(Change::Insert, row, Vec::new());
                }
                left = source.next().await?;
            }
            Ordering::Greater => {
                if let Some(row) = right.take() {
                    result.record(Change::Delete, row, Vec::new());
                }
                right = target.next().await?;
            }
            Ordering::Equal => {
                if let (Some(a), Some(b)) = (left.take(), right.take()) {
                    classify_pair(&mut result, a, &b);
                }
                left = source.next().await?;
                right = target.next().await?;
            }
        }
    }
}

fn classify_pair(result: &mut DataCompare, source: Vec<Value>, target: &[Value]) {
    let mut changed = Vec::new();
    for index in result.layout.key_len()..result.layout.width() {
        match compare_cell(&source[index], &target[index]) {
            Cell::Same => {}
            Cell::Different => changed.push(index),
            Cell::Unknown => result.unknown_cells += 1,
        }
    }
    if changed.is_empty() {
        result.counts.same += 1;
        return;
    }
    result.record(Change::Update, source, changed);
}

pub struct TableSide<'a> {
    pub connection: &'a dyn Connection,
    pub schema: Option<&'a str>,
    pub table: &'a str,
}

#[derive(Clone, Copy)]
struct Reads<'a> {
    driver_id: &'a str,
    columns: &'a [ColumnInfo],
    key_len: usize,
    control: &'a OperationControl,
}

impl<'a> Reads<'a> {
    fn reader(self, side: TableSide<'a>, which: Side) -> KeysetReader<'a> {
        KeysetReader {
            reads: self,
            side,
            which,
            last: None,
            done: false,
        }
    }
}

struct KeysetReader<'a> {
    reads: Reads<'a>,
    side: TableSide<'a>,
    which: Side,
    last: Option<Vec<Value>>,
    done: bool,
}

#[async_trait]
impl RowSource for KeysetReader<'_> {
    async fn next_page(&mut self) -> Result<Option<Vec<Vec<Value>>>, DataCompareError> {
        if self.done {
            return Ok(None);
        }
        let reads = self.reads;
        check_pre_dispatch(reads.control)?;
        let (sql, params) = page_query(
            reads.driver_id,
            self.side.schema,
            self.side.table,
            reads.columns,
            reads.key_len,
            self.last.as_deref(),
        );
        let result = self
            .side
            .connection
            .query_params_controlled(&sql, &params, reads.control)
            .await?;
        if result.truncated {
            return Err(DataCompareError::Truncated(self.which));
        }
        let rows: Vec<Vec<Value>> = result.rows.iter().map(<[Value]>::to_vec).collect();
        self.done = rows.len() < PAGE_ROWS;
        if let Some(row) = rows.last() {
            self.last = Some(
                row.get(..reads.key_len)
                    .ok_or(DataCompareError::Truncated(self.which))?
                    .to_vec(),
            );
        }
        Ok(Some(rows))
    }
}

fn is_text(data_type: &str) -> bool {
    let lower = data_type.to_ascii_lowercase();
    ["char", "text", "clob", "string", "enum", "set(", "name"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn order_term(driver_id: &str, column: &ColumnInfo) -> String {
    let quoted = quote_ident(driver_id, &column.name);
    let text = is_text(&column.data_type);
    match driver_id {
        "postgres" if text => format!("{quoted} COLLATE \"C\""),
        "mysql" if text => format!("CAST({quoted} AS BINARY)"),
        "sqlite" => format!("{quoted} COLLATE BINARY"),
        "mssql" if text => format!("{quoted} COLLATE Latin1_General_BIN2"),
        _ => quoted,
    }
}

fn param_term(driver_id: &str, column: &ColumnInfo, index: usize) -> String {
    let placeholder = placeholder_for(driver_id, index);
    if Dialect::from_id(driver_id) == Some(Dialect::Mysql) && is_text(&column.data_type) {
        return format!("CAST({placeholder} AS BINARY)");
    }
    placeholder
}

pub fn page_query(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    key_len: usize,
    after: Option<&[Value]>,
) -> (String, Vec<Value>) {
    let select = columns
        .iter()
        .map(|column| quote_ident(driver_id, &column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!("SELECT {select} FROM {}", qualified_table(driver_id, schema, table));
    let mut params = Vec::new();
    if let Some(after) = after {
        let mut disjuncts = Vec::new();
        for depth in 0..key_len {
            let mut terms = Vec::new();
            for index in 0..=depth {
                let operator = if index < depth { "=" } else { ">" };
                let column = &columns[index];
                terms.push(format!(
                    "{} {operator} {}",
                    order_term(driver_id, column),
                    param_term(driver_id, column, params.len())
                ));
                params.push(after[index].clone());
            }
            disjuncts.push(format!("({})", terms.join(" AND ")));
        }
        sql.push_str(&format!(" WHERE {}", disjuncts.join(" OR ")));
    }
    let order = columns[..key_len]
        .iter()
        .map(|column| order_term(driver_id, column))
        .collect::<Vec<_>>()
        .join(", ");
    sql.push_str(&format!(" ORDER BY {order}"));
    if Dialect::from_id(driver_id) == Some(Dialect::Mssql) {
        sql.push_str(&format!(" OFFSET 0 ROWS FETCH NEXT {PAGE_ROWS} ROWS ONLY"));
    } else {
        sql.push_str(&format!(" LIMIT {PAGE_ROWS}"));
    }
    (sql, params)
}

pub async fn compare_tables(
    driver_id: &str,
    source: TableSide<'_>,
    target: TableSide<'_>,
    key: Option<&[String]>,
    control: &OperationControl,
) -> Result<DataCompare, DataCompareError> {
    if Dialect::from_id(driver_id).is_none() {
        return Err(DataCompareError::Unsupported);
    }
    let source_columns = source
        .connection
        .fetch_columns_controlled(source.schema, source.table, control)
        .await?;
    let target_columns = target
        .connection
        .fetch_columns_controlled(target.schema, target.table, control)
        .await?;
    let layout = plan_layout(&source_columns, &target_columns, key)?;
    let columns = layout.columns.clone();
    let reads = Reads {
        driver_id,
        columns: &columns,
        key_len: layout.key_len(),
        control,
    };
    let (left, right) = (reads.reader(source, Side::Source), reads.reader(target, Side::Target));
    merge(layout.clone(), left, right).await
}

#[cfg(test)]
#[path = "data_compare_tests.rs"]
mod tests;
