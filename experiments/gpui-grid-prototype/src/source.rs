use std::sync::{Arc, OnceLock};

use bookie_core::{QueryResult, QueryRows, Value};
use bookie_results::result_find::{FindError, find_matches};
use bookie_results::result_predicate::{ColumnScope, ResultPredicate};
use bookie_results::result_scan::{ScanProgress, ScanSource, scan_pages};
use bookie_results::stream_result_spool::StreamResultSpoolManager;
use bookie_results::text_match::TextMatchOptions;
use tokio_util::sync::CancellationToken;

use crate::core_bridge::{cell_to_value, column_infos, value_to_cell};
use crate::model::{Cell, Column, Diff, FindResult, Op, PAGE_ROWS, Predicate, build_diff, row_at};

pub trait ResultSource: Send + Sync + 'static {
    fn total(&self) -> u64;
    fn row(&self, index: u64) -> Vec<Cell>;
    fn find(&self, query: &str, cap: usize) -> Result<FindResult, &'static str>;
    fn filter(&self, predicate: &Predicate) -> Result<Vec<u32>, &'static str>;
    fn compare(&self, keyed: bool) -> Diff;
}

pub type Source = Arc<dyn ResultSource>;

pub const MAX_QUERY_BYTES: usize = 256;

pub struct Synthetic {
    total: u64,
    scan: OnceLock<Result<ScanSource, &'static str>>,
}

impl Synthetic {
    pub fn new(total: u64) -> Self {
        Self {
            total,
            scan: OnceLock::new(),
        }
    }

    fn scan(&self) -> Result<&ScanSource, &'static str> {
        self.scan
            .get_or_init(|| spool_rows(self.total).map_err(|_| "the result could not be prepared for scanning"))
            .as_ref()
            .map_err(|message| *message)
    }
}

fn spool_rows(total: u64) -> Result<ScanSource, Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let spool = StreamResultSpoolManager::from_environment().create()?;
    let columns = column_infos();
    let mut start = 0;
    while start < total {
        let end = (start + PAGE_ROWS).min(total);
        let rows: Vec<Vec<Value>> = (start..end)
            .map(|index| row_at(index).iter().map(cell_to_value).collect())
            .collect();
        let page = QueryResult {
            columns: columns.clone(),
            rows: QueryRows::from_rows(rows),
            truncated: false,
        };
        runtime.block_on(spool.append_page(start / PAGE_ROWS, Arc::new(page)))?;
        start = end;
    }
    Ok(ScanSource::new(spool, columns, total))
}

fn contains(column: Option<usize>, needle: &str) -> ResultPredicate {
    ResultPredicate::Contains {
        scope: column.map_or(ColumnScope::Any, ColumnScope::Column),
        needle: needle.to_string(),
        options: TextMatchOptions::default(),
    }
}

fn display_equals(value: Option<&Value>, normalized_text: &str) -> Option<bool> {
    let value = value?;
    if matches!(value, Value::Null) {
        return None;
    }
    if let Value::Text(value) | Value::JsonText(value) = value {
        return Some(value.to_lowercase() == normalized_text);
    }
    let matches = match value {
        Value::Bool(value) => value.to_string() == normalized_text,
        Value::Int(value) => value.to_string() == normalized_text,
        Value::Float(value) => value.to_string() == normalized_text,
        _ => value_to_cell(value.clone()).display().to_lowercase() == normalized_text,
    };
    Some(matches)
}

impl ResultSource for Synthetic {
    fn total(&self) -> u64 {
        self.total
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        row_at(index)
    }

    fn find(&self, query: &str, cap: usize) -> Result<FindResult, &'static str> {
        if cap == 0 || cap > bookie_results::result_find::MAX_FIND_MATCHES {
            return Err("match cap exceeds the prototype limit");
        }
        if query.is_empty() || query.len() > MAX_QUERY_BYTES {
            return Err("invalid search text");
        }
        let scan = self.scan()?;
        let report = find_matches(
            scan,
            contains(None, query),
            &CancellationToken::new(),
            &ScanProgress::default(),
        )
        .map_err(find_message)?;
        let global = |page: usize, row: usize| page as u64 * PAGE_ROWS + row as u64;
        let mut matches: Vec<u64> = report.matches.iter().map(|hit| global(hit.page, hit.row)).collect();
        if matches.len() <= cap {
            return Ok(FindResult {
                matches,
                capped: report.capped,
                scanned: report.rows_scanned,
            });
        }
        let scanned = matches[cap] + 1;
        matches.truncate(cap);
        Ok(FindResult {
            matches,
            capped: true,
            scanned,
        })
    }

    fn filter(&self, predicate: &Predicate) -> Result<Vec<u32>, &'static str> {
        let scan = self.scan()?;
        let column = Column::ALL
            .iter()
            .position(|column| *column == predicate.column)
            .unwrap_or(0);
        let normalized_text = predicate.text.to_lowercase();
        let prepared = if predicate.op == Op::Contains {
            Some(
                contains(Some(column), &predicate.text)
                    .prepare(scan.columns().len())
                    .map_err(|_| "this filter cannot be applied exactly")?,
            )
        } else {
            None
        };
        let mut kept = Vec::new();
        let mut tally = bookie_results::result_predicate::CellTally::default();
        scan_pages(
            scan,
            &CancellationToken::new(),
            &ScanProgress::default(),
            |index, page| {
                for (row, cells) in page.rows.iter().enumerate() {
                    let matches = match predicate.op {
                        Op::Contains => prepared
                            .as_ref()
                            .is_some_and(|prepared| prepared.matches(cells, &mut tally)),
                        Op::Equals => display_equals(cells.get(column), &normalized_text).unwrap_or(false),
                        Op::NotEquals => {
                            display_equals(cells.get(column), &normalized_text).is_none_or(|matches| !matches)
                        }
                    };
                    if matches {
                        kept.push((index as u64 * PAGE_ROWS + row as u64) as u32);
                    }
                }
                Ok(std::ops::ControlFlow::Continue(()))
            },
        )
        .map_err(|_| "the scan did not finish")?;
        Ok(kept)
    }

    fn compare(&self, keyed: bool) -> Diff {
        build_diff(self.total, keyed)
    }
}

fn find_message(error: FindError) -> &'static str {
    match error {
        FindError::Predicate(_) => "invalid search text",
        FindError::Scan(_) => "the scan did not finish",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_synthetic_source_answers_through_the_results_crate() {
        let source: Source = Arc::new(Synthetic::new(1_000));
        assert_eq!(source.total(), 1_000);
        assert_eq!(source.row(7), row_at(7));
        assert_eq!(source.find("sample 999", 10).map(|found| found.matches), Ok(vec![999]));
        assert!(!source.compare(false).summary.keyed);
    }

    #[test]
    fn display_equality_keeps_unicode_case_insensitive_matching() {
        let value = Value::Text("Ä".into());
        assert_eq!(display_equals(Some(&value), &"ä".to_lowercase()), Some(true));
    }
}
