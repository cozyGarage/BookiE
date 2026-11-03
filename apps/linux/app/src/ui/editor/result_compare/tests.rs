use std::sync::Arc;

use bookie_app_services::result_compare::{CompareError, CompareMode, CompareSummary};
use bookie_core::{ColumnInfo, QueryResult, Value};

use super::super::{StatementIdentity, StatementOutcome, StatementOutcomeKind};
use super::selection::{can_compare, candidates, key_choices};
use super::summary::{differing_rows, failure_message, summary_text};

pub(super) fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "text".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

pub(super) fn result(names: &[&str], rows: usize, truncated: bool) -> Arc<QueryResult> {
    Arc::new(QueryResult {
        columns: names.iter().map(|name| column(name)).collect(),
        rows: (0..rows)
            .map(|index| names.iter().map(|_| Value::Int(index as i64)).collect::<Vec<_>>())
            .collect(),
        truncated,
    })
}

pub(super) fn outcome(preview: &str, kind: StatementOutcomeKind) -> StatementOutcome {
    StatementOutcome {
        identity: StatementIdentity::default(),
        sql_preview: preview.into(),
        elapsed_ms: 0,
        kind,
        pinned: false,
    }
}

pub(super) fn rows_outcome(preview: &str, sets: Vec<Arc<QueryResult>>) -> StatementOutcome {
    outcome(preview, StatementOutcomeKind::Rows(sets))
}

pub(super) fn summary(key_columns: usize) -> CompareSummary {
    CompareSummary {
        mode: if key_columns == 0 {
            CompareMode::WholeRow
        } else {
            CompareMode::Keyed
        },
        key_columns,
        rows_left: 10,
        rows_right: 9,
        equal: 7,
        added: 1,
        removed: 2,
        changed: 3,
        unknown_left: 1,
        unknown_right: 0,
        differences_capped: false,
    }
}

#[test]
fn every_complete_row_set_is_a_candidate_with_its_row_count() {
    let outcomes = [
        rows_outcome("SELECT a", vec![result(&["id"], 3, false)]),
        rows_outcome("SELECT b", vec![result(&["id"], 1, false), result(&["id"], 2, false)]),
    ];
    let found = candidates(&outcomes);
    assert_eq!(found.len(), 3);
    assert!(found[0].label.contains("3 rows"));
    assert!(found[0].label.starts_with("SELECT a"));
    assert!(found[1].label.contains("#1") && found[2].label.contains("#2"));
}

#[test]
fn truncated_empty_column_and_non_grid_outcomes_are_not_candidates() {
    let outcomes = [
        rows_outcome("SELECT t", vec![result(&["id"], 3, true)]),
        rows_outcome("SELECT none", vec![result(&[], 0, false)]),
        outcome("SELECT failed", StatementOutcomeKind::NotRun),
    ];
    assert!(candidates(&outcomes).is_empty());
}

#[test]
fn compare_needs_two_candidates() {
    let one = [rows_outcome("SELECT a", vec![result(&["id"], 1, false)])];
    assert!(!can_compare(one.iter()));
    let two = [
        rows_outcome("SELECT a", vec![result(&["id"], 1, false)]),
        rows_outcome("SELECT b", vec![result(&["id"], 1, false)]),
    ];
    assert!(can_compare(two.iter()));
}

#[test]
fn key_choices_are_the_shared_column_names_and_refuse_disjoint_results() {
    let outcomes = [
        rows_outcome("SELECT a", vec![result(&["id", "name"], 1, false)]),
        rows_outcome("SELECT b", vec![result(&["name", "id", "extra"], 1, false)]),
        rows_outcome("SELECT c", vec![result(&["other"], 1, false)]),
    ];
    let found = candidates(&outcomes);
    assert_eq!(key_choices(&found[0], &found[1]).unwrap(), vec!["id", "name"]);
    assert_eq!(
        key_choices(&found[0], &found[2]).unwrap_err(),
        CompareError::NoCommonColumns
    );
}

#[test]
fn a_long_multi_line_preview_is_flattened_and_shortened_in_the_label() {
    let long = format!("SELECT\n{}", "x".repeat(200));
    let found = candidates(&[rows_outcome(&long, vec![result(&["id"], 1, false)])]);
    assert!(!found[0].label.contains('\n'));
    assert!(found[0].label.chars().count() < 80);
}

#[test]
fn the_summary_lists_changed_only_for_keyed_comparisons() {
    let whole = summary_text(&summary(0));
    assert!(whole.contains("Added 1") && whole.contains("Removed 2") && !whole.contains("Changed"));
    let keyed = summary_text(&summary(1));
    assert!(keyed.contains("Changed 3") && keyed.contains("Not comparable 1"));
    assert_eq!(differing_rows(&summary(1)), 6);
}

#[test]
fn every_failure_has_a_distinct_message() {
    let errors = [
        CompareError::Cancelled,
        CompareError::IncompleteInput,
        CompareError::NoCommonColumns,
        CompareError::AmbiguousColumns,
        CompareError::UnknownKeyColumn,
        CompareError::UnreadableKey,
        CompareError::DuplicateKey,
        CompareError::TooManyRows,
        CompareError::Source,
    ];
    let messages: std::collections::HashSet<String> = errors.iter().map(failure_message).collect();
    assert_eq!(messages.len(), errors.len());
}

fn pages_request(keys: &[&str]) -> super::editor::CompareRequest {
    let outcomes = [
        rows_outcome("SELECT a", vec![result(&["id", "name"], 3, false)]),
        rows_outcome("SELECT b", vec![result(&["id", "name"], 2, false)]),
    ];
    let found = candidates(&outcomes);
    super::editor::CompareRequest::new(
        found[0].clone(),
        found[1].clone(),
        keys.iter().map(|key| (*key).to_string()).collect(),
    )
}

fn context() -> bookie_policy::AuditConnectionContext {
    bookie_policy::AuditConnectionContext {
        id: uuid::Uuid::new_v4(),
        name: "analytics".into(),
        environment: bookie_core::Environment::Prod,
        driver_id: "postgres".into(),
    }
}

fn journal_events(service: &crate::services::database_service::DatabaseService) -> Vec<serde_json::Value> {
    let path = service.audit_journal_path_for_test();
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["event"].clone())
        .collect()
}

#[tokio::test]
async fn a_comparison_writes_its_intent_before_reading_and_its_outcome_after() {
    let service = Arc::new(crate::services::database_service::DatabaseService::new_isolated());
    let request = pages_request(&["id"]);
    let report = super::editor::run_audited(service.clone(), context(), &request)
        .await
        .map_err(|_| "comparison failed")
        .unwrap();
    assert_eq!(report.summary.removed, 1);
    let events = journal_events(&service);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["phase"], "intent");
    assert_eq!(events[1]["phase"], "outcome");
    assert_eq!(events[1]["terminal_status"], "succeeded");
    assert_eq!(events[1]["administrative_action"]["affected_count"], 1);
    assert_eq!(events[0]["operation_id"], events[1]["operation_id"]);
    let text = serde_json::to_string(&events).unwrap();
    assert!(!text.contains("SELECT a") && !text.contains("SELECT b"));
}

#[tokio::test]
async fn a_cancelled_comparison_records_a_cancelled_outcome() {
    let service = Arc::new(crate::services::database_service::DatabaseService::new_isolated());
    let request = pages_request(&[]);
    request.ticket.cancel().cancel();
    let failure = super::editor::run_audited(service.clone(), context(), &request)
        .await
        .err();
    assert!(matches!(
        failure,
        Some(super::editor::CompareFailure::Compare(CompareError::Cancelled))
    ));
    let events = journal_events(&service);
    assert_eq!(events[1]["terminal_status"], "cancelled");
}

#[test]
fn can_compare_agrees_with_the_candidate_list_and_ignores_ineligible_sets() {
    let mixed = [
        rows_outcome("SELECT t", vec![result(&["id"], 3, true), result(&["id"], 1, false)]),
        outcome("SELECT failed", StatementOutcomeKind::NotRun),
    ];
    assert_eq!(candidates(&mixed).len(), 1);
    assert!(!can_compare(mixed.iter()));
    let pair = [rows_outcome(
        "SELECT p",
        vec![result(&["id"], 1, false), result(&["id"], 2, false)],
    )];
    assert_eq!(candidates(&pair).len(), 2);
    assert!(can_compare(pair.iter()));
}
