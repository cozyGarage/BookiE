use std::collections::{HashMap, HashSet};

use bookie_core::{ColumnInfo, Value};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use crate::value_display::copy_text;

pub const MAX_ROWS: usize = 10_000;
pub const MAX_BARS: usize = 200;
pub const MAX_POINTS: usize = 2_000;
pub const SLOTS: usize = 5;
pub const PROFILE_ROWS: usize = 500;
const FEW_LABELS: usize = 60;
const SHORT_TEXT: f32 = 30.;
const LONG_TEXT: f32 = 60.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Bars,
    Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Aggregate {
    Sum,
    Count,
    Average,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Settings {
    pub kind: Kind,
    pub label: usize,
    pub values: [Option<usize>; SLOTS],
    pub aggregate: Aggregate,
}

impl Settings {
    pub fn picked(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.values
            .iter()
            .enumerate()
            .filter_map(|(slot, column)| column.map(|column| (slot, column)))
    }

    pub fn slot_of(&self, column: usize) -> Option<usize> {
        self.values.iter().position(|value| *value == Some(column))
    }

    pub fn toggle_value(&mut self, column: usize) -> bool {
        if let Some(slot) = self.slot_of(column) {
            self.values[slot] = None;
            return true;
        }
        match self.values.iter().position(Option::is_none) {
            Some(slot) => {
                self.values[slot] = Some(column);
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub numeric: bool,
    pub id_like: bool,
    pub temporal: bool,
    pub filled: usize,
    pub distinct: usize,
    pub avg_len: f32,
}

pub fn profile(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> Vec<Profile> {
    let rows = &rows[..rows.len().min(PROFILE_ROWS)];
    columns
        .iter()
        .enumerate()
        .map(|(column, info)| {
            let (mut filled, mut numbers, mut dates, mut chars) = (0, 0, 0, 0);
            let mut seen = HashSet::new();
            for value in rows.iter().filter_map(|row| row.get(column)) {
                let Some(text) = label_text(value) else { continue };
                filled += 1;
                chars += text.chars().count();
                numbers += usize::from(number(value).is_some());
                dates += usize::from(temporal_value(value, &text));
                seen.insert(text);
            }
            let numeric = filled > 0 && numbers == filled;
            Profile {
                numeric,
                id_like: id_like(info),
                temporal: !numeric && (temporal_type(&info.data_type) || (filled > 0 && dates == filled)),
                filled,
                distinct: seen.len(),
                avg_len: if filled == 0 { 0. } else { chars as f32 / filled as f32 },
            }
        })
        .collect()
}

pub fn defaults(profiles: &[Profile]) -> Settings {
    let mut value = (0..profiles.len())
        .rev()
        .find(|&c| profiles[c].numeric && !profiles[c].id_like);
    let label = (0..profiles.len())
        .filter(|&c| Some(c) != value)
        .min_by_key(|&c| (label_rank(&profiles[c]), c));
    let label = match label {
        Some(label) => label,
        None => value.take().unwrap_or(0),
    };
    let mut values = [None; SLOTS];
    values[0] = value;
    Settings {
        kind: if profiles.get(label).is_some_and(|p| p.temporal) {
            Kind::Line
        } else {
            Kind::Bars
        },
        label,
        values,
        aggregate: if value.is_some() { Aggregate::Sum } else { Aggregate::Count },
    }
}

fn label_rank(p: &Profile) -> u8 {
    let text = !p.numeric && !p.temporal && !p.id_like && p.filled > 0;
    match () {
        _ if text && p.distinct <= FEW_LABELS && p.avg_len <= SHORT_TEXT => 0,
        _ if p.temporal && !p.id_like => 1,
        _ if text && p.avg_len <= LONG_TEXT => 2,
        _ if p.numeric && !p.id_like => 3,
        _ => 4,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cut {
    Top,
    First,
}

#[derive(Debug, Default, PartialEq)]
pub struct ChartData {
    pub labels: Vec<Option<String>>,
    pub series: Vec<Series>,
    pub rows: usize,
    pub rows_cut: bool,
    pub groups: usize,
    pub cut: Option<Cut>,
}

impl ChartData {
    pub fn combines(&self) -> bool {
        self.groups < self.rows
    }

    pub fn range(&self, kind: Kind) -> Option<(f64, f64)> {
        let values = self.series.iter().flat_map(|s| s.values.iter().flatten().copied());
        let (lo, hi) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
        if lo > hi {
            return None;
        }
        Some(match kind {
            Kind::Bars => (lo.min(0.), hi.max(0.)),
            Kind::Line => (lo, hi),
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct Series {
    pub slot: usize,
    pub column: Option<usize>,
    pub values: Vec<Option<f64>>,
}

#[derive(Clone, Copy)]
enum Sum {
    Exact(Decimal),
    Float(f64),
}

impl Sum {
    fn add(self, value: &Value, number: f64) -> Self {
        let exact = match value {
            Value::Int(n) => Some(Decimal::from(*n)),
            Value::Decimal(d) => Some(*d),
            _ => None,
        };
        match (self, exact) {
            (Self::Exact(sum), Some(exact)) => sum
                .checked_add(exact)
                .map_or_else(|| Self::Float(decimal_f64(sum) + number), Self::Exact),
            (sum, _) => Self::Float(sum.as_f64() + number),
        }
    }

    fn as_f64(self) -> f64 {
        match self {
            Self::Exact(sum) => decimal_f64(sum),
            Self::Float(sum) => sum,
        }
    }

    fn average(self, count: usize) -> f64 {
        match self {
            Self::Exact(sum) => sum
                .checked_div(Decimal::from(count))
                .map_or(f64::NAN, decimal_f64),
            Self::Float(sum) => sum / count as f64,
        }
    }
}

fn decimal_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(f64::NAN)
}

type Totals = Vec<Vec<(Sum, usize)>>;

struct Groups {
    labels: Vec<Option<String>>,
    totals: Totals,
}

fn group_rows(rows: &[Vec<Value>], settings: &Settings, picked: &[(usize, Option<usize>)], profiles: &[Profile]) -> Groups {
    let numeric = |column: usize| profiles.get(column).is_some_and(|p| p.numeric);
    let mut index: HashMap<Option<String>, usize> = HashMap::new();
    let mut labels = Vec::new();
    let mut totals: Totals = vec![Vec::new(); picked.len()];
    for row in rows {
        let label = row.get(settings.label).and_then(label_text);
        let group = *index.entry(label.clone()).or_insert_with(|| {
            labels.push(label);
            totals
                .iter_mut()
                .for_each(|series| series.push((Sum::Exact(Decimal::ZERO), 0)));
            labels.len() - 1
        });
        for (series, (_, column)) in totals.iter_mut().zip(picked) {
            let total = &mut series[group];
            let Some(column) = column else {
                total.1 += 1;
                continue;
            };
            let Some(value) = row.get(*column).filter(|_| numeric(*column)) else { continue };
            if let Some(n) = number(value) {
                *total = (total.0.add(value, n), total.1 + 1);
            }
        }
    }
    Groups { labels, totals }
}

pub fn compute(rows: &[Vec<Value>], profiles: &[Profile], settings: &Settings) -> ChartData {
    let read = &rows[..rows.len().min(MAX_ROWS)];
    let mut picked: Vec<(usize, Option<usize>)> = settings.picked().map(|(slot, c)| (slot, Some(c))).collect();
    if picked.is_empty() && settings.aggregate == Aggregate::Count {
        picked.push((0, None));
    }
    let Groups { labels, totals } = group_rows(read, settings, &picked, profiles);
    let aggregate = if labels.len() < read.len() {
        settings.aggregate
    } else {
        Aggregate::Sum
    };
    let value = |(sum, count): (Sum, usize)| match aggregate {
        Aggregate::Count => Some(count as f64),
        _ if count == 0 => None,
        Aggregate::Sum => Some(sum.as_f64()).filter(|v| v.is_finite()),
        Aggregate::Average => Some(sum.average(count)).filter(|v| v.is_finite()),
    };
    let totals: Vec<Vec<Option<f64>>> = totals
        .into_iter()
        .zip(&picked)
        .map(|(series, (_, column))| match column {
            None => series.into_iter().map(|(_, count)| Some(count as f64)).collect(),
            Some(_) => series.into_iter().map(value).collect(),
        })
        .collect();
    let temporal = profiles.get(settings.label).is_some_and(|p| p.temporal);
    let (order, cut) = keep(&labels, &totals, temporal, settings.kind);
    ChartData {
        labels: order.iter().map(|&g| labels[g].clone()).collect(),
        series: picked
            .iter()
            .zip(&totals)
            .map(|(&(slot, column), values)| Series {
                slot,
                column,
                values: order.iter().map(|&g| values[g]).collect(),
            })
            .collect(),
        rows: read.len(),
        rows_cut: rows.len() > MAX_ROWS,
        groups: labels.len(),
        cut,
    }
}

fn keep(labels: &[Option<String>], totals: &[Vec<Option<f64>>], temporal: bool, kind: Kind) -> (Vec<usize>, Option<Cut>) {
    let mut order: Vec<usize> = (0..labels.len()).collect();
    if temporal {
        order.sort_by(|&a, &b| (labels[a].is_none(), &labels[a]).cmp(&(labels[b].is_none(), &labels[b])));
    }
    let limit = match kind {
        Kind::Bars => MAX_BARS,
        Kind::Line => MAX_POINTS,
    };
    if order.len() <= limit {
        return (order, None);
    }
    match totals.first().filter(|_| kind == Kind::Bars && !temporal) {
        Some(first) => {
            let mut biggest = order.clone();
            biggest.sort_by(|&a, &b| {
                let (a, b) = (first[a].unwrap_or(f64::NEG_INFINITY), first[b].unwrap_or(f64::NEG_INFINITY));
                b.total_cmp(&a)
            });
            let kept: HashSet<usize> = biggest[..limit].iter().copied().collect();
            order.retain(|group| kept.contains(group));
            (order, Some(Cut::Top))
        }
        None => {
            order.truncate(limit);
            (order, Some(Cut::First))
        }
    }
}

pub fn nice_ticks(lo: f64, hi: f64, count: usize) -> Vec<f64> {
    let (mut lo, mut hi) = (lo.min(hi), lo.max(hi));
    if !lo.is_finite() || !hi.is_finite() {
        return vec![0., 1.];
    }
    if lo == hi {
        match lo {
            0. => hi = 1.,
            v if v > 0. => (lo, hi) = (v * 0.9, v * 1.1),
            v => (lo, hi) = (v * 1.1, v * 0.9),
        }
    }
    let step = nice_step((hi - lo) / count.saturating_sub(1).max(1) as f64);
    let start = (lo / step).floor() * step;
    let steps = ((hi - start) / step - 1e-9).ceil().max(1.) as usize;
    (0..=steps)
        .map(|i| start + i as f64 * step)
        .map(|v| if v.abs() < step * 1e-9 { 0. } else { v })
        .collect()
}

fn nice_step(raw: f64) -> f64 {
    let power = 10f64.powf(raw.log10().floor());
    let nice = match raw / power {
        f if f <= 1. => 1.,
        f if f <= 2. => 2.,
        f if f <= 2.5 => 2.5,
        f if f <= 5. => 5.,
        _ => 10.,
    };
    nice * power
}

pub fn tick_format(max_abs: f64, step: f64) -> (f64, &'static str, usize) {
    let (unit, suffix) = match max_abs {
        m if m >= 1e9 => (1e9, "B"),
        m if m >= 1e6 => (1e6, "M"),
        m if m >= 1e4 => (1e3, "k"),
        _ => (1., ""),
    };
    let scaled = step / unit;
    let decimals = (0..6)
        .find(|&d| {
            let shifted = scaled * 10f64.powi(d as i32);
            (shifted - shifted.round()).abs() < 1e-6 * shifted.abs().max(1.)
        })
        .unwrap_or(6);
    (unit, suffix, decimals)
}

pub fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        Value::Decimal(d) => d.to_f64(),
        _ => None,
    }
    .filter(|n| n.is_finite())
}

pub fn label_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        value => Some(copy_text(value).into_string()),
    }
}

fn temporal_type(type_name: &str) -> bool {
    let name = type_name.to_ascii_lowercase();
    name.contains("date") || name.contains("time")
}

fn temporal_value(value: &Value, text: &str) -> bool {
    match value {
        Value::Date(_) | Value::DateTime(_) | Value::TimestampTz(_) => true,
        Value::Text(_) => looks_like_date(text),
        _ => false,
    }
}

fn id_like(column: &ColumnInfo) -> bool {
    let (name, kind) = (&column.name, column.data_type.to_ascii_lowercase());
    let lower = name.to_ascii_lowercase();
    column.primary_key
        || matches!(lower.as_str(), "id" | "uuid" | "guid" | "oid" | "rowid")
        || ["_id", "_uuid", "_guid"].iter().any(|end| lower.ends_with(end))
        || (name.len() > 2 && name.ends_with("Id"))
        || kind.contains("uuid")
        || matches!(kind.as_str(), "uniqueidentifier" | "oid")
}

fn looks_like_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
#[path = "result_chart_tests.rs"]
mod tests;
