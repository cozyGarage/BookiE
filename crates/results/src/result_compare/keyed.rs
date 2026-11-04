use std::collections::{HashMap, HashSet};

use bookie_core::Value;
use tokio_util::sync::CancellationToken;

use super::collector::Collector;
use super::columns::CommonColumns;
use super::digest::{Digest, cells_digest, has_null_or_unreadable, has_unreadable};
use super::multiset::{ADDED, REMOVED, UNKNOWN_LEFT, UNKNOWN_RIGHT};
use super::{CompareError, CompareMode, CompareRequest, CompareSummary, Outcome};
use crate::result_scan::ScanProgress;

pub(super) const BEFORE: &str = "Before";
pub(super) const AFTER: &str = "After";

struct LeftEntry {
    row: Digest,
    unknown: bool,
    seen: bool,
}

struct Indices {
    key_left: Vec<usize>,
    key_right: Vec<usize>,
}

struct State {
    entries: HashMap<Digest, LeftEntry>,
    wanted: HashSet<Digest>,
    added_keys: HashSet<Digest>,
    after: HashMap<Digest, Vec<Value>>,
    collector: Collector,
    summary: CompareSummary,
}

pub(crate) fn compare(
    request: &CompareRequest,
    common: &CommonColumns,
    cancel: &CancellationToken,
    progress: &ScanProgress,
) -> Result<Outcome, CompareError> {
    let indices = Indices {
        key_left: common.key_left(),
        key_right: common.key_right(),
    };
    let mut state = State::new(request.key_columns.len());
    state.summary.rows_left = request
        .left
        .for_each_row(cancel, progress, |row| state.index_left(row, common, &indices))?;
    state.summary.rows_right = request
        .right
        .for_each_row(cancel, progress, |row| state.match_right(row, common, &indices))?;
    state.summary.removed = state.entries.values().filter(|entry| !entry.seen).count() as u64;
    if state.needs_left_pass() {
        request.left.for_each_row(cancel, progress, |row| {
            state.collect_left(row, common, &indices);
            Ok(())
        })?;
    }
    Ok(state.finish())
}

impl State {
    fn new(key_columns: usize) -> Self {
        Self {
            entries: HashMap::new(),
            wanted: HashSet::new(),
            added_keys: HashSet::new(),
            after: HashMap::new(),
            collector: Collector::default(),
            summary: CompareSummary {
                mode: CompareMode::Keyed,
                key_columns,
                rows_left: 0,
                rows_right: 0,
                equal: 0,
                added: 0,
                removed: 0,
                changed: 0,
                unknown_left: 0,
                unknown_right: 0,
                differences_capped: false,
            },
        }
    }

    fn index_left(&mut self, row: &[Value], common: &CommonColumns, indices: &Indices) -> Result<(), CompareError> {
        if has_null_or_unreadable(row, &indices.key_left) {
            return Err(CompareError::UnreadableKey);
        }
        let entry = LeftEntry {
            row: cells_digest(row, common.left_indices()),
            unknown: has_unreadable(row, common.left_indices()),
            seen: false,
        };
        if self
            .entries
            .insert(cells_digest(row, &indices.key_left), entry)
            .is_some()
        {
            return Err(CompareError::DuplicateKey);
        }
        Ok(())
    }

    fn match_right(&mut self, row: &[Value], common: &CommonColumns, indices: &Indices) -> Result<(), CompareError> {
        if has_null_or_unreadable(row, &indices.key_right) {
            return Err(CompareError::UnreadableKey);
        }
        let key = cells_digest(row, &indices.key_right);
        let right_unknown = has_unreadable(row, common.right_indices());
        let row_digest = cells_digest(row, common.right_indices());
        let Some(entry) = self.entries.get_mut(&key) else {
            if !self.added_keys.insert(key) {
                return Err(CompareError::DuplicateKey);
            }
            self.summary.added += 1;
            self.collector.push(ADDED, common.project_right(row));
            return Ok(());
        };
        if entry.seen {
            return Err(CompareError::DuplicateKey);
        }
        entry.seen = true;
        let left_unknown = entry.unknown;
        let equal = entry.row == row_digest;
        self.classify_pair(key, left_unknown, right_unknown, equal, || common.project_right(row));
        Ok(())
    }

    fn classify_pair(
        &mut self,
        key: Digest,
        left_unknown: bool,
        right_unknown: bool,
        equal: bool,
        right_values: impl FnOnce() -> Vec<Value>,
    ) {
        if left_unknown || right_unknown {
            self.summary.unknown_left += u64::from(left_unknown);
            self.summary.unknown_right += u64::from(right_unknown);
            self.wanted.insert(key);
            if right_unknown {
                self.collector.push(UNKNOWN_RIGHT, right_values());
            }
            return;
        }
        if equal {
            self.summary.equal += 1;
            return;
        }
        self.summary.changed += 1;
        self.wanted.insert(key);
        if let Some(after) = self.collector.reserve_pair(right_values) {
            self.after.insert(key, after);
        }
    }

    fn needs_left_pass(&self) -> bool {
        self.summary.removed > 0 || !self.wanted.is_empty()
    }

    fn collect_left(&mut self, row: &[Value], common: &CommonColumns, indices: &Indices) {
        let key = cells_digest(row, &indices.key_left);
        let removed = self.entries.get(&key).is_some_and(|entry| !entry.seen);
        if removed {
            self.collector.push(REMOVED, common.project_left(row));
            return;
        }
        if !self.wanted.contains(&key) {
            return;
        }
        let unknown = has_unreadable(row, common.left_indices());
        if unknown {
            self.collector.push(UNKNOWN_LEFT, common.project_left(row));
        } else if let Some(after) = self.after.remove(&key) {
            self.collector.push_reserved_pair(common.project_left(row), after);
        }
    }

    fn finish(self) -> Outcome {
        let (rows, capped) = self.collector.finish();
        let mut summary = self.summary;
        summary.differences_capped = capped;
        Outcome { summary, rows }
    }
}
