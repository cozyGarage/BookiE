use std::sync::Arc;

use async_trait::async_trait;
use tablepro_policy::{ApprovalOutcome, ApprovalRequest, ApprovalSink, Principal};

pub struct ApprovalRouter {
    human: Arc<dyn ApprovalSink>,
    agent: Arc<dyn ApprovalSink>,
}

impl ApprovalRouter {
    pub fn new(human: Arc<dyn ApprovalSink>, agent: Arc<dyn ApprovalSink>) -> Self {
        Self { human, agent }
    }
}

#[async_trait]
impl ApprovalSink for ApprovalRouter {
    async fn request(&self, request: ApprovalRequest) -> ApprovalOutcome {
        let sink = match &request.principal {
            Principal::Human { .. } => &self.human,
            Principal::Agent { .. } => &self.agent,
        };
        super::approval_wait::measure(sink.request(request)).await
    }
}

#[cfg(test)]
#[path = "approval_router_tests.rs"]
mod tests;
