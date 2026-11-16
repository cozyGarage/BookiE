use super::*;

fn outcome(is_write: bool, rows_affected: Option<u64>) -> StatementOutcome {
    StatementOutcome {
        identity: StatementIdentity::default(),
        sql_preview: String::new(),
        sql: String::new(),
        elapsed_ms: 8,
        kind: StatementOutcomeKind::Rows(Vec::new()),
        pinned: false,
        detail: super::super::outcome_detail::OutcomeDetail {
            rows_affected,
            is_write,
            ..Default::default()
        },
    }
}

#[test]
fn empty_write_reports_the_database_count_or_marks_it_unknown() {
    assert_eq!(no_result_description(&outcome(true, Some(1))), "1 row affected · 8 ms");
    assert_eq!(no_result_description(&outcome(true, Some(3))), "3 rows affected · 8 ms");
    assert!(no_result_description(&outcome(true, None)).contains("count unavailable"));
    assert_eq!(no_result_description(&outcome(false, None)), "No rows returned · 8 ms");
}
