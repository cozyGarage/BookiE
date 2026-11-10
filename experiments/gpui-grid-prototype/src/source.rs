use std::sync::{Arc, Mutex};

use bookie_core::{QueryResult, QueryRows, Value};
use bookie_results::result_find::{FindError, find_matches};
use bookie_results::result_predicate::{ColumnScope, ResultPredicate};
use bookie_results::result_scan::{ScanProgress, ScanSource, scan_pages};
use bookie_results::stream_result_spool::StreamResultSpoolManager;
use bookie_results::text_match::TextMatchOptions;
use tokio_util::sync::CancellationToken;

use crate::core_bridge::{cell_to_value, column_infos, value_to_cell};
use crate::model::{
    Cell, ColumnMeta, Diff, FindResult, Op, PAGE_ROWS, Predicate, build_diff, row_at, synthetic_columns,
};

pub trait ResultSource: Send + Sync + 'static {
    fn columns(&self) -> &[ColumnMeta];
    fn editable(&self) -> bool {
        false
    }
    fn total(&self) -> u64;
    fn row(&self, index: u64) -> Vec<Cell>;
    fn find(&self, query: &str, cap: usize, cancel: &CancellationToken) -> Result<FindResult, &'static str>;
    fn filter(&self, predicate: &Predicate, cancel: &CancellationToken) -> Result<Vec<u32>, &'static str>;
    fn compare(&self, keyed: bool, cancel: &CancellationToken) -> Result<Diff, &'static str>;
}

pub type Source = Arc<dyn ResultSource>;

pub const MAX_QUERY_BYTES: usize = 256;

pub struct Synthetic {
    total: u64,
    columns: Vec<ColumnMeta>,
    scan: Mutex<Option<ScanSource>>,
}

impl Synthetic {
    pub fn new(total: u64) -> Self {
        Self {
            total,
            columns: synthetic_columns(),
            scan: Mutex::new(None),
        }
    }

    fn scan(&self, cancel: &CancellationToken) -> Result<ScanSource, &'static str> {
        if cancel.is_cancelled() {
            return Err("the operation was cancelled");
        }
        let mut scan = self
            .scan
            .lock()
            .map_err(|_| "the result could not be prepared for scanning")?;
        if let Some(scan) = scan.as_ref() {
            return Ok(scan.clone());
        }
        let created = spool_rows(self.total, cancel)?;
        *scan = Some(created.clone());
        Ok(created)
    }
}

fn spool_rows(total: u64, cancel: &CancellationToken) -> Result<ScanSource, &'static str> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|_| "the result could not be prepared for scanning")?;
    let spool = StreamResultSpoolManager::from_environment()
        .create()
        .map_err(|_| "the result could not be prepared for scanning")?;
    let columns = column_infos();
    let mut start = 0;
    while start < total {
        if cancel.is_cancelled() {
            return Err("the operation was cancelled");
        }
        let end = (start + PAGE_ROWS).min(total);
        let rows: Vec<Vec<Value>> = (start..end)
            .map(|index| row_at(index).iter().map(cell_to_value).collect())
            .collect();
        let page = QueryResult {
            columns: columns.clone(),
            rows: QueryRows::from_rows(rows),
            truncated: false,
        };
        runtime
            .block_on(spool.append_page(start / PAGE_ROWS, Arc::new(page)))
            .map_err(|_| "the result could not be prepared for scanning")?;
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
    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }

    fn editable(&self) -> bool {
        true
    }

    fn total(&self) -> u64 {
        self.total
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        row_at(index)
    }

    fn find(&self, query: &str, cap: usize, cancel: &CancellationToken) -> Result<FindResult, &'static str> {
        find_in(&self.scan(cancel)?, query, cap, cancel)
    }

    fn filter(&self, predicate: &Predicate, cancel: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        filter_in(&self.scan(cancel)?, predicate, cancel)
    }

    fn compare(&self, keyed: bool, cancel: &CancellationToken) -> Result<Diff, &'static str> {
        build_diff(self.total, keyed, cancel)
    }
}

pub fn find_in(
    scan: &ScanSource,
    query: &str,
    cap: usize,
    cancel: &CancellationToken,
) -> Result<FindResult, &'static str> {
    if cap == 0 || cap > bookie_results::result_find::MAX_FIND_MATCHES {
        return Err("match cap exceeds the prototype limit");
    }
    if query.is_empty() || query.len() > MAX_QUERY_BYTES {
        return Err("invalid search text");
    }
    let report = find_matches(scan, contains(None, query), cancel, &ScanProgress::default()).map_err(find_message)?;
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

pub fn filter_in(
    scan: &ScanSource,
    predicate: &Predicate,
    cancel: &CancellationToken,
) -> Result<Vec<u32>, &'static str> {
    let column = predicate.column;
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
    scan_pages(scan, cancel, &ScanProgress::default(), |index, page| {
        for (row, cells) in page.rows.iter().enumerate() {
            let matches = match predicate.op {
                Op::Contains => prepared
                    .as_ref()
                    .is_some_and(|prepared| prepared.matches(cells, &mut tally)),
                Op::Equals => display_equals(cells.get(column), &normalized_text).unwrap_or(false),
                Op::NotEquals => display_equals(cells.get(column), &normalized_text).is_none_or(|matches| !matches),
            };
            if matches {
                kept.push((index as u64 * PAGE_ROWS + row as u64) as u32);
            }
        }
        Ok(std::ops::ControlFlow::Continue(()))
    })
    .map_err(|error| match error {
        bookie_results::result_scan::ScanError::Cancelled => "filter cancelled",
        _ => "the scan did not finish",
    })?;
    Ok(kept)
}

fn find_message(error: FindError) -> &'static str {
    match error {
        FindError::Predicate(_) => "invalid search text",
        FindError::Scan(bookie_results::result_scan::ScanError::Cancelled) => "search cancelled",
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
        assert_eq!(
            source
                .find("sample 999", 10, &CancellationToken::new())
                .map(|found| found.matches),
            Ok(vec![999])
        );
        assert!(!source.compare(false, &CancellationToken::new()).unwrap().summary.keyed);
    }

    #[test]
    fn display_equality_keeps_unicode_case_insensitive_matching() {
        let value = Value::Text("Ä".into());
        assert_eq!(display_equals(Some(&value), &"ä".to_lowercase()), Some(true));
    }
}
