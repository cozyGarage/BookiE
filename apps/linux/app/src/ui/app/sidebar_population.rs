use std::ops::Range;

pub(super) const BATCH_SIZE: usize = 8;

pub(super) struct SidebarPopulation {
    total: usize,
    next: usize,
}

impl SidebarPopulation {
    pub(super) fn new(total: usize) -> Self {
        Self { total, next: 0 }
    }

    pub(super) fn next_batch(&mut self) -> Option<Range<usize>> {
        if self.next >= self.total {
            return None;
        }
        let start = self.next;
        self.next = self.next.saturating_add(BATCH_SIZE).min(self.total);
        Some(start..self.next)
    }

    pub(super) fn has_remaining(&self) -> bool {
        self.next < self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_cover_the_catalog_once_and_never_exceed_the_limit() {
        let mut population = SidebarPopulation::new(37);
        let batches: Vec<_> = std::iter::from_fn(|| population.next_batch()).collect();
        assert_eq!(batches, [0..8, 8..16, 16..24, 24..32, 32..37]);
        assert!(batches.iter().all(|batch| batch.len() <= BATCH_SIZE));
        assert_eq!(population.next_batch(), None);
    }

    #[test]
    fn empty_catalog_has_no_batches() {
        assert_eq!(SidebarPopulation::new(0).next_batch(), None);
    }
}
