use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Default)]
pub(super) struct ReloadSequence(Arc<AtomicU64>);

impl ReloadSequence {
    pub(super) fn begin(&self) -> u64 {
        self.0.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub(super) fn is_latest(&self, sequence: u64) -> bool {
        self.0.load(Ordering::Acquire) == sequence
    }
}

#[cfg(test)]
mod tests {
    use super::ReloadSequence;

    #[test]
    fn a_reload_that_finishes_after_a_newer_one_started_is_stale() {
        let sequence = ReloadSequence::default();
        let older = sequence.begin();
        let newer = sequence.begin();
        assert!(!sequence.is_latest(older));
        assert!(sequence.is_latest(newer));
    }

    #[test]
    fn a_reload_with_no_newer_one_is_current_and_clones_share_the_count() {
        let sequence = ReloadSequence::default();
        let shared = sequence.clone();
        let only = sequence.begin();
        assert!(shared.is_latest(only));
        let next = shared.begin();
        assert!(!sequence.is_latest(only));
        assert!(sequence.is_latest(next));
    }
}
