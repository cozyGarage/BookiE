use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use bookie_app_services::result_scan::ScanProgress;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Default)]
pub(in crate::ui::editor) struct FilterTicket {
    pub(super) cancel: CancellationToken,
    pub(super) progress: ScanProgress,
    finished: AtomicBool,
    message: Mutex<String>,
}

impl FilterTicket {
    pub(in crate::ui::editor) fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    pub(in crate::ui::editor) fn progress(&self) -> &ScanProgress {
        &self.progress
    }

    pub(in crate::ui::editor) fn finish(&self, message: String) {
        *self.message.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = message;
        self.finished.store(true, Ordering::Release);
    }

    pub(super) fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub(super) fn message(&self) -> String {
        self.message
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
