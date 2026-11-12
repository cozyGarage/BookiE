use bookie_app_services::server_messages_view::MessagesView;
use bookie_app_services::statement_timing::StatementTiming;
use bookie_core::ServerMessages;

use super::StatementOutcome;

#[derive(Debug, Clone, Default)]
pub struct OutcomeDetail {
    pub messages: ServerMessages,
    pub approval_wait_ms: u128,
}

pub(super) fn totals(outcomes: &[StatementOutcome]) -> (StatementTiming, MessagesView) {
    let timing = StatementTiming::total(
        outcomes
            .iter()
            .map(|outcome| StatementTiming::new(outcome.elapsed_ms, outcome.detail.approval_wait_ms)),
    );
    let mut messages = ServerMessages::default();
    for outcome in outcomes {
        messages.extend(outcome.detail.messages.clone());
    }
    (timing, MessagesView::new(&messages))
}

#[cfg(test)]
mod tests {
    use bookie_core::{MessageSeverity, ServerMessage};

    use super::*;
    use crate::ui::editor::{StatementIdentity, StatementOutcomeKind};

    fn outcome(elapsed_ms: u128, approval_wait_ms: u128, texts: &[&str]) -> StatementOutcome {
        let mut messages = ServerMessages::default();
        for text in texts {
            messages.push(ServerMessage::new(MessageSeverity::Notice, None, text));
        }
        StatementOutcome {
            identity: StatementIdentity::default(),
            sql_preview: String::new(),
            sql: String::new(),
            elapsed_ms,
            kind: StatementOutcomeKind::Rows(Vec::new()),
            pinned: false,
            detail: OutcomeDetail {
                messages,
                approval_wait_ms,
            },
        }
    }

    #[test]
    fn a_script_totals_its_timing_and_counts_every_statements_messages() {
        let (timing, view) = totals(&[outcome(10, 5, &["a", "b"]), outcome(20, 0, &["c"])]);
        assert_eq!(timing, StatementTiming::new(30, 5));
        assert_eq!(view.total(), 3);
    }
}
