use bookie_app_services::result_find::{FindMatch, FindReport};
use bookie_app_services::text_match::TextMatchOptions;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum SearchState {
    #[default]
    Idle,
    Searching,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct SearchSpec {
    pub needle: String,
    pub options: TextMatchOptions,
    pub column: Option<usize>,
}

#[derive(Default)]
pub(super) struct MatchCursor {
    generation: u64,
    matches: Vec<FindMatch>,
    capped: bool,
    unsearchable: u64,
    current: Option<usize>,
    spec: SearchSpec,
    state: SearchState,
    requested_forward: Option<bool>,
}

impl MatchCursor {
    pub(super) fn begin(&mut self, spec: SearchSpec) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.matches.clear();
        self.capped = false;
        self.unsearchable = 0;
        self.current = None;
        self.state = SearchState::Searching;
        self.requested_forward = None;
        self.spec = spec;
        self.generation
    }

    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.matches.clear();
        self.current = None;
        self.state = SearchState::Idle;
    }

    pub(super) fn end_search(&mut self, generation: u64) {
        if self.accepts(generation) && self.state == SearchState::Searching {
            self.state = SearchState::Idle;
        }
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
        self.state = SearchState::Complete;
        true
    }

    pub(super) fn has_results_for(&self, spec: &SearchSpec) -> bool {
        self.state == SearchState::Complete && &self.spec == spec
    }

    pub(super) fn request_direction(&mut self, forward: bool) {
        self.requested_forward = Some(forward);
    }

    pub(super) fn take_direction(&mut self) -> Option<bool> {
        self.requested_forward.take()
    }

    pub(super) fn is_searching_for(&self, spec: &SearchSpec) -> bool {
        self.state == SearchState::Searching && &self.spec == spec
    }

    pub(super) fn keeps_for(&self, spec: &SearchSpec) -> bool {
        self.state != SearchState::Idle && &self.spec == spec
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
        if self.state != SearchState::Complete {
            return String::new();
        }
        let base = self.base_status();
        if self.unsearchable == 0 {
            return base;
        }
        let skipped = bookie_app_services::trn!(
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
            return bookie_app_services::tr!("No match");
        }
        let total = if self.capped { format!("{total}+") } else { total };
        match self.current {
            Some(index) => bookie_app_services::tr!("Match {n} of {total}")
                .replace("{n}", &crate::ui::count_format::format_thousands(index as u128 + 1))
                .replace("{total}", &total),
            None if self.capped => bookie_app_services::tr!("{total} matches (list capped)").replace("{total}", &total),
            None => bookie_app_services::tr!("{total} matches").replace("{total}", &total),
        }
    }
}
