use super::*;
use tablepro_policy::Principal;
use tempfile::TempDir;

#[tokio::test]
async fn append_and_verify() {
    let dir = TempDir::new().unwrap();
    let journal = AuditJournal::open_validated(dir.path().join("audit.jsonl")).unwrap();
    journal.append(sample_event(Principal::human_gui())).await.unwrap();
    journal
        .append(sample_event(Principal::Agent {
            token: "abc".into(),
            client: Some("cursor".into()),
            model: None,
        }))
        .await
        .unwrap();
    assert_eq!(journal.verify_chain().await.unwrap(), 2);
    assert_eq!(journal.recent(10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn recent_returns_the_requested_tail_in_chronological_order() {
    let dir = TempDir::new().unwrap();
    let journal = AuditJournal::open_validated(dir.path().join("audit.jsonl")).unwrap();
    let operation_ids = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
    for operation_id in operation_ids {
        let mut event = sample_event(Principal::human_gui());
        event.operation_id = operation_id;
        journal.append(event).await.unwrap();
    }

    let recent = journal.recent(2).await.unwrap();
    assert_eq!(
        recent.iter().map(|event| event.operation_id).collect::<Vec<_>>(),
        operation_ids[1..].to_vec()
    );
    assert!(journal.recent(0).await.unwrap().is_empty());
}

#[tokio::test]
async fn recent_preserves_a_complete_record_without_a_line_terminator() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("audit.jsonl");
    let journal = AuditJournal::open_validated(path.clone()).unwrap();
    let event = sample_event(Principal::human_gui());
    let operation_id = event.operation_id;
    journal.append(event).await.unwrap();
    let mut bytes = tokio::fs::read(&path).await.unwrap();
    assert_eq!(bytes.pop(), Some(b'\n'));
    tokio::fs::write(&path, &bytes).await.unwrap();

    let recent = journal.recent(1).await.unwrap();

    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].operation_id, operation_id);
    assert_eq!(tokio::fs::read(path).await.unwrap(), bytes);
}

#[test]
fn verification_starts_at_the_beginning_of_the_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("audit.jsonl");
    let event = sample_event(Principal::human_gui());
    let operation_id = event.operation_id;
    append_locked(&path, event, false, None).unwrap();
    let mut file = open_file(&path).unwrap().file;
    file.seek(SeekFrom::End(0)).unwrap();

    let (_, recent) = verify_locked_with_recent(&mut file, false, Some(1)).unwrap();

    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].operation_id, operation_id);
}

#[tokio::test]
async fn a_pending_write_is_distinct_from_an_unknown_outcome() {
    let dir = TempDir::new().unwrap();
    let journal = AuditJournal::open_validated(dir.path().join("audit.jsonl")).unwrap();
    let connection_id = Uuid::new_v4();
    let first = write_event(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
    journal.record(first.clone()).await.unwrap();
    journal
        .record(write_event(
            Uuid::new_v4(),
            AuditRecordPhase::Intent,
            AuditTerminalStatus::Pending,
        ))
        .await
        .unwrap();

    let competing = write_event(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
    assert!(matches!(
        journal.record(competing).await,
        Err(AuditError::WriteInFlight)
    ));

    journal
        .record(write_event_for(&first, AuditTerminalStatus::Succeeded))
        .await
        .unwrap();
    journal
        .record(write_event(
            connection_id,
            AuditRecordPhase::Intent,
            AuditTerminalStatus::Pending,
        ))
        .await
        .unwrap();

    let unresolved_dir = TempDir::new().unwrap();
    let unresolved = AuditJournal::open_validated(unresolved_dir.path().join("audit.jsonl")).unwrap();
    let unknown = write_event(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
    unresolved.record(unknown.clone()).await.unwrap();
    unresolved
        .record(write_event_for(&unknown, AuditTerminalStatus::Unknown))
        .await
        .unwrap();
    let next = write_event(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
    assert!(matches!(
        unresolved.record(next).await,
        Err(AuditError::UnresolvedWrite)
    ));
}

#[tokio::test]
async fn a_rollback_is_never_blocked_by_an_unresolved_write_but_a_commit_still_is() {
    let dir = TempDir::new().unwrap();
    let journal = AuditJournal::open_validated(dir.path().join("audit.jsonl")).unwrap();
    let connection_id = Uuid::new_v4();
    let unknown = write_event(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
    journal.record(unknown.clone()).await.unwrap();
    journal
        .record(write_event_for(&unknown, AuditTerminalStatus::Unknown))
        .await
        .unwrap();

    let mut rollback = write_event(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
    rollback.operation_class = AuditOperationClass::TransactionRollback;
    journal
        .record(rollback.clone())
        .await
        .expect("ending a transaction on a blocked connection is allowed");
    journal
        .record(write_event_for(&rollback, AuditTerminalStatus::Succeeded))
        .await
        .unwrap();

    let mut commit = rollback.clone();
    commit.operation_id = Uuid::new_v4();
    commit.operation_class = AuditOperationClass::TransactionCommit;
    assert!(matches!(journal.record(commit).await, Err(AuditError::UnresolvedWrite)));
    let next = write_event(connection_id, AuditRecordPhase::Intent, AuditTerminalStatus::Pending);
    assert!(
        matches!(journal.record(next).await, Err(AuditError::UnresolvedWrite)),
        "the earlier outcome stays unresolved after the rollback"
    );
}

fn write_event(connection_id: Uuid, phase: AuditRecordPhase, status: AuditTerminalStatus) -> AuditEvent {
    let mut event = sample_event(Principal::human_gui());
    event.connection_id = connection_id;
    event.operation_class = AuditOperationClass::Mutation;
    event.phase = phase;
    event.terminal_status = status;
    event
}

fn write_event_for(intent: &AuditEvent, status: AuditTerminalStatus) -> AuditEvent {
    let mut event = intent.clone();
    event.phase = AuditRecordPhase::Outcome;
    event.terminal_status = status;
    event
}

#[test]
fn durable_append_fails_when_data_sync_fails() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("audit.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(path).unwrap();
    let error = append_to_file_with_sync(
        &mut file,
        JournalTail {
            seq: 0,
            hash: GENESIS_HASH.to_string(),
        },
        sample_event(Principal::human_gui()),
        true,
        |_| Err(std::io::Error::other("injected sync failure")),
    );

    assert!(error.is_err());
}

#[test]
fn durable_creation_propagates_parent_sync_failure() {
    let dir = TempDir::new().unwrap();
    let parent = dir.path().join("journal");
    std::fs::create_dir(&parent).unwrap();
    let path = parent.join("audit.jsonl");
    let opened = open_file(&path).unwrap();
    assert!(opened.created);

    let moved_parent = dir.path().join("moved-journal");
    std::fs::rename(&parent, moved_parent).unwrap();

    assert!(sync_created_file_and_parent(&opened, &path).is_err());
}

#[tokio::test]
async fn a_record_with_non_canonical_whitespace_in_its_event_json_still_verifies() {
    // Simulates a record written by a differently-formatted (but
    // semantically identical) serialization of AuditEvent -- e.g.
    // a future field-order or whitespace change. Hashing the raw
    // on-disk bytes must still validate it; re-serializing the
    // parsed struct to recompute the hash would not reproduce
    // these exact bytes and would wrongly reject it as tampered.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("audit.jsonl");
    let event = sample_event(Principal::human_gui());
    let canonical = serde_json::to_string(&event).unwrap();
    let padded = canonical.replacen(':', ":  ", 1);
    assert_ne!(padded, canonical);
    let hash = record_hash(GENESIS_HASH, 1, padded.as_bytes());
    let line = format!(r#"{{"seq":1,"prev_hash":"{GENESIS_HASH}","hash":"{hash}","event":{padded}}}"#);
    tokio::fs::write(&path, format!("{line}\n")).await.unwrap();
    let journal = AuditJournal::open_validated(path).expect("a non-canonical but faithfully hashed record must verify");
    assert!(!journal.recovery().has_unresolved_writes());
}

#[tokio::test]
async fn detects_tamper() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("audit.jsonl");
    let journal = AuditJournal::open_validated(path.clone()).unwrap();
    journal.append(sample_event(Principal::human_gui())).await.unwrap();
    let mut text = tokio::fs::read_to_string(&path).await.unwrap();
    text = text.replace("read_allow", "read_deny");
    tokio::fs::write(&path, text).await.unwrap();
    assert!(journal.verify_chain().await.is_err());
    assert!(AuditJournal::open_validated(path).is_err());
}

#[tokio::test]
async fn recent_with_a_short_limit_still_detects_tampering_in_older_records() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("audit.jsonl");
    let journal = AuditJournal::open_validated(path.clone()).unwrap();
    for _ in 0..3 {
        journal.append(sample_event(Principal::human_gui())).await.unwrap();
    }
    let mut text = tokio::fs::read_to_string(&path).await.unwrap();
    text = text.replacen("read_allow", "read_deny", 1);
    tokio::fs::write(path, text).await.unwrap();

    assert!(journal.recent(1).await.is_err());
}
