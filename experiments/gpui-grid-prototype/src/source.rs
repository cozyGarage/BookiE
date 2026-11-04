use std::sync::Arc;

use crate::model::{Cell, Diff, FindResult, Predicate, build_diff, filter_rows, find_rows, row_at};

pub trait ResultSource: Send + Sync + 'static {
    fn total(&self) -> u64;
    fn row(&self, index: u64) -> Vec<Cell>;
    fn find(&self, query: &str, cap: usize) -> Result<FindResult, &'static str>;
    fn filter(&self, predicate: &Predicate) -> Vec<u32>;
    fn compare(&self, keyed: bool) -> Diff;
}

pub type Source = Arc<dyn ResultSource>;

pub struct Synthetic {
    pub total: u64,
}

impl ResultSource for Synthetic {
    fn total(&self) -> u64 {
        self.total
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        row_at(index)
    }

    fn find(&self, query: &str, cap: usize) -> Result<FindResult, &'static str> {
        find_rows(self.total, query, cap)
    }

    fn filter(&self, predicate: &Predicate) -> Vec<u32> {
        filter_rows(self.total, predicate)
    }

    fn compare(&self, keyed: bool) -> Diff {
        build_diff(self.total, keyed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_synthetic_source_answers_like_the_model_functions() {
        let source: Source = Arc::new(Synthetic { total: 1_000 });
        assert_eq!(source.total(), 1_000);
        assert_eq!(source.row(7), row_at(7));
        assert_eq!(source.find("sample 999", 10).map(|found| found.matches), Ok(vec![999]));
        assert!(!source.compare(false).summary.keyed);
    }
}
