use bookie_core::{ColumnInfo, Value};
use rust_decimal::Decimal;

#[derive(Debug, PartialEq)]
pub struct ColumnStats {
    pub column: String,
    pub count: u32,
    pub minimum: String,
    pub maximum: String,
    pub sum: String,
    pub average: String,
}

#[derive(Clone, Copy)]
enum Total {
    Exact {
        sum: Decimal,
        count: u32,
        minimum: Decimal,
        maximum: Decimal,
    },
    Float {
        sum: f64,
        count: u32,
        minimum: f64,
        maximum: f64,
    },
}

fn add(total: Option<Total>, value: &Value) -> Option<Option<Total>> {
    match (total, value) {
        (total, Value::Null) => Some(total),
        (None, Value::Int(n)) => Some(Some(exact(Decimal::from(*n)))),
        (None, Value::Decimal(d)) => Some(Some(exact(*d))),
        (None, Value::Float(f)) if f.is_finite() => Some(Some(float(*f))),
        (
            Some(Total::Exact {
                sum,
                count,
                minimum,
                maximum,
            }),
            Value::Int(i),
        ) => exact_add(sum, count, minimum, maximum, Decimal::from(*i)),
        (
            Some(Total::Exact {
                sum,
                count,
                minimum,
                maximum,
            }),
            Value::Decimal(d),
        ) => exact_add(sum, count, minimum, maximum, *d),
        (
            Some(Total::Float {
                sum,
                count,
                minimum,
                maximum,
            }),
            Value::Float(f),
        ) if f.is_finite() => {
            let next_sum = sum + f;
            next_sum.is_finite().then_some(Some(Total::Float {
                sum: next_sum,
                count: count.checked_add(1)?,
                minimum: minimum.min(*f),
                maximum: maximum.max(*f),
            }))
        }
        _ => None,
    }
}

fn exact(value: Decimal) -> Total {
    Total::Exact {
        sum: value,
        count: 1,
        minimum: value,
        maximum: value,
    }
}

fn float(value: f64) -> Total {
    Total::Float {
        sum: value,
        count: 1,
        minimum: value,
        maximum: value,
    }
}

fn exact_add(sum: Decimal, count: u32, minimum: Decimal, maximum: Decimal, value: Decimal) -> Option<Option<Total>> {
    Some(Some(Total::Exact {
        sum: sum.checked_add(value)?,
        count: count.checked_add(1)?,
        minimum: minimum.min(value),
        maximum: maximum.max(value),
    }))
}

fn describe(total: Total) -> Option<(String, String)> {
    match total {
        Total::Exact { sum, count, .. } => Some((
            sum.normalize().to_string(),
            (sum / Decimal::from(count)).round_dp(6).normalize().to_string(),
        )),
        Total::Float { sum, count, .. } if sum.is_finite() => {
            Some((sum.to_string(), (sum / f64::from(count)).to_string()))
        }
        Total::Float { .. } => None,
    }
}

#[derive(Default)]
pub struct StatsAccumulator {
    total: Option<Total>,
    unusable: bool,
}

impl StatsAccumulator {
    pub fn push(&mut self, value: &Value) {
        if self.unusable {
            return;
        }
        match add(self.total, value) {
            Some(total) => self.total = total,
            None => self.unusable = true,
        }
    }

    pub fn unreadable(&mut self) {
        self.unusable = true;
    }

    pub fn finish(self, column: &str) -> Option<ColumnStats> {
        if self.unusable {
            return None;
        }
        let total = self.total?;
        let (sum, average) = describe(total)?;
        let (count, minimum, maximum) = match total {
            Total::Exact {
                count,
                minimum,
                maximum,
                ..
            } => (count, minimum.normalize().to_string(), maximum.normalize().to_string()),
            Total::Float {
                count,
                minimum,
                maximum,
                ..
            } => (count, minimum.to_string(), maximum.to_string()),
        };
        Some(ColumnStats {
            column: column.to_string(),
            count,
            minimum,
            maximum,
            sum,
            average,
        })
    }
}

pub fn column_stats(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> Vec<ColumnStats> {
    column_stats_where(columns, rows, |_| true)
}

pub fn column_stats_in(columns: &[ColumnInfo], rows: &[Vec<Value>], only: &[usize]) -> Vec<ColumnStats> {
    column_stats_where(columns, rows, |index| only.contains(&index))
}

fn column_stats_where(columns: &[ColumnInfo], rows: &[Vec<Value>], wanted: impl Fn(usize) -> bool) -> Vec<ColumnStats> {
    columns
        .iter()
        .enumerate()
        .filter(|(index, _)| wanted(*index))
        .filter_map(|(index, column)| {
            let mut accumulator = StatsAccumulator::default();
            for row in rows {
                match row.get(index) {
                    Some(value) => accumulator.push(value),
                    None => accumulator.unreadable(),
                }
            }
            accumulator.finish(&column.name)
        })
        .collect()
}

pub fn tooltip(stats: &[ColumnStats]) -> String {
    stats
        .iter()
        .map(|s| {
            crate::tr!("{column}: count {count}, min {minimum}, max {maximum}, sum {sum}, average {average}")
                .replace("{column}", &s.column)
                .replace("{count}", &s.count.to_string())
                .replace("{minimum}", &s.minimum)
                .replace("{maximum}", &s.maximum)
                .replace("{sum}", &s.sum)
                .replace("{average}", &s.average)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "selection_stats_tests.rs"]
mod tests;
