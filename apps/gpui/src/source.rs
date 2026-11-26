use std::sync::{Arc, Mutex};

use bookie_core::{FilterValueKind, QueryResult, QueryRows, Value, filter_value_kind};
use bookie_results::result_compare::{CompareError, CompareInput, CompareReport, CompareRequest, compare_results};
use bookie_results::result_find::{FindError, find_matches};
use bookie_results::result_predicate::{CellTally, ColumnScope, ResultPredicate};
use bookie_results::result_scan::{ScanProgress, ScanSource, scan_pages};
use bookie_results::stream_result_spool::StreamResultSpoolManager;
use bookie_results::text_match::TextMatchOptions;
use std::sync::OnceLock;
use tokio_util::sync::CancellationToken;

use crate::core_bridge::column_infos;
use crate::model::{
    Cell, CellText, ColumnMeta, Diff, DiffRow, DiffSummary, FindResult, MAX_DIFF_ROWS, Op, PAGE_ROWS, Predicate,
    right_row, row_at, synthetic_columns,
};

pub trait ResultSource: Send + Sync + 'static {
    fn columns(&self) -> &[ColumnMeta];
    fn editable(&self) -> bool {
        false
    }
    fn origin(&self) -> String {
        "Synthetic data · no database connection".into()
    }
    fn total(&self) -> u64;
    fn estimated_retained_bytes(&self) -> usize {
        0
    }
    fn executed_sql(&self) -> Option<&str> {
        None
    }
    fn load_elapsed(&self) -> Option<std::time::Duration> {
        None
    }
    fn is_partial(&self) -> bool {
        false
    }
    fn snapshot_consistent(&self) -> bool {
        false
    }
    fn row(&self, index: u64) -> Vec<Cell>;
    fn row_ref(&self, _index: u64) -> Option<&[Cell]> {
        None
    }
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
            return Err("canceled the operation");
        }
        let mut scan = self
            .scan
            .lock()
            .map_err(|_| "the result could not be prepared for scanning")?;
        if let Some(scan) = scan.as_ref() {
            return Ok(scan.clone());
        }
        let created = spool_rows_from((0..self.total).map(row_at), cancel)?;
        *scan = Some(created.clone());
        Ok(created)
    }
}

pub(crate) fn spool_manager() -> StreamResultSpoolManager {
    static MANAGER: OnceLock<StreamResultSpoolManager> = OnceLock::new();
    MANAGER.get_or_init(StreamResultSpoolManager::from_environment).clone()
}

fn spool_rows_from(
    rows: impl Iterator<Item = Vec<Value>>,
    cancel: &CancellationToken,
) -> Result<ScanSource, &'static str> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|_| "the result could not be prepared for scanning")?;
    let spool = spool_manager()
        .create()
        .map_err(|_| "the result could not be prepared for scanning")?;
    let columns = column_infos();
    let mut page: Vec<Vec<Value>> = Vec::new();
    let mut page_index = 0_u64;
    let mut count = 0_u64;
    let flush = |page: &mut Vec<Vec<Value>>, page_index: &mut u64| -> Result<(), &'static str> {
        let rows = QueryResult {
            columns: columns.clone(),
            rows: QueryRows::from_rows(std::mem::take(page)),
            truncated: false,
        };
        runtime
            .block_on(spool.append_page(*page_index, Arc::new(rows)))
            .map_err(|_| "the result could not be prepared for scanning")?;
        *page_index += 1;
        Ok(())
    };
    for row in rows {
        if cancel.is_cancelled() {
            return Err("canceled the operation");
        }
        page.push(row);
        count += 1;
        if page.len() as u64 == PAGE_ROWS {
            flush(&mut page, &mut page_index)?;
        }
    }
    if !page.is_empty() {
        flush(&mut page, &mut page_index)?;
    }
    Ok(ScanSource::new(spool, columns, count))
}

enum Equality {
    Typed(bookie_results::result_predicate::PreparedPredicate),
    Displayed { column: usize, text: String, negate: bool },
}

fn equality(
    column_count: usize,
    kind: Option<FilterValueKind>,
    predicate: &Predicate,
) -> Result<Equality, &'static str> {
    let negate = predicate.op == Op::NotEquals;
    let typed = match kind {
        None | Some(FilterValueKind::Text) => Err(()),
        Some(kind) => {
            crate::core_bridge::parse_edit(crate::core_bridge::edit_kind(kind), &predicate.text).map_err(|_| ())
        }
    };
    match typed {
        Ok(value) => {
            let result = if negate {
                ResultPredicate::NotEquals {
                    column: predicate.column,
                    value,
                }
            } else {
                ResultPredicate::Equals {
                    column: predicate.column,
                    value,
                }
            };
            result
                .prepare(column_count)
                .map(Equality::Typed)
                .map_err(|_| "this filter cannot be applied exactly")
        }
        Err(_) => Ok(Equality::Displayed {
            column: predicate.column,
            text: predicate.text.clone(),
            negate,
        }),
    }
}

fn displayed_equals(cells: &[Value], column: usize, text: &str, negate: bool) -> bool {
    let equal = match cells.get(column) {
        Some(Value::Null) | None => false,
        Some(cell) => cell.display() == text,
    };
    equal != negate
}

fn contains(column: Option<usize>, needle: &str) -> ResultPredicate {
    ResultPredicate::Contains {
        scope: column.map_or(ColumnScope::Any, ColumnScope::Column),
        needle: needle.to_string(),
        options: TextMatchOptions::default(),
    }
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
        if cancel.is_cancelled() {
            return Err("canceled the comparison");
        }
        let left = self.scan(cancel)?;
        let right = spool_rows_from((0..self.total).filter_map(right_row), cancel)?;
        let request = CompareRequest {
            left: CompareInput::Spool(left),
            right: CompareInput::Spool(right),
            key_columns: if keyed { vec!["value".into()] } else { Vec::new() },
        };
        let report = compare_results(&request, cancel, &ScanProgress::default()).map_err(compare_message)?;
        Ok(diff_view(&report, keyed))
    }
}

fn validate_find(query: &str, cap: usize) -> Result<(), &'static str> {
    if cap == 0 || cap > bookie_results::result_find::MAX_FIND_MATCHES {
        return Err("match cap exceeds the prototype limit");
    }
    if query.is_empty() || query.len() > MAX_QUERY_BYTES {
        return Err("invalid search text");
    }
    Ok(())
}

pub(crate) fn find_buffered_rows(
    rows: &QueryRows,
    column_count: usize,
    query: &str,
    cap: usize,
    cancel: &CancellationToken,
) -> Result<FindResult, &'static str> {
    validate_find(query, cap)?;
    let predicate = contains(None, query)
        .prepare(column_count)
        .map_err(|error| find_message(error.into()))?;
    let mut tally = CellTally::default();
    let mut matches = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err("canceled the search");
        }
        if !predicate.matches(row, &mut tally) {
            continue;
        }
        if matches.len() == cap {
            return Ok(FindResult {
                matches,
                capped: true,
                scanned: index as u64 + 1,
            });
        }
        matches.push(index as u64);
    }
    if cancel.is_cancelled() {
        return Err("canceled the search");
    }
    Ok(FindResult {
        matches,
        capped: false,
        scanned: rows.len() as u64,
    })
}

pub fn find_in(
    scan: &ScanSource,
    query: &str,
    cap: usize,
    cancel: &CancellationToken,
) -> Result<FindResult, &'static str> {
    validate_find(query, cap)?;
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

fn prepare_filter(
    column_count: usize,
    kind: Option<FilterValueKind>,
    predicate: &Predicate,
) -> Result<impl Fn(&[Value], &mut CellTally) -> bool, &'static str> {
    let (contains, equality) = match predicate.op {
        Op::Contains => (
            Some(
                contains(Some(predicate.column), &predicate.text)
                    .prepare(column_count)
                    .map_err(|_| "this filter cannot be applied exactly")?,
            ),
            None,
        ),
        Op::Equals | Op::NotEquals => (None, Some(equality(column_count, kind, predicate)?)),
    };
    Ok(move |cells: &[Value], tally: &mut CellTally| match &equality {
        Some(Equality::Typed(prepared)) => prepared.matches(cells, tally),
        Some(Equality::Displayed { column, text, negate }) => displayed_equals(cells, *column, text, *negate),
        None => contains.as_ref().is_some_and(|prepared| prepared.matches(cells, tally)),
    })
}

pub(crate) fn filter_buffered_rows(
    result: &QueryResult,
    predicate: &Predicate,
    cancel: &CancellationToken,
) -> Result<Vec<u32>, &'static str> {
    let kind = result.columns.get(predicate.column).map(filter_value_kind);
    let matches = prepare_filter(result.columns.len(), kind, predicate)?;
    let mut tally = CellTally::default();
    let mut kept = Vec::new();
    for (index, row) in result.rows.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err("canceled the filter");
        }
        if matches(row, &mut tally) {
            kept.push(u32::try_from(index).map_err(|_| "the filter found too many rows")?);
        }
    }
    if cancel.is_cancelled() {
        return Err("canceled the filter");
    }
    Ok(kept)
}

pub fn filter_in(
    scan: &ScanSource,
    predicate: &Predicate,
    cancel: &CancellationToken,
) -> Result<Vec<u32>, &'static str> {
    let kind = scan.columns().get(predicate.column).map(filter_value_kind);
    let matches = prepare_filter(scan.columns().len(), kind, predicate)?;
    let mut kept = Vec::new();
    let mut tally = bookie_results::result_predicate::CellTally::default();
    scan_pages(scan, cancel, &ScanProgress::default(), |index, page| {
        for (row, cells) in page.rows.iter().enumerate() {
            if matches(cells, &mut tally) {
                kept.push((index as u64 * PAGE_ROWS + row as u64) as u32);
            }
        }
        Ok(std::ops::ControlFlow::Continue(()))
    })
    .map_err(|error| match error {
        bookie_results::result_scan::ScanError::Cancelled => "canceled the filter",
        bookie_results::result_scan::ScanError::PageBudget => {
            "not enough memory to scan this result; close other large results and retry"
        }
        _ => "the scan did not finish",
    })?;
    Ok(kept)
}

fn compare_message(error: CompareError) -> &'static str {
    match error {
        CompareError::Cancelled => "canceled the comparison",
        CompareError::PageMemoryBusy => "not enough memory to compare this result; close other large results and retry",
        _ => "the comparison did not finish",
    }
}

fn diff_view(report: &CompareReport, keyed: bool) -> Diff {
    let summary = &report.summary;
    let rows = &report.diff.rows;
    let shown = rows.len().min(MAX_DIFF_ROWS);
    let diff_rows = (0..shown)
        .filter_map(|index| rows.get(index))
        .map(|row| DiffRow {
            change: match row.first() {
                Some(Value::Text(label)) if label == "Added" => "Added",
                Some(Value::Text(label)) if label == "Removed" => "Removed",
                Some(Value::Text(label)) if label == "Before" => "Before",
                _ => "After",
            },
            cells: row.iter().skip(1).cloned().collect(),
        })
        .collect();
    Diff {
        summary: DiffSummary {
            keyed,
            equal: summary.equal,
            added: summary.added,
            removed: summary.removed,
            changed: summary.changed,
            unknown: summary.unknown_left + summary.unknown_right,
        },
        rows: diff_rows,
        capped: rows.len() > MAX_DIFF_ROWS,
    }
}

fn find_message(error: FindError) -> &'static str {
    match error {
        FindError::Predicate(_) => "invalid search text",
        FindError::Scan(bookie_results::result_scan::ScanError::Cancelled) => "canceled the search",
        FindError::Scan(bookie_results::result_scan::ScanError::PageBudget) => {
            "not enough memory to search this result; close other large results and retry"
        }
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
    fn every_result_spools_into_one_budget_for_the_whole_process() {
        let first = Synthetic::new(500).scan(&CancellationToken::new()).unwrap();
        let second = Synthetic::new(700).scan(&CancellationToken::new()).unwrap();
        let reserved = spool_manager().reserved_bytes();
        assert!(
            reserved >= first.spool().stored_bytes() + second.spool().stored_bytes() && reserved > 0,
            "two live results must count against one shared budget, saw {reserved}"
        );
    }

    #[test]
    fn equality_is_exact_and_typed_where_the_type_is_known() {
        let columns = crate::model::synthetic_columns();
        let exact = crate::model::parse_predicate("label = Sample 1", &columns).unwrap();
        let folded = crate::model::parse_predicate("label = sample 1", &columns).unwrap();
        let source = Synthetic::new(10);
        assert_eq!(source.filter(&exact, &CancellationToken::new()), Ok(vec![1]));
        assert_eq!(source.filter(&folded, &CancellationToken::new()), Ok(vec![]));
        let bad = crate::model::parse_predicate("value = many", &columns).unwrap();
        assert_eq!(source.filter(&bad, &CancellationToken::new()), Ok(vec![]));
    }

    #[test]
    fn a_shared_scan_budget_refusal_is_actionable_for_each_consumer() {
        let scan_error = bookie_results::result_scan::ScanError::PageBudget;
        assert!(find_message(FindError::Scan(scan_error)).contains("memory"));
        assert!(compare_message(CompareError::PageMemoryBusy).contains("memory"));
    }
}
