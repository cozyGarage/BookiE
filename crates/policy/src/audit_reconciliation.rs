use bookie_core::Environment;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::audit::{
    AuditApprovalOutcome, AuditConnectionContext, AuditEvent, AuditOperationClass, AuditPreviewState, AuditRecordPhase,
    AuditTerminalStatus, AuditTransactionOutcome,
};
use crate::principal::Principal;

pub const MAX_RECONCILIATION_REASON_CHARS: usize = 1000;
const MAX_OPERATOR_CHARS: usize = 256;
const RULE: &str = "write_outcome_reconciliation";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditReconciliationClaim {
    Committed,
    RolledBack,
    StillUnknown,
}

impl AuditReconciliationClaim {
    pub fn clears_the_block(self) -> bool {
        !matches!(self, Self::StillUnknown)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditReconciliationEvidence {
    None,
    CommitTokenChecked,
    ServerCheckedManually,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReconciliation {
    pub reconciles_operation_id: Uuid,
    pub claim: AuditReconciliationClaim,
    pub reason: String,
    pub evidence: AuditReconciliationEvidence,
    pub operator_os_user: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ReconciliationError {
    #[error("only a person can resolve an unknown write outcome")]
    NotAPerson,
    #[error("give a reason for the decision")]
    ReasonEmpty,
    #[error("the reason is longer than {MAX_RECONCILIATION_REASON_CHARS} characters")]
    ReasonTooLong,
    #[error("the reason contains a control character")]
    ReasonHasControlCharacter,
    #[error("the operator name is empty, too long or has unusual characters")]
    OperatorInvalid,
    #[error("a production connection needs the evidence for a committed or rolled back claim")]
    EvidenceRequired,
}

impl AuditReconciliation {
    pub fn new(
        reconciles_operation_id: Uuid,
        claim: AuditReconciliationClaim,
        reason: &str,
        evidence: AuditReconciliationEvidence,
        operator_os_user: String,
    ) -> Self {
        Self {
            reconciles_operation_id,
            claim,
            reason: reason.trim().to_string(),
            evidence,
            operator_os_user,
        }
    }

    pub fn validate(&self, environment: Environment) -> Result<(), ReconciliationError> {
        if self.reason.is_empty() {
            return Err(ReconciliationError::ReasonEmpty);
        }
        if self.reason.chars().count() > MAX_RECONCILIATION_REASON_CHARS {
            return Err(ReconciliationError::ReasonTooLong);
        }
        if !crate::approval::sql_is_reviewable(&self.reason) || self.reason.chars().any(char::is_control) {
            return Err(ReconciliationError::ReasonHasControlCharacter);
        }
        let user = self.operator_os_user.trim();
        if user.is_empty()
            || user.chars().count() > MAX_OPERATOR_CHARS
            || !crate::approval::sql_is_reviewable(user)
            || user.chars().any(char::is_control)
        {
            return Err(ReconciliationError::OperatorInvalid);
        }
        if environment == Environment::Prod
            && self.claim.clears_the_block()
            && self.evidence == AuditReconciliationEvidence::None
        {
            return Err(ReconciliationError::EvidenceRequired);
        }
        Ok(())
    }
}

impl AuditEvent {
    pub fn reconciliation(
        principal: Principal,
        connection: AuditConnectionContext,
        reconciliation: AuditReconciliation,
    ) -> Result<Self, ReconciliationError> {
        if !matches!(principal, Principal::Human { .. }) {
            return Err(ReconciliationError::NotAPerson);
        }
        reconciliation.validate(connection.environment)?;
        Ok(Self {
            timestamp: Utc::now(),
            operation_id: Uuid::new_v4(),
            batch_id: None,
            phase: AuditRecordPhase::Outcome,
            principal,
            connection_id: connection.id,
            connection_name: connection.name,
            environment: connection.environment,
            driver_id: connection.driver_id,
            operation_class: AuditOperationClass::Administrative,
            redacted_sql: "[NOT_APPLICABLE]".into(),
            sql_hash: hex::encode(Sha256::digest(RULE.as_bytes())),
            targets: Vec::new(),
            decision_rule: RULE.into(),
            approval_outcome: AuditApprovalOutcome::NotRequired,
            preview_state: AuditPreviewState::NotRequested,
            terminal_status: AuditTerminalStatus::Succeeded,
            transaction_outcome: AuditTransactionOutcome::NotApplicable,
            error_category: None,
            error: None,
            rows_affected: None,
            rows_skipped: None,
            duration_ms: None,
            transport_attempt: None,
            administrative_action: None,
            reconciliation: Some(reconciliation),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(environment: Environment) -> AuditConnectionContext {
        AuditConnectionContext {
            id: Uuid::from_u128(7),
            name: "db".into(),
            environment,
            driver_id: "postgres".into(),
        }
    }

    fn claim(reason: &str, evidence: AuditReconciliationEvidence) -> AuditReconciliation {
        AuditReconciliation::new(
            Uuid::from_u128(1),
            AuditReconciliationClaim::Committed,
            reason,
            evidence,
            "operator".into(),
        )
    }

    fn agent() -> Principal {
        Principal::Agent {
            token: "t".into(),
            client: None,
            model: None,
        }
    }

    #[test]
    fn an_agent_can_never_resolve_an_outcome() {
        let event = AuditEvent::reconciliation(
            agent(),
            context(Environment::Dev),
            claim("checked", AuditReconciliationEvidence::None),
        );
        assert_eq!(event.unwrap_err(), ReconciliationError::NotAPerson);
    }

    #[test]
    fn the_reason_is_required_trimmed_bounded_and_free_of_control_characters() {
        let bad =
            |reason: &str| claim(reason, AuditReconciliationEvidence::ServerCheckedManually).validate(Environment::Dev);
        assert_eq!(bad("   "), Err(ReconciliationError::ReasonEmpty));
        assert_eq!(bad(&"x".repeat(1001)), Err(ReconciliationError::ReasonTooLong));
        assert_eq!(bad("line\nbreak"), Err(ReconciliationError::ReasonHasControlCharacter));
        assert_eq!(bad("  fine  "), Ok(()));
        assert_eq!(claim("  fine  ", AuditReconciliationEvidence::None).reason, "fine");
    }

    #[test]
    fn production_needs_evidence_to_clear_the_block_but_not_to_stay_unknown() {
        let committed = claim("checked", AuditReconciliationEvidence::None);
        assert_eq!(
            committed.validate(Environment::Prod),
            Err(ReconciliationError::EvidenceRequired)
        );
        assert_eq!(committed.validate(Environment::Dev), Ok(()));
        let unknown = AuditReconciliation {
            claim: AuditReconciliationClaim::StillUnknown,
            ..committed
        };
        assert_eq!(unknown.validate(Environment::Prod), Ok(()));
    }

    #[test]
    fn the_record_is_one_succeeded_administrative_outcome_without_sql() {
        let event = AuditEvent::reconciliation(
            Principal::human_gui(),
            context(Environment::Dev),
            claim("checked", AuditReconciliationEvidence::ServerCheckedManually),
        )
        .unwrap();
        assert_eq!(event.phase, AuditRecordPhase::Outcome);
        assert_eq!(event.terminal_status, AuditTerminalStatus::Succeeded);
        assert_eq!(event.operation_class, AuditOperationClass::Administrative);
        assert_eq!(event.redacted_sql, "[NOT_APPLICABLE]");
        assert_eq!(event.connection_id, Uuid::from_u128(7));
    }

    #[test]
    fn invisible_or_reordering_characters_and_odd_operator_names_are_refused() {
        let hidden = claim(
            "looks fine\u{202E}evil",
            AuditReconciliationEvidence::ServerCheckedManually,
        );
        assert_eq!(
            hidden.validate(Environment::Dev),
            Err(ReconciliationError::ReasonHasControlCharacter)
        );
        let mut nameless = claim("fine", AuditReconciliationEvidence::ServerCheckedManually);
        nameless.operator_os_user = "  ".into();
        assert_eq!(
            nameless.validate(Environment::Dev),
            Err(ReconciliationError::OperatorInvalid)
        );
        nameless.operator_os_user = "x".repeat(257);
        assert_eq!(
            nameless.validate(Environment::Dev),
            Err(ReconciliationError::OperatorInvalid)
        );
    }
}
