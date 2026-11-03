use bookie_core::Value;

use super::{DiffRow, MAX_COMPARE_DIFFERENCES};

#[derive(Default)]
pub(crate) struct Collector {
    rows: Vec<DiffRow>,
    reserved: usize,
    capped: bool,
}

impl Collector {
    pub(crate) fn has_room(&self) -> bool {
        self.rows.len() + self.reserved < MAX_COMPARE_DIFFERENCES
    }

    pub(crate) fn reserve_pair(&mut self) -> bool {
        if self.rows.len() + self.reserved + 2 > MAX_COMPARE_DIFFERENCES {
            self.capped = true;
            return false;
        }
        self.reserved += 2;
        true
    }

    pub(crate) fn push_reserved_pair(&mut self, before: Vec<Value>, after: Vec<Value>) {
        self.reserved = self.reserved.saturating_sub(2);
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
        if self.has_room() {
            self.rows.push(DiffRow { change, values });
        } else {
            self.capped = true;
        }
    }

    pub(crate) fn mark_capped(&mut self) {
        self.capped = true;
    }

    pub(crate) fn finish(self) -> (Vec<DiffRow>, bool) {
        (self.rows, self.capped)
    }
}
