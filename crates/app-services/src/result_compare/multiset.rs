use std::collections::HashMap;

use bookie_core::Value;
use tokio_util::sync::CancellationToken;

use super::collector::Collector;
use super::columns::CommonColumns;
use super::digest::{Digest, cells_digest, has_unreadable};
use super::{CompareError, CompareMode, CompareRequest, CompareSummary, Outcome};
use crate::result_scan::ScanProgress;

pub(crate) const REMOVED: &str = "Removed";
pub(crate) const ADDED: &str = "Added";
pub(crate) const UNKNOWN_LEFT: &str = "Unknown (left)";
pub(crate) const UNKNOWN_RIGHT: &str = "Unknown (right)";

pub(crate) fn compare(
    request: &CompareRequest,
    common: &CommonColumns,
    cancel: &CancellationToken,
    progress: &ScanProgress,
) -> Result<Outcome, CompareError> {
    let mut collector = Collector::default();
    let mut counts: HashMap<Digest, u32> = HashMap::new();
    let mut summary = empty_summary();
    summary.rows_left = request.left.for_each_row(cancel, progress, |row| {
        count_left(row, common, &mut counts, &mut collector, &mut summary)
    })?;
    summary.rows_right = request.right.for_each_row(cancel, progress, |row| {
        match_right(row, common, &mut counts, &mut collector, &mut summary);
        Ok(())
    })?;
    summary.removed = counts.values().map(|&count| u64::from(count)).sum();
    if summary.removed > 0 {
        collect_removed(request, common, cancel, progress, &mut counts, &mut collector)?;
    }
    let (rows, capped) = collector.finish();
    summary.differences_capped = capped;
    Ok(Outcome { summary, rows })
}

fn empty_summary() -> CompareSummary {
    CompareSummary {
        mode: CompareMode::WholeRow,
        key_columns: 0,
        rows_left: 0,
        rows_right: 0,
        equal: 0,
        added: 0,
        removed: 0,
        changed: 0,
        unknown_left: 0,
        unknown_right: 0,
        differences_capped: false,
    }
}

fn count_left(
    row: &[Value],
    common: &CommonColumns,
    counts: &mut HashMap<Digest, u32>,
    collector: &mut Collector,
    summary: &mut CompareSummary,
) -> Result<(), CompareError> {
    if has_unreadable(row, common.left_indices()) {
        summary.unknown_left += 1;
        collector.push(UNKNOWN_LEFT, common.project_left(row));
        return Ok(());
    }
    *counts.entry(cells_digest(row, common.left_indices())).or_insert(0) += 1;
    Ok(())
}

fn match_right(
    row: &[Value],
    common: &CommonColumns,
    counts: &mut HashMap<Digest, u32>,
    collector: &mut Collector,
    summary: &mut CompareSummary,
) {
    if has_unreadable(row, common.right_indices()) {
        summary.unknown_right += 1;
        collector.push(UNKNOWN_RIGHT, common.project_right(row));
        return;
    }
    match counts.get_mut(&cells_digest(row, common.right_indices())) {
        Some(count) if *count > 0 => {
            *count -= 1;
            summary.equal += 1;
        }
        _ => {
            summary.added += 1;
            collector.push(ADDED, common.project_right(row));
        }
    }
}

fn collect_removed(
    request: &CompareRequest,
    common: &CommonColumns,
    cancel: &CancellationToken,
    progress: &ScanProgress,
    counts: &mut HashMap<Digest, u32>,
    collector: &mut Collector,
) -> Result<(), CompareError> {
    request.left.for_each_row(cancel, progress, |row| {
        if has_unreadable(row, common.left_indices()) {
            return Ok(());
        }
        let digest = cells_digest(row, common.left_indices());
        if let Some(count) = counts.get_mut(&digest)
            && *count > 0
        {
            *count -= 1;
            collector.push(REMOVED, common.project_left(row));
        }
        Ok(())
    })?;
    Ok(())
}
