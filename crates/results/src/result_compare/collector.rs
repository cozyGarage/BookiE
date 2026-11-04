use bookie_core::{QueryResultBudget, Value};

use super::{DiffRow, MAX_COMPARE_DIFFERENCES};

#[derive(Default)]
pub(crate) struct Collector {
    rows: Vec<DiffRow>,
    reserved: usize,
    budget: QueryResultBudget,
    capped: bool,
}

impl Collector {
    pub(crate) fn has_room(&self) -> bool {
        self.rows.len() + self.reserved < MAX_COMPARE_DIFFERENCES
    }

    pub(crate) fn reserve_pair(&mut self, after: impl FnOnce() -> Vec<Value>) -> Option<Vec<Value>> {
        if self.rows.len() + self.reserved + 2 > MAX_COMPARE_DIFFERENCES {
            self.capped = true;
            return None;
        }
        let after = after();
        if !self.budget.admit(&after) {
            self.capped = true;
            return None;
        }
        self.reserved += 2;
        Some(after)
    }

    pub(crate) fn push_reserved_pair(&mut self, before: Vec<Value>, after: Vec<Value>) {
        self.reserved = self.reserved.saturating_sub(2);
        if !self.budget.admit(&before) {
            self.capped = true;
            return;
        }
        self.rows.push(DiffRow {
            change: super::keyed::BEFORE,
            values: before,
        });
        self.rows.push(DiffRow {
            change: super::keyed::AFTER,
            values: after,
        });
    }

    pub(crate) fn push(&mut self, change: &'static str, values: Vec<Value>) {
        if self.has_room() && self.budget.admit(&values) {
            self.rows.push(DiffRow { change, values });
        } else {
            self.capped = true;
        }
    }

    pub(crate) fn finish(self) -> (Vec<DiffRow>, bool) {
        (self.rows, self.capped)
    }
}
