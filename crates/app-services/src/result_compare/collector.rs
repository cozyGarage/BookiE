use bookie_core::Value;

use super::{DiffRow, MAX_COMPARE_DIFFERENCES};

#[derive(Default)]
pub(crate) struct Collector {
    rows: Vec<DiffRow>,
    capped: bool,
}

impl Collector {
    pub(crate) fn has_room(&self) -> bool {
        self.rows.len() < MAX_COMPARE_DIFFERENCES
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
