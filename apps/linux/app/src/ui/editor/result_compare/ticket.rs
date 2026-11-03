use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use bookie_app_services::result_scan::ScanProgress;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Default)]
pub(in crate::ui) struct CompareTicket {
    cancel: CancellationToken,
    progress: ScanProgress,
    finished: AtomicBool,
    message: Mutex<String>,
}

impl CompareTicket {
    pub(super) fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    pub(super) fn progress(&self) -> &ScanProgress {
        &self.progress
    }

    pub(super) fn finish(&self, message: String) {
        *self.message.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = message;
        self.finished.store(true, Ordering::Release);
    }

    pub(super) fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}
