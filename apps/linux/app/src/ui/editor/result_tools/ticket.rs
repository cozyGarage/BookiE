use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bookie_app_services::result_scan::ScanProgress;
use tokio_util::sync::CancellationToken;

use super::super::StatementIdentity;

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

    pub(in crate::ui::editor) fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub(super) fn message(&self) -> String {
        self.message
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

thread_local! {
    static ACTIVE: RefCell<Vec<(StatementIdentity, Arc<FilterTicket>)>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn register(identity: StatementIdentity, ticket: &Arc<FilterTicket>) {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        active.retain(|(_, ticket)| !ticket.is_finished());
        active.push((identity, ticket.clone()));
    });
}

pub(super) fn running(identity: StatementIdentity) -> bool {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        active.retain(|(_, ticket)| !ticket.is_finished());
        active.iter().any(|(candidate, _)| *candidate == identity)
    })
}

pub(super) fn cancel(identity: StatementIdentity) {
    ACTIVE.with(|active| {
        for (candidate, ticket) in active.borrow().iter() {
            if *candidate == identity {
                ticket.cancel().cancel();
            }
        }
    });
}

pub(super) fn active(identity: StatementIdentity) -> Option<Arc<FilterTicket>> {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        active.retain(|(_, ticket)| !ticket.is_finished());
        active
            .iter()
            .find(|(candidate, _)| *candidate == identity)
            .map(|(_, ticket)| ticket.clone())
    })
}
