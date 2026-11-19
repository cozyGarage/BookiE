use bookie_policy::{
    AuditConnectionContext, AuditEvent, AuditReconciliation, AuditReconciliationClaim, AuditReconciliationEvidence,
    AuditSink, Principal,
};
use bookie_storage::AuditJournal;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownOutcome {
    pub operation_id: Uuid,
    pub connection_id: Uuid,
    pub connection_name: String,
    pub environment: String,
    pub when: String,
    pub class: String,
    pub sql: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeDecision {
    Committed,
    RolledBack,
    StillUnknown,
}

pub async fn list_unknown_outcomes(connection_id: Option<Uuid>) -> Result<Vec<UnknownOutcome>, String> {
    let journal = AuditJournal::open_default().map_err(|error| error.to_string())?;
    list_in(&journal, connection_id).await
}

pub async fn list_unknown_outcomes_at(
    journal_path: std::path::PathBuf,
    connection_id: Option<Uuid>,
) -> Result<Vec<UnknownOutcome>, String> {
    let journal = AuditJournal::open_validated(journal_path).map_err(|error| error.to_string())?;
    list_in(&journal, connection_id).await
}

pub async fn resolve_unknown_outcome_at(
    journal_path: std::path::PathBuf,
    operation_id: Uuid,
    decision: OutcomeDecision,
    reason: &str,
    checked_myself: bool,
) -> Result<(), String> {
    let journal = AuditJournal::open_validated(journal_path).map_err(|error| error.to_string())?;
    resolve_in(&journal, operation_id, decision, reason, checked_myself).await
}

pub async fn resolve_unknown_outcome(
    operation_id: Uuid,
    decision: OutcomeDecision,
    reason: &str,
    checked_myself: bool,
) -> Result<(), String> {
    let journal = AuditJournal::open_default().map_err(|error| error.to_string())?;
    resolve_in(&journal, operation_id, decision, reason, checked_myself).await
}

async fn unresolved_events(journal: &AuditJournal) -> Result<Vec<AuditEvent>, String> {
    let mut events = journal.unresolved_unknowns().await.map_err(|error| error.to_string())?;
    events.reverse();
    Ok(events)
}

async fn list_in(journal: &AuditJournal, connection_id: Option<Uuid>) -> Result<Vec<UnknownOutcome>, String> {
    Ok(unresolved_events(journal)
        .await?
        .into_iter()
        .filter(|event| {
            connection_id.is_none_or(|wanted| event.connection_id == wanted || event.connection_id.is_nil())
        })
        .map(|event| UnknownOutcome {
            operation_id: event.operation_id,
            connection_id: event.connection_id,
            connection_name: event.connection_name,
            environment: format!("{:?}", event.environment),
            when: event.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            class: format!("{:?}", event.operation_class),
            sql: event.redacted_sql,
            error: event.error,
        })
        .collect())
}

async fn resolve_in(
    journal: &AuditJournal,
    operation_id: Uuid,
    decision: OutcomeDecision,
    reason: &str,
    checked_myself: bool,
) -> Result<(), String> {
    let original = unresolved_events(journal)
        .await?
        .into_iter()
        .find(|event| event.operation_id == operation_id)
        .ok_or_else(|| "that write is no longer an unresolved unknown outcome".to_string())?;
    let evidence = if checked_myself && decision != OutcomeDecision::StillUnknown {
        AuditReconciliationEvidence::ServerCheckedManually
    } else {
        AuditReconciliationEvidence::None
    };
    let claim = match decision {
        OutcomeDecision::Committed => AuditReconciliationClaim::Committed,
        OutcomeDecision::RolledBack => AuditReconciliationClaim::RolledBack,
        OutcomeDecision::StillUnknown => AuditReconciliationClaim::StillUnknown,
    };
    let event = AuditEvent::reconciliation(
        Principal::human_gui(),
        AuditConnectionContext {
            id: original.connection_id,
            name: original.connection_name,
            environment: original.environment,
            driver_id: original.driver_id,
        },
        AuditReconciliation::new(operation_id, claim, reason, evidence, operator_name()),
    )
    .map_err(|error| error.to_string())?;
    journal.record(event).await.map_err(|error| error.to_string())
}

fn operator_name() -> String {
    let uid = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| process_uid(&status));
    let passwd = std::fs::read_to_string("/etc/passwd").ok();
    match (uid, passwd) {
        (Some(uid), Some(passwd)) => name_for_uid(&passwd, uid).unwrap_or_else(|| format!("uid {uid}")),
        (Some(uid), None) => format!("uid {uid}"),
        _ => "unknown".into(),
    }
}

fn process_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().nth(1))
        .and_then(|effective| effective.parse().ok())
}

fn name_for_uid(passwd: &str, uid: u32) -> Option<String> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let _password = fields.next()?;
        (fields.next()?.parse::<u32>().ok()? == uid).then(|| name.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_policy::{AuditOperationClass, AuditRecordPhase, AuditTerminalStatus};

    fn write(connection_id: Uuid, phase: AuditRecordPhase, status: AuditTerminalStatus) -> AuditEvent {
        let mut event = bookie_storage::sample_event(Principal::human_gui());
        event.connection_id = connection_id;
        event.connection_name = "orders db".into();
        event.operation_class = AuditOperationClass::Mutation;
        event.phase = phase;
        event.terminal_status = status;
        event
    }

    async fn journal_with_unknown(dir: &tempfile::TempDir) -> (AuditJournal, Uuid, Uuid) {
        let journal = AuditJournal::open_validated(dir.path().join("audit.jsonl")).unwrap();
        let connection_id = Uuid::new_v4();
        let intent = write(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
        journal.record(intent.clone()).await.unwrap();
        let mut unknown = intent.clone();
        unknown.phase = AuditRecordPhase::Outcome;
        unknown.terminal_status = AuditTerminalStatus::Unknown;
        unknown.error = Some("lost the acknowledgement".into());
        journal.record(unknown).await.unwrap();
        (journal, connection_id, intent.operation_id)
    }

    #[tokio::test]
    async fn an_unknown_write_is_listed_with_its_record_and_gone_once_decided() {
        let dir = tempfile::tempdir().unwrap();
        let (journal, connection_id, operation_id) = journal_with_unknown(&dir).await;
        let listed = list_in(&journal, Some(connection_id)).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].operation_id, operation_id);
        assert_eq!(listed[0].connection_name, "orders db");
        assert_eq!(listed[0].error.as_deref(), Some("lost the acknowledgement"));
        assert!(list_in(&journal, Some(Uuid::new_v4())).await.unwrap().is_empty());

        resolve_in(
            &journal,
            operation_id,
            OutcomeDecision::Committed,
            "checked the table by hand",
            true,
        )
        .await
        .unwrap();
        assert!(list_in(&journal, None).await.unwrap().is_empty());
        let intent = write(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
        journal
            .record(intent)
            .await
            .expect("the connection accepts writes again");
    }

    #[tokio::test]
    async fn still_unknown_keeps_it_listed_and_a_missing_reason_or_stale_id_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (journal, _connection_id, operation_id) = journal_with_unknown(&dir).await;
        resolve_in(
            &journal,
            operation_id,
            OutcomeDecision::StillUnknown,
            "waiting for the DBA",
            false,
        )
        .await
        .unwrap();
        assert_eq!(list_in(&journal, None).await.unwrap().len(), 1);
        assert!(
            resolve_in(&journal, operation_id, OutcomeDecision::Committed, "  ", true)
                .await
                .is_err()
        );
        assert!(
            resolve_in(&journal, Uuid::new_v4(), OutcomeDecision::Committed, "x", true)
                .await
                .is_err()
        );
        assert_eq!(list_in(&journal, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn production_needs_the_declaration_and_an_unchecked_decision_records_no_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let journal = AuditJournal::open_validated(dir.path().join("audit.jsonl")).unwrap();
        let connection_id = Uuid::new_v4();
        let mut intent = write(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
        intent.environment = bookie_core::Environment::Prod;
        journal.record(intent.clone()).await.unwrap();
        let mut unknown = intent.clone();
        unknown.phase = AuditRecordPhase::Outcome;
        unknown.terminal_status = AuditTerminalStatus::Unknown;
        journal.record(unknown).await.unwrap();

        let refused = resolve_in(&journal, intent.operation_id, OutcomeDecision::Committed, "fine", false).await;
        assert!(refused.is_err(), "a production claim needs the operator's declaration");
        assert_eq!(list_in(&journal, None).await.unwrap().len(), 1);
        resolve_in(&journal, intent.operation_id, OutcomeDecision::Committed, "fine", true)
            .await
            .unwrap();
        let recorded = journal.recent(1).await.unwrap().remove(0).reconciliation.unwrap();
        assert_eq!(recorded.evidence, AuditReconciliationEvidence::ServerCheckedManually);
    }

    #[test]
    fn the_operator_is_the_effective_os_user_and_not_an_environment_value() {
        let status = "Name:\tbookie\nUid:\t1000\t1001\t1001\t1001\n";
        assert_eq!(process_uid(status), Some(1001));
        assert_eq!(process_uid("Name:\tx\n"), None);
        let passwd = "root:x:0:0:root:/root:/bin/bash\ntrung:x:1001:1001::/home/trung:/bin/zsh\n";
        assert_eq!(name_for_uid(passwd, 1001).as_deref(), Some("trung"));
        assert_eq!(name_for_uid(passwd, 7), None);
        assert!(!operator_name().is_empty());
    }
}
