use std::sync::Arc;

use bookie_app_services::result_filter::{FilteredResult, filter_into_spool};
use bookie_app_services::result_find::{FindMatch, FindReport};
use bookie_app_services::result_predicate::{ColumnScope, ResultPredicate};
use bookie_app_services::result_scan::{ScanProgress, ScanSource};
use bookie_app_services::stream_result_spool::{StreamResultSpool, StreamResultSpoolManager};
use bookie_core::{ColumnInfo, QueryResult, Value};
use tokio_util::sync::CancellationToken;

use super::cursor::MatchCursor;
use super::derived::{derived_outcome, derived_result, next_generation};
use super::editor::{failure_message, success_message};
use crate::ui::editor::pinned::{MAX_PINNED, PinnedResults};
use crate::ui::editor::stream_pages::page_row_starts_from_counts as page_row_starts;
use crate::ui::editor::stream_pages::{ResidentPageStore, StreamTerminal, StreamedResult};
use crate::ui::editor::{StatementIdentity, StatementOutcomeKind};
use crate::ui::export_dialog::{StreamExportQuery, StreamExportRequest};

fn column(name: &str) -> ColumnInfo {
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

fn report(count: usize, capped: bool, unsearchable: u64) -> FindReport {
    FindReport {
        matches: (0..count)
            .map(|row| FindMatch {
                page: row / 2,
                row: row % 2,
            })
            .collect(),
        capped,
        rows_scanned: 100,
        unsearchable_cells: unsearchable,
    }
}

fn cursor_with(count: usize) -> (MatchCursor, u64) {
    let mut cursor = MatchCursor::default();
    let generation = cursor.begin("needle");
    assert!(cursor.complete(generation, report(count, false, 0)));
    (cursor, generation)
}

#[test]
fn stepping_forward_starts_at_the_first_match_and_wraps() {
    let (mut cursor, _) = cursor_with(3);
    let visited: Vec<_> = (0..4).map(|_| cursor.step(true).unwrap()).collect();
    assert_eq!(visited[0], FindMatch { page: 0, row: 0 });
    assert_eq!(visited[2], FindMatch { page: 1, row: 0 });
    assert_eq!(visited[3], visited[0]);
}

#[test]
fn stepping_backward_starts_at_the_last_match_and_wraps() {
    let (mut cursor, _) = cursor_with(3);
    assert_eq!(cursor.step(false), Some(FindMatch { page: 1, row: 0 }));
    assert_eq!(cursor.step(false), Some(FindMatch { page: 0, row: 1 }));
    assert_eq!(cursor.step(false), Some(FindMatch { page: 0, row: 0 }));
    assert_eq!(cursor.step(false), Some(FindMatch { page: 1, row: 0 }));
}

#[test]
fn stepping_with_no_matches_returns_nothing() {
    let (mut cursor, _) = cursor_with(0);
    assert_eq!(cursor.step(true), None);
    assert_eq!(cursor.status(), "No match");
}

#[test]
fn a_stale_search_result_is_dropped() {
    let mut cursor = MatchCursor::default();
    let first = cursor.begin("a");
    let second = cursor.begin("ab");
    assert!(!cursor.accepts(first));
    assert!(!cursor.complete(first, report(5, false, 0)));
    assert!(cursor.complete(second, report(1, false, 0)));
    assert_eq!(cursor.step(true), Some(FindMatch { page: 0, row: 0 }));
}

#[test]
fn results_are_reused_only_for_the_same_completed_search() {
    let mut cursor = MatchCursor::default();
    let generation = cursor.begin("abc");
    assert!(!cursor.has_results_for("abc"));
    cursor.complete(generation, report(1, false, 0));
    assert!(cursor.has_results_for("abc"));
    assert!(!cursor.has_results_for("abcd"));
    cursor.invalidate();
    assert!(!cursor.has_results_for("abc"));
    assert!(!cursor.accepts(generation));
}

#[test]
fn status_text_reports_position_cap_and_unsearchable_cells() {
    let mut cursor = MatchCursor::default();
    let generation = cursor.begin("x");
    cursor.complete(generation, report(10_000, true, 1));
    assert_eq!(
        cursor.status(),
        "10,000+ matches (list capped) · 1 cell could not be searched"
    );
    cursor.step(true);
    cursor.step(true);
    assert_eq!(cursor.status(), "Match 2 of 10,000+ · 1 cell could not be searched");
    let generation = cursor.begin("y");
    cursor.complete(generation, report(3, false, 2));
    assert_eq!(cursor.status(), "3 matches · 2 cells could not be searched");
    assert_eq!(MatchCursor::default().status(), "");
}

#[test]
fn page_row_starts_are_one_based_running_totals() {
    assert_eq!(page_row_starts(&[3, 2, 4]), vec![1, 4, 6]);
    assert_eq!(page_row_starts(&[]), Vec::<usize>::new());
}

#[test]
fn derived_generations_are_unique_and_far_from_run_generations() {
    let first = next_generation();
    let second = next_generation();
    assert!(second > first);
    assert!(first >= 1 << 62);
}

fn id_column() -> Vec<ColumnInfo> {
    vec![column("id"), column("label")]
}

fn spooled(pages: &[Vec<Vec<Value>>]) -> (StreamResultSpool, Vec<usize>) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let spool = StreamResultSpoolManager::default().create().unwrap();
    for (index, rows) in pages.iter().enumerate() {
        let page = Arc::new(QueryResult {
            columns: id_column(),
            rows: rows.clone().into(),
            truncated: false,
        });
        runtime.block_on(spool.append_page(index as u64, page)).unwrap();
    }
    (spool, pages.iter().map(Vec::len).collect())
}

fn row(id: i64, label: &str) -> Vec<Value> {
    vec![Value::Int(id), Value::Text(label.into())]
}

fn source_result(spool: StreamResultSpool, counts: Vec<usize>) -> StreamedResult {
    let total = counts.iter().sum();
    StreamedResult {
        pages: Vec::new(),
        page_indices: Vec::new(),
        page_row_starts: page_row_starts(&counts),
        retained_rows: 0,
        columns: id_column(),
        total_pages: counts.len() as u64,
        total_rows: total,
        terminal: StreamTerminal::Complete,
        export: Some(StreamExportRequest {
            query: StreamExportQuery {
                driver_id: "postgres".into(),
                suggested_name: "orders".into(),
            },
            columns: id_column(),
            spool: Some(spool.clone()),
            total_rows: total,
        }),
        generation: 7,
        spool: Some(spool),
        resident_pages: ResidentPageStore::default(),
        page_row_counts: counts,
    }
}

fn filtered(source: &StreamedResult, needle: &str) -> FilteredResult {
    let scan = ScanSource::new(
        source.spool.clone().unwrap(),
        source.columns.clone(),
        source.total_rows as u64,
    );
    filter_into_spool(
        &scan,
        ResultPredicate::Contains {
            scope: ColumnScope::Any,
            needle: needle.into(),
        },
        &StreamResultSpoolManager::default(),
        &CancellationToken::new(),
        &ScanProgress::default(),
    )
    .unwrap()
}

#[test]
fn a_derived_result_is_a_complete_spool_backed_result_with_its_own_counts() {
    let (spool, counts) = spooled(&[
        vec![row(1, "apple"), row(2, "pear"), row(3, "apple pie")],
        vec![row(4, "plum"), row(5, "Apple")],
    ]);
    let source = source_result(spool, counts);
    let derived = derived_result(&source, filtered(&source, "apple"), 1 << 62);
    assert!(matches!(derived.terminal, StreamTerminal::Complete));
    assert_eq!((derived.total_rows, derived.total_pages), (3, 1));
    assert_eq!(derived.page_row_counts, vec![3]);
    assert_eq!(derived.page_row_starts, vec![1]);
    assert!(derived.pages.is_empty() && derived.page_indices.is_empty());
    assert_eq!(derived.generation, 1 << 62);
    let export = derived.export.as_ref().unwrap();
    assert_eq!(export.total_rows, 3);
    assert_eq!(export.query.suggested_name, "orders-filtered");
    assert_eq!(export.query.driver_id, "postgres");
    let spool = derived.spool.clone().unwrap();
    assert_ne!(spool.stored_bytes(), source.spool.as_ref().unwrap().stored_bytes());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let page = runtime.block_on(spool.load_page(0, derived.columns.clone())).unwrap();
    let ids: Vec<&Value> = page.rows.iter().map(|row| &row[0]).collect();
    assert_eq!(ids, vec![&Value::Int(1), &Value::Int(3), &Value::Int(5)]);
}

#[test]
fn a_derived_result_without_a_source_export_has_no_export() {
    let (spool, counts) = spooled(&[vec![row(1, "a")]]);
    let mut source = source_result(spool, counts);
    source.export = None;
    let derived = derived_result(&source, filtered(&source, "a"), 1 << 62);
    assert!(derived.export.is_none());
}

#[test]
fn a_derived_outcome_is_pinned_with_a_flat_preview_and_a_new_identity() {
    let (spool, counts) = spooled(&[vec![row(1, "a")]]);
    let source = source_result(spool, counts);
    let identity = StatementIdentity {
        statement_index: 4,
        digest: [9; 32],
    };
    let needle = "line one\nline two ".repeat(10);
    let first = derived_outcome(identity, &source, filtered(&source, "a"), &needle);
    let second = derived_outcome(identity, &source, filtered(&source, "a"), &needle);
    assert!(first.pinned);
    assert_eq!(first.identity.statement_index, 4);
    assert_ne!(first.identity.digest, identity.digest);
    assert_ne!(first.identity.digest, second.identity.digest);
    assert!(first.sql_preview.starts_with("FILTER line one line two"));
    assert!(!first.sql_preview.contains('\n'));
    assert!(first.sql_preview.chars().count() <= "FILTER ".len() + 60);
    let StatementOutcomeKind::Streamed(result) = &first.kind else {
        panic!("derived outcome is streamed")
    };
    let StatementOutcomeKind::Streamed(other) = &second.kind else {
        panic!("derived outcome is streamed")
    };
    assert_ne!(result.generation, other.generation);
}

#[test]
fn adding_a_pin_keeps_the_newest_and_marks_it_pinned() {
    let (spool, counts) = spooled(&[vec![row(1, "a")]]);
    let source = source_result(spool, counts);
    let identity = StatementIdentity::default();
    let mut pins = PinnedResults::default();
    let mut previews = Vec::new();
    for index in 0..MAX_PINNED + 2 {
        let mut outcome = derived_outcome(identity, &source, filtered(&source, "a"), "a");
        outcome.pinned = false;
        outcome.sql_preview = format!("n{index}");
        previews.push(outcome.sql_preview.clone());
        let added = pins.add_pin(outcome);
        assert_eq!(added.evicted, usize::from(index >= MAX_PINNED));
        assert_eq!(added.index, pins.count() - 1);
    }
    let shown = pins.displayed();
    assert_eq!(shown.len(), MAX_PINNED);
    assert!(shown.iter().all(|outcome| outcome.pinned));
    let kept: Vec<_> = shown.iter().map(|outcome| outcome.sql_preview.clone()).collect();
    assert_eq!(kept, previews[previews.len() - MAX_PINNED..].to_vec());
}

#[test]
fn filter_messages_name_the_quota_and_count_rows() {
    use bookie_app_services::result_filter::FilterError;
    use bookie_app_services::result_scan::ScanError;
    use bookie_app_services::stream_result_spool::StreamResultSpoolError;
    let quota = failure_message(&FilterError::Spool(StreamResultSpoolError::QuotaExceeded));
    assert!(quota.contains("temporary storage"));
    assert_eq!(
        failure_message(&FilterError::Scan(ScanError::Cancelled)),
        "Filter cancelled"
    );
    assert_eq!(success_message(1, 0, true), "Showing 1 matching row in a new result");
    assert_eq!(
        success_message(3, 0, false),
        "3 matching rows pinned; they show when the running query finishes"
    );
    assert_eq!(
        success_message(12_345, 2, true),
        "Showing 12,345 matching rows in a new result · 2 cells could not be searched"
    );
}

#[test]
fn a_cell_filter_builds_an_exact_typed_predicate_and_a_short_label() {
    use super::cell_filter::cell_predicate;
    let columns = [column("id"), column("name")];
    let (predicate, label) = cell_predicate(&columns, 1, Value::Text("Ada".into()), false).unwrap();
    assert_eq!(
        predicate,
        ResultPredicate::Equals {
            column: 1,
            value: Value::Text("Ada".into())
        }
    );
    assert_eq!(label, "name = Ada");
    let (predicate, label) = cell_predicate(&columns, 0, Value::Int(7), true).unwrap();
    assert_eq!(
        predicate,
        ResultPredicate::NotEquals {
            column: 0,
            value: Value::Int(7)
        }
    );
    assert_eq!(label, "id ≠ 7");
    let (_, label) = cell_predicate(&columns, 1, Value::Text("x".repeat(80)), false).unwrap();
    assert_eq!(label.chars().count(), "name = ".chars().count() + 24);
}

#[test]
fn a_cell_filter_keeps_null_distinct_from_empty_text_and_refuses_unreadable_values() {
    use super::cell_filter::cell_predicate;
    let columns = [column("id")];
    let (null, _) = cell_predicate(&columns, 0, Value::Null, false).unwrap();
    let prepared = null.prepare(1).unwrap();
    let mut tally = bookie_app_services::result_predicate::CellTally::default();
    assert!(prepared.matches(&[Value::Null], &mut tally));
    assert!(!prepared.matches(&[Value::Text(String::new())], &mut tally));
    assert!(cell_predicate(&columns, 0, Value::Undecodable("x".into()), false).is_none());
    assert!(cell_predicate(&columns, 3, Value::Int(1), false).is_none());
}

#[test]
fn only_a_complete_streamed_result_can_be_filtered() {
    let (spool, counts) = spooled(&[vec![row(1, "a")]]);
    let mut source = source_result(spool, counts);
    assert!(super::editor::can_filter(&source));
    for terminal in [
        StreamTerminal::Loading,
        StreamTerminal::Cancelled,
        StreamTerminal::TimedOut,
    ] {
        source.terminal = terminal;
        assert!(!super::editor::can_filter(&source));
    }
}

#[test]
fn a_panicking_filter_worker_becomes_a_filter_error() {
    use bookie_app_services::result_filter::FilterError;
    let outcome: Result<(), FilterError> = super::editor::guarded(|| panic!("worker failed"));
    assert!(outcome.is_err());
    assert_eq!(failure_message(&outcome.unwrap_err()), "The filter could not finish");
}

#[test]
fn a_find_worker_that_never_answers_is_reported_as_a_failed_search() {
    use bookie_app_services::result_find::FindError;
    let dropped = super::actions::received(Err(async_channel::RecvError));
    assert!(matches!(dropped, Err(FindError::Scan(_))));
    let answered = super::actions::received(Ok(Ok(report(0, false, 0))));
    assert!(answered.is_ok());
}

#[test]
fn an_eviction_is_named_in_the_status_only_when_a_pin_was_closed() {
    use crate::ui::editor::pinned::with_eviction_note;
    assert_eq!(with_eviction_note("Done".into(), 0), "Done");
    assert!(with_eviction_note("Done".into(), 1).contains("oldest pinned result was closed"));
}

#[test]
fn a_running_filter_is_found_by_its_statement_and_cancelled_together() {
    let identity = StatementIdentity {
        statement_index: 41,
        digest: [7; 32],
    };
    let other = StatementIdentity {
        statement_index: 42,
        digest: [8; 32],
    };
    let ticket = Arc::new(super::ticket::FilterTicket::default());
    assert!(!super::ticket::running(identity));
    super::ticket::register(identity, &ticket);
    assert!(super::ticket::running(identity));
    assert!(super::ticket::active(identity).is_some_and(|active| Arc::ptr_eq(&active, &ticket)));
    assert!(super::ticket::active(other).is_none());
    assert!(!super::ticket::running(other));
    super::ticket::cancel(other);
    assert!(!ticket.cancel().is_cancelled());
    super::ticket::cancel(identity);
    assert!(ticket.cancel().is_cancelled());
    ticket.finish(String::new());
    assert!(!super::ticket::running(identity));
    assert!(super::ticket::active(identity).is_none());
}

#[test]
fn a_find_jump_locates_the_row_by_source_index_in_a_reordered_model() {
    use relm4::gtk::gio;
    use relm4::gtk::prelude::*;
    let result = Arc::new(QueryResult {
        columns: id_column(),
        rows: vec![row(1, "a"), row(2, "b"), row(3, "c")].into(),
        truncated: false,
    });
    let store = gio::ListStore::new::<crate::ui::row_object::RowObject>();
    for source in [2_usize, 0, 1] {
        store.append(&crate::ui::row_object::RowObject::from_shared(&result, source, None).unwrap());
    }
    assert_eq!(store.n_items(), 3);
    assert_eq!(super::jump::position_of_source(store.upcast_ref(), 0), Some(1));
    assert_eq!(super::jump::position_of_source(store.upcast_ref(), 2), Some(0));
    assert_eq!(super::jump::position_of_source(store.upcast_ref(), 1), Some(2));
    assert_eq!(super::jump::position_of_source(store.upcast_ref(), 9), None);
}
