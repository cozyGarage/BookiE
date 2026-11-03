use bookie_app_services::result_find::{FindMatch, FindReport};

#[derive(Default)]
pub(super) struct MatchCursor {
    generation: u64,
    matches: Vec<FindMatch>,
    capped: bool,
    unsearchable: u64,
    current: Option<usize>,
    needle: String,
    complete: bool,
    searching: bool,
}

impl MatchCursor {
    pub(super) fn begin(&mut self, needle: &str) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.matches.clear();
        self.capped = false;
        self.unsearchable = 0;
        self.current = None;
        self.complete = false;
        self.searching = true;
        self.needle = needle.to_owned();
        self.generation
    }

    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.matches.clear();
        self.current = None;
        self.complete = false;
        self.searching = false;
    }

    pub(super) fn accepts(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub(super) fn complete(&mut self, generation: u64, report: FindReport) -> bool {
        if !self.accepts(generation) {
            return false;
        }
        self.matches = report.matches;
        self.capped = report.capped;
        self.unsearchable = report.unsearchable_cells;
        self.complete = true;
        self.searching = false;
        true
    }

    pub(super) fn has_results_for(&self, needle: &str) -> bool {
        self.complete && self.needle == needle
    }

    pub(super) fn keeps_for(&self, needle: &str) -> bool {
        (self.complete || self.searching) && self.needle == needle
    }

    pub(super) fn step(&mut self, forward: bool) -> Option<FindMatch> {
        let count = self.matches.len();
        if count == 0 {
            return None;
        }
        let next = match (self.current, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
        };
        self.current = Some(next);
        self.matches.get(next).copied()
    }

    pub(super) fn status(&self) -> String {
        if !self.complete {
            return String::new();
        }
        let base = self.base_status();
        if self.unsearchable == 0 {
            return base;
        }
        let skipped = crate::trn!(
            "{n} cell could not be searched",
            "{n} cells could not be searched",
            self.unsearchable as usize
        )
        .replace(
            "{n}",
            &crate::ui::count_format::format_thousands(u128::from(self.unsearchable)),
        );
        format!("{base} · {skipped}")
    }

    fn base_status(&self) -> String {
        let total = crate::ui::count_format::format_thousands(self.matches.len() as u128);
        if self.matches.is_empty() {
            return crate::tr!("No match");
        }
        let total = if self.capped { format!("{total}+") } else { total };
        match self.current {
            Some(index) => crate::tr!("Match {n} of {total}")
                .replace("{n}", &crate::ui::count_format::format_thousands(index as u128 + 1))
                .replace("{total}", &total),
            None if self.capped => crate::tr!("{total} matches (list capped)").replace("{total}", &total),
            None => crate::tr!("{total} matches").replace("{total}", &total),
        }
    }
}
