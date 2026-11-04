use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use tokio_util::sync::CancellationToken;

#[derive(Debug, Default)]
pub struct BusyJobs {
    jobs: Arc<Mutex<HashMap<u64, CancellationToken>>>,
    next: AtomicU64,
}

#[derive(Debug)]
pub struct BusyJob {
    id: u64,
    jobs: Arc<Mutex<HashMap<u64, CancellationToken>>>,
}

impl BusyJobs {
    pub fn begin(&self, cancel: CancellationToken) -> BusyJob {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, cancel);
        BusyJob {
            id,
            jobs: Arc::clone(&self.jobs),
        }
    }

    pub fn active(&self) -> usize {
        self.jobs.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn cancel_all(&self) {
        for token in self.jobs.lock().unwrap_or_else(PoisonError::into_inner).values() {
            token.cancel();
        }
    }
}

impl Drop for BusyJob {
    fn drop(&mut self) {
        self.jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

pub fn global() -> &'static BusyJobs {
    static GLOBAL: OnceLock<BusyJobs> = OnceLock::new();
    GLOBAL.get_or_init(BusyJobs::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_counts_until_its_guard_is_dropped() {
        let jobs = BusyJobs::default();
        assert_eq!(jobs.active(), 0);
        let first = jobs.begin(CancellationToken::new());
        let second = jobs.begin(CancellationToken::new());
        assert_eq!(jobs.active(), 2);
        drop(first);
        assert_eq!(jobs.active(), 1);
        drop(second);
        assert_eq!(jobs.active(), 0);
    }

    #[test]
    fn cancelling_all_reaches_every_running_job_and_leaves_them_registered() {
        let jobs = BusyJobs::default();
        let (a, b) = (CancellationToken::new(), CancellationToken::new());
        let _guards = [jobs.begin(a.clone()), jobs.begin(b.clone())];
        jobs.cancel_all();
        assert!(a.is_cancelled() && b.is_cancelled());
        assert_eq!(jobs.active(), 2, "a job stays busy until it has actually finished");
    }

    #[test]
    fn a_finished_job_is_not_cancelled_later() {
        let jobs = BusyJobs::default();
        let token = CancellationToken::new();
        drop(jobs.begin(token.clone()));
        jobs.cancel_all();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn the_global_registry_is_shared() {
        assert!(std::ptr::eq(global(), global()));
    }
}
