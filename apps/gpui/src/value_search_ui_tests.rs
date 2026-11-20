use bookie_app_services::value_search::{SearchReport, StopReason, TableHit, TableOutcome, TableStatus, TableTarget};
use bookie_core::Value;

use super::summarize;
use bookie_gpui::messages::MessageKind;

fn outcome(name: &str, hits: usize, more: bool) -> TableOutcome {
    TableOutcome {
        target: TableTarget {
            schema: Some("public".into()),
            name: name.into(),
        },
        status: TableStatus::Searched,
        hits: (0..hits)
            .map(|index| TableHit {
                shown: vec![
                    ("id".into(), Value::Int(index as i64 + 1)),
                    ("name".into(), Value::Text("Ada".into())),
                ],
                matched: vec!["name".into()],
            })
            .collect(),
        matched_columns: if hits > 0 { vec!["name".into()] } else { Vec::new() },
        more_rows_exist: more,
        columns_not_searched: 0,
    }
}

fn report(tables: Vec<TableOutcome>) -> SearchReport {
    SearchReport {
        tables,
        ..SearchReport::default()
    }
}

#[test]
fn a_table_with_hits_gets_a_success_line_and_its_first_match() {
    let lines = summarize(
        &report(vec![outcome("people", 2, false), outcome("empty", 0, false)]),
        "Ada",
    );
    assert_eq!(lines[0].0, MessageKind::Success);
    assert!(
        lines[0].1.starts_with("public.people: 2 matching row(s) in name"),
        "{}",
        lines[0].1
    );
    assert!(
        lines[1].1.contains("id = 1") && lines[1].1.contains("name = Ada"),
        "{}",
        lines[1].1
    );
    assert_eq!(lines.len(), 2, "tables without hits add no line");
}

#[test]
fn more_matching_rows_than_the_cap_are_said_to_exist() {
    let lines = summarize(&report(vec![outcome("people", 25, true)]), "Ada");
    assert!(lines[0].1.contains("at least"), "{}", lines[0].1);
}

#[test]
fn no_hits_says_what_was_searched() {
    let lines = summarize(&report(vec![outcome("a", 0, false), outcome("b", 0, false)]), "zzz");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].0, MessageKind::Info);
    assert!(
        lines[0].1.contains("zzz") && lines[0].1.contains("2 tables"),
        "{}",
        lines[0].1
    );
}

#[test]
fn a_stopped_search_adds_the_shared_notes_as_errors() {
    let mut stopped = report(vec![outcome("people", 1, false)]);
    stopped.stopped = Some(StopReason::TimedOut);
    stopped.tables_not_reached = 3;
    let lines = summarize(&stopped, "Ada");
    let last = lines.last().unwrap();
    assert_eq!(last.0, MessageKind::Error);
    assert!(last.1.contains("ran out of time"), "{}", last.1);
}
