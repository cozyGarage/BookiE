use super::*;
use bookie_core::Environment;
use bookie_policy::{
    AuditConnectionContext, AuditReconciliation, AuditReconciliationClaim, AuditReconciliationEvidence, Principal,
};
use tempfile::TempDir;

fn write_intent(connection_id: Uuid) -> AuditEvent {
    let mut event = sample_event(Principal::human_gui());
    event.connection_id = connection_id;
    event.operation_class = AuditOperationClass::Mutation;
    event.phase = AuditRecordPhase::Intent;
    event.terminal_status = AuditTerminalStatus::Pending;
    event
}

fn outcome_of(intent: &AuditEvent, status: AuditTerminalStatus) -> AuditEvent {
    let mut event = intent.clone();
    event.phase = AuditRecordPhase::Outcome;
    event.terminal_status = status;
    event
}

async fn journal_with_unknown_write(dir: &TempDir) -> (AuditJournal, Uuid, Uuid) {
    let journal = AuditJournal::open_validated(dir.path().join("audit.jsonl")).unwrap();
    let connection_id = Uuid::new_v4();
    let intent = write_intent(connection_id);
    journal.record(intent.clone()).await.unwrap();
    journal
        .record(outcome_of(&intent, AuditTerminalStatus::Unknown))
        .await
        .unwrap();
    (journal, connection_id, intent.operation_id)
}

fn resolution(
    connection_id: Uuid,
    operation_id: Uuid,
    claim: AuditReconciliationClaim,
    principal: Principal,
) -> Result<AuditEvent, bookie_policy::ReconciliationError> {
    AuditEvent::reconciliation(
        principal,
        AuditConnectionContext {
            id: connection_id,
            name: "test".into(),
            environment: Environment::Dev,
            driver_id: "postgres".into(),
        },
        AuditReconciliation::new(
            operation_id,
            claim,
            "checked the table by hand",
            AuditReconciliationEvidence::ServerCheckedManually,
            "operator".into(),
        ),
    )
}

#[tokio::test]
async fn a_committed_claim_lifts_the_block_and_survives_a_restart() {
    let dir = TempDir::new().unwrap();
    let (journal, connection_id, operation_id) = journal_with_unknown_write(&dir).await;
    assert!(matches!(
        journal.record(write_intent(connection_id)).await,
        Err(AuditError::UnresolvedWrite)
    ));

    let claim = resolution(
        connection_id,
        operation_id,
        AuditReconciliationClaim::Committed,
        Principal::human_gui(),
    )
    .unwrap();
    journal.record(claim).await.unwrap();

    journal
        .record(write_intent(connection_id))
        .await
        .expect("the connection accepts writes once its unknown outcome is decided");
    assert!(journal.verify_chain().await.unwrap() >= 4);
    let reopened = AuditJournal::open_validated(dir.path().join("audit.jsonl")).unwrap();
    assert!(!reopened.recovery().has_unresolved_writes());
}

#[tokio::test]
async fn still_unknown_keeps_the_block_and_can_be_recorded_again() {
    let dir = TempDir::new().unwrap();
    let (journal, connection_id, operation_id) = journal_with_unknown_write(&dir).await;
    for _ in 0..2 {
        let note = resolution(
            connection_id,
            operation_id,
            AuditReconciliationClaim::StillUnknown,
            Principal::human_gui(),
        )
        .unwrap();
        journal.record(note).await.unwrap();
    }
    assert!(matches!(
        journal.record(write_intent(connection_id)).await,
        Err(AuditError::UnresolvedWrite)
    ));
}

#[tokio::test]
async fn a_second_decision_an_unknown_target_or_another_connection_is_refused() {
    let dir = TempDir::new().unwrap();
    let (journal, connection_id, operation_id) = journal_with_unknown_write(&dir).await;
    let committed = resolution(
        connection_id,
        operation_id,
        AuditReconciliationClaim::Committed,
        Principal::human_gui(),
    )
    .unwrap();
    journal.record(committed).await.unwrap();

    let again = resolution(
        connection_id,
        operation_id,
        AuditReconciliationClaim::RolledBack,
        Principal::human_gui(),
    )
    .unwrap();
    assert!(matches!(journal.record(again).await, Err(AuditError::Persistence(_))));

    let nothing = resolution(
        connection_id,
        Uuid::new_v4(),
        AuditReconciliationClaim::Committed,
        Principal::human_gui(),
    )
    .unwrap();
    assert!(matches!(journal.record(nothing).await, Err(AuditError::Persistence(_))));

    let dir = TempDir::new().unwrap();
    let (journal, _connection_id, operation_id) = journal_with_unknown_write(&dir).await;
    let elsewhere = resolution(
        Uuid::new_v4(),
        operation_id,
        AuditReconciliationClaim::Committed,
        Principal::human_gui(),
    )
    .unwrap();
    assert!(matches!(
        journal.record(elsewhere).await,
        Err(AuditError::Persistence(_))
    ));
}

#[tokio::test]
async fn a_write_still_in_flight_cannot_be_resolved() {
    let dir = TempDir::new().unwrap();
    let journal = AuditJournal::open_validated(dir.path().join("audit.jsonl")).unwrap();
    let connection_id = Uuid::new_v4();
    let intent = write_intent(connection_id);
    journal.record(intent.clone()).await.unwrap();
    let claim = resolution(
        connection_id,
        intent.operation_id,
        AuditReconciliationClaim::Committed,
        Principal::human_gui(),
    )
    .unwrap();
    assert!(matches!(journal.record(claim).await, Err(AuditError::WriteInFlight)));
}

#[tokio::test]
async fn a_forged_agent_record_is_refused_and_fails_chain_verification_if_written_anyway() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("audit.jsonl");
    let (journal, connection_id, operation_id) = journal_with_unknown_write(&dir).await;
    let mut forged = resolution(
        connection_id,
        operation_id,
        AuditReconciliationClaim::Committed,
        Principal::human_gui(),
    )
    .unwrap();
    forged.principal = Principal::Agent {
        token: "t".into(),
        client: None,
        model: None,
    };
    assert!(matches!(
        journal.record(forged.clone()).await,
        Err(AuditError::Persistence(_))
    ));

    let tail = {
        let mut file = std::fs::File::open(&path).unwrap();
        scan_records(&mut file, JournalState::genesis(), 0, false, None)
            .unwrap()
            .0
            .tail
    };
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    append_to_file(&mut file, tail, forged, true).unwrap();
    assert!(
        journal.verify_chain().await.is_err(),
        "a record the journal would never have admitted is corruption"
    );
}
