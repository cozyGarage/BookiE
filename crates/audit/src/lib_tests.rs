use bookie_policy::{AuditOperationClass, AuditRecordPhase, AuditSink, AuditTerminalStatus, Principal};
use bookie_storage::{AuditJournal, sample_event};
use uuid::Uuid;

use super::*;

async fn run_args(args: &[&str], journal: &std::path::Path) -> (i32, String, String) {
    let arguments: Vec<String> = args.iter().map(|text| text.to_string()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&arguments, Some(journal.to_path_buf()), &mut out, &mut err).await;
    (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
}

async fn global_unknown(path: &std::path::Path) -> Uuid {
    let journal = AuditJournal::open_validated(path.to_path_buf()).unwrap();
    let mut intent = sample_event(Principal::human_gui());
    intent.connection_id = Uuid::nil();
    intent.operation_class = AuditOperationClass::Mutation;
    intent.phase = AuditRecordPhase::Intent;
    intent.terminal_status = AuditTerminalStatus::Pending;
    intent.redacted_sql = "UPDATE t SET a = ?".into();
    journal.record(intent.clone()).await.unwrap();
    let mut unknown = intent.clone();
    unknown.phase = AuditRecordPhase::Outcome;
    unknown.terminal_status = AuditTerminalStatus::Unknown;
    unknown.error = Some("tunnel dropped mid-COMMIT".into());
    journal.record(unknown).await.unwrap();
    intent.operation_id
}

#[tokio::test]
async fn a_global_unknown_is_listed_and_lifted_by_a_stated_decision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.jsonl");
    let operation_id = global_unknown(&path).await;

    let (code, out, _) = run_args(&["list"], &path).await;
    assert_eq!(code, 0);
    assert!(out.contains(&operation_id.to_string()), "{out}");
    assert!(out.contains("global block"), "{out}");
    assert!(out.contains("tunnel dropped mid-COMMIT"), "{out}");

    let id = operation_id.to_string();
    let (code, _, err) = run_args(&["resolve", &id, "committed", "--reason", "checked by hand"], &path).await;
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("lifts the write block"), "{err}");
    let (_, out, _) = run_args(&["list"], &path).await;
    assert!(out.contains("No write outcome"), "{out}");
}

#[tokio::test]
async fn bad_arguments_a_missing_reason_and_stale_ids_are_refused_without_touching_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.jsonl");
    let operation_id = global_unknown(&path).await.to_string();

    for args in [
        vec![],
        vec!["unknown-command"],
        vec!["resolve"],
        vec!["resolve", "not-a-uuid", "committed", "--reason", "x"],
        vec!["resolve", &operation_id, "maybe", "--reason", "x"],
        vec!["resolve", &operation_id, "committed"],
        vec!["resolve", &operation_id, "committed", "--reason"],
        vec!["resolve", &operation_id, "committed", "--reason", "x", "--bogus"],
    ] {
        let (code, _, _) = run_args(&args, &path).await;
        assert_eq!(code, 2, "{args:?}");
    }
    let stale = Uuid::new_v4().to_string();
    let (code, _, err) = run_args(&["resolve", &stale, "committed", "--reason", "x"], &path).await;
    assert_eq!(code, 1, "{err}");
    let (code, _, err) = run_args(&["resolve", &operation_id, "committed", "--reason", "   "], &path).await;
    assert_eq!(code, 1, "{err}");
    let (_, out, _) = run_args(&["list"], &path).await;
    assert!(out.contains(&operation_id), "nothing was decided: {out}");
}

#[tokio::test]
async fn still_unknown_leaves_the_block_and_says_nothing_about_lifting_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.jsonl");
    let operation_id = global_unknown(&path).await.to_string();
    let (code, _, err) = run_args(
        &["resolve", &operation_id, "still-unknown", "--reason", "waiting"],
        &path,
    )
    .await;
    assert_eq!(code, 0, "{err}");
    assert!(!err.contains("lifts"), "{err}");
    let (_, out, _) = run_args(&["list"], &path).await;
    assert!(out.contains(&operation_id));
}
