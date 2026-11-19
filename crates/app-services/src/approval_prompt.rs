use std::time::Duration;

use async_trait::async_trait;
pub use bookie_policy::{ApprovalOutcome, ApprovalRequest};
use bookie_policy::{ApprovalSink, ReviewableApprovalSink};
use tokio::sync::{mpsc, oneshot};

pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

pub struct PendingApproval {
    pub request: ApprovalRequest,
    pub respond: oneshot::Sender<ApprovalOutcome>,
}

pub type ApprovalInbox = mpsc::UnboundedReceiver<PendingApproval>;

pub fn channel() -> (ChannelApprovalSink, ApprovalInbox) {
    let (tx, inbox) = mpsc::unbounded_channel();
    (ChannelApprovalSink::new(tx), inbox)
}

pub fn reviewable_prompt_sink(tx: mpsc::UnboundedSender<PendingApproval>) -> std::sync::Arc<dyn ApprovalSink> {
    std::sync::Arc::new(ReviewableApprovalSink::new(ChannelApprovalSink::new(tx)))
}

pub fn prompt_channel() -> (mpsc::UnboundedSender<PendingApproval>, ApprovalInbox) {
    mpsc::unbounded_channel()
}

#[derive(Clone)]
pub struct ChannelApprovalSink {
    tx: mpsc::UnboundedSender<PendingApproval>,
    timeout: Duration,
}

impl ChannelApprovalSink {
    pub fn new(tx: mpsc::UnboundedSender<PendingApproval>) -> Self {
        Self {
            tx,
            timeout: APPROVAL_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait]
impl ApprovalSink for ChannelApprovalSink {
    async fn request(&self, req: ApprovalRequest) -> ApprovalOutcome {
        let cancellation = req.cancellation.clone();
        let (respond, answer) = oneshot::channel();
        if self.tx.send(PendingApproval { request: req, respond }).is_err() {
            return ApprovalOutcome::Deny;
        }
        let cancelled = async {
            match &cancellation {
                Some(token) => token.cancelled().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            biased;
            () = cancelled => ApprovalOutcome::Deny,
            answered = tokio::time::timeout(self.timeout, answer) => match answered {
                Ok(Ok(outcome)) => outcome,
                _ => ApprovalOutcome::Deny,
            },
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn sample_request(sql: &str) -> ApprovalRequest {
    ApprovalRequest {
        principal: bookie_policy::Principal::human_gui(),
        environment: bookie_core::Environment::Prod,
        connection_id: uuid::Uuid::nil(),
        connection_name: "db".into(),
        sql: sql.into(),
        facts: bookie_policy::StatementFacts::unparseable("test"),
        rule: "rule".into(),
        reason: "reason".into(),
        preview: None,
        estimated_rows: None,
        cancellation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(sql: &str) -> ApprovalRequest {
        sample_request(sql)
    }

    #[tokio::test]
    async fn the_answer_given_in_the_window_is_the_outcome() {
        let (sink, mut inbox) = channel();
        let asked = tokio::spawn(async move { sink.request(request("UPDATE t SET a = 1")).await });
        let pending = inbox.recv().await.expect("a prompt");
        assert_eq!(pending.request.sql, "UPDATE t SET a = 1");
        let _ = pending.respond.send(ApprovalOutcome::AllowOnce);
        assert_eq!(asked.await.expect("joined"), ApprovalOutcome::AllowOnce);
    }

    #[tokio::test]
    async fn a_prompt_nobody_answers_is_denied_when_it_times_out() {
        let (sink, mut inbox) = channel();
        let sink = sink.with_timeout(Duration::from_millis(30));
        let asked = tokio::spawn(async move { sink.request(request("DELETE FROM t")).await });
        let pending = inbox.recv().await.expect("a prompt");
        assert_eq!(asked.await.expect("joined"), ApprovalOutcome::Deny);
        assert!(pending.respond.is_closed(), "the window can see the question is gone");
    }

    #[tokio::test]
    async fn a_closed_window_or_a_dropped_answer_denies() {
        let (sink, inbox) = channel();
        drop(inbox);
        assert_eq!(sink.request(request("DELETE FROM t")).await, ApprovalOutcome::Deny);
        let (sink, mut inbox) = channel();
        let asked = tokio::spawn(async move { sink.request(request("DELETE FROM t")).await });
        drop(inbox.recv().await);
        assert_eq!(asked.await.expect("joined"), ApprovalOutcome::Deny);
    }

    #[tokio::test]
    async fn cancelling_the_operation_denies_the_open_prompt() {
        let (sink, mut inbox) = channel();
        let token = tokio_util::sync::CancellationToken::new();
        let mut req = request("DELETE FROM t");
        req.cancellation = Some(token.clone());
        let asked = tokio::spawn(async move { sink.request(req).await });
        let _pending = inbox.recv().await.expect("a prompt");
        token.cancel();
        assert_eq!(asked.await.expect("joined"), ApprovalOutcome::Deny);
    }

    #[tokio::test]
    async fn a_statement_nobody_can_read_never_reaches_the_prompt() {
        let (sink, mut inbox) = channel();
        let sink = ReviewableApprovalSink::new(sink);
        let hidden = "SELECT 1\u{202E}; DROP TABLE t";
        assert_eq!(sink.request(request(hidden)).await, ApprovalOutcome::Deny);
        assert!(inbox.try_recv().is_err(), "the question was never asked");
    }
}
