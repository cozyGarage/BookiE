use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::classify::StatementFacts;
use crate::principal::Principal;
use bookie_core::Environment;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub principal: Principal,
    pub environment: Environment,
    pub connection_id: Uuid,
    pub connection_name: String,
    pub sql: String,
    pub facts: StatementFacts,
    pub rule: String,
    pub reason: String,
    pub preview: Option<String>,
    pub estimated_rows: Option<u64>,
    /// Runtime-only cancellation scope for interactive approval UI.
    #[serde(skip)]
    pub cancellation: Option<CancellationToken>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOutcome {
    AllowOnce,
    Deny,
    AllowAndRemember,
    UsedGrant,
}

#[async_trait]
pub trait ApprovalSink: Send + Sync {
    async fn request(&self, req: ApprovalRequest) -> ApprovalOutcome;
}

/// Always allows. Tests may use this to isolate policy decisions from UI.
pub struct AutoApproveSink;

#[async_trait]
impl ApprovalSink for AutoApproveSink {
    async fn request(&self, _req: ApprovalRequest) -> ApprovalOutcome {
        ApprovalOutcome::AllowOnce
    }
}

/// Always denies RequireApproval. Default for headless agentd without a
/// configured interactive strategy.
pub struct DenyApprovalSink;

#[async_trait]
impl ApprovalSink for DenyApprovalSink {
    async fn request(&self, _req: ApprovalRequest) -> ApprovalOutcome {
        ApprovalOutcome::Deny
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalGrant {
    pub connection_id: Uuid,
    pub rule: String,
    pub principal: Principal,
}

#[derive(Debug, Default)]
pub struct ApprovalGrants {
    grants: std::sync::Mutex<Vec<ApprovalGrant>>,
}

impl ApprovalGrants {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<ApprovalGrant>> {
        self.grants.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn list(&self) -> Vec<ApprovalGrant> {
        self.lock().clone()
    }

    pub fn revoke(&self, grant: &ApprovalGrant) -> bool {
        let mut grants = self.lock();
        let before = grants.len();
        grants.retain(|existing| existing != grant);
        grants.len() != before
    }

    pub fn revoke_connection(&self, connection_id: Uuid) {
        self.lock().retain(|grant| grant.connection_id != connection_id);
    }

    fn covers(&self, grant: &ApprovalGrant) -> bool {
        self.lock().contains(grant)
    }

    fn add(&self, grant: ApprovalGrant) {
        let mut grants = self.lock();
        if !grants.contains(&grant) {
            grants.push(grant);
        }
    }
}

fn grant_for(req: &ApprovalRequest) -> ApprovalGrant {
    ApprovalGrant {
        connection_id: req.connection_id,
        rule: req.rule.clone(),
        principal: req.principal.clone(),
    }
}

pub struct RememberingApprovalSink<S> {
    inner: S,
    grants: std::sync::Arc<ApprovalGrants>,
}

impl<S: ApprovalSink> RememberingApprovalSink<S> {
    pub fn new(inner: S, grants: std::sync::Arc<ApprovalGrants>) -> Self {
        Self { inner, grants }
    }
}

#[async_trait]
impl<S: ApprovalSink> ApprovalSink for RememberingApprovalSink<S> {
    async fn request(&self, req: ApprovalRequest) -> ApprovalOutcome {
        let may_remember = req.environment != Environment::Prod;
        let grant = grant_for(&req);
        if may_remember && self.grants.covers(&grant) {
            return ApprovalOutcome::UsedGrant;
        }
        match self.inner.request(req).await {
            ApprovalOutcome::AllowAndRemember if may_remember => {
                self.grants.add(grant);
                ApprovalOutcome::AllowAndRemember
            }
            ApprovalOutcome::AllowAndRemember | ApprovalOutcome::UsedGrant => ApprovalOutcome::AllowOnce,
            other => other,
        }
    }
}
