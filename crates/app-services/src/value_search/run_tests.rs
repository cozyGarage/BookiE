use std::sync::Mutex;
use std::time::Duration;

use bookie_core::{ColumnInfo, DriverError, OperationControl, QueryResult, SearchMode, Value};
use tokio_util::sync::CancellationToken;

use super::*;

type Behave = Box<dyn Fn(&str, &str) -> Behaviour + Send + Sync>;

struct Fake {
    calls: Mutex<Vec<String>>,
    behave: Behave,
    cancel_on: Option<(String, CancellationToken)>,
}

enum Behaviour {
    Rows(Vec<Vec<Value>>),
    Fail(DriverError),
    Hang,
}

fn column(name: &str, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable: true,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

impl SearchSource for Fake {
    fn columns(
        &self,
        table: &TableTarget,
        _control: &OperationControl,
    ) -> impl std::future::Future<Output = Result<Vec<ColumnInfo>, DriverError>> + Send {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("columns {}", table.name));
        let behaviour = (self.behave)("columns", &table.name);
        let cancel = self.cancel_on.clone();
        let name = table.name.clone();
        async move {
            if let Some((target, token)) = cancel
                && target == name
            {
                token.cancel();
                return Err(DriverError::Cancelled);
            }
            match behaviour {
                Behaviour::Fail(error) => Err(error),
                Behaviour::Hang => {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    Ok(Vec::new())
                }
                Behaviour::Rows(_) => Ok(vec![column("id", "integer", true), column("name", "text", false)]),
            }
        }
    }

    fn query(
        &self,
        sql: &str,
        _params: &[Value],
        control: &OperationControl,
    ) -> impl std::future::Future<Output = Result<QueryResult, DriverError>> + Send {
        let table = sql.split('"').nth(1).unwrap_or_default().to_owned();
        let name = sql
            .split(" FROM ")
            .nth(1)
            .and_then(|rest| rest.split('"').nth(1))
            .unwrap_or_default()
            .to_owned();
        self.calls.lock().expect("calls").push(format!("query {name}"));
        let behaviour = (self.behave)("query", &name);
        drop(table);
        let deadline = control.deadline();
        async move {
            match behaviour {
                Behaviour::Rows(rows) => Ok(QueryResult {
                    columns: Vec::new(),
                    rows: bookie_core::QueryRows::from_rows(rows),
                    truncated: false,
                }),
                Behaviour::Fail(error) => Err(error),
                Behaviour::Hang => {
                    if let Some(deadline) = deadline {
                        tokio::time::sleep_until(deadline).await;
                    }
                    Err(DriverError::TimedOut)
                }
            }
        }
    }
}

fn fake(behave: impl Fn(&str, &str) -> Behaviour + Send + Sync + 'static) -> Fake {
    Fake {
        calls: Mutex::new(Vec::new()),
        behave: Box::new(behave),
        cancel_on: None,
    }
}

fn targets(names: &[&str]) -> Vec<TableTarget> {
    names
        .iter()
        .map(|name| TableTarget {
            schema: None,
            name: (*name).to_owned(),
        })
        .collect()
}

fn request<'a>(tables: Vec<TableTarget>, limits: SearchLimits) -> SearchRequest<'a> {
    SearchRequest {
        driver_id: "postgres",
        tables,
        term: "ada",
        options: SearchOptions {
            mode: SearchMode::Contains,
            include_non_text: false,
        },
        limits,
    }
}

fn hit_row(id: i64, name_matched: bool) -> Vec<Value> {
    vec![Value::Int(id), Value::Int(i64::from(name_matched))]
}

#[tokio::test]
async fn tables_are_searched_one_after_another_and_hits_name_their_matching_columns() {
    let source = fake(|step, table| match (step, table) {
        ("query", "people") => Behaviour::Rows(vec![hit_row(1, true), hit_row(2, true)]),
        _ => Behaviour::Rows(Vec::new()),
    });
    let report = run_search(
        &source,
        request(targets(&["people", "orders"]), SearchLimits::default()),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("run");

    assert_eq!(
        *source.calls.lock().expect("calls"),
        ["columns people", "query people", "columns orders", "query orders"]
    );
    assert_eq!(report.stopped, None);
    let people = report.with_hits().next().expect("a hit table");
    assert_eq!(people.hits.len(), 2);
    assert_eq!(people.matched_columns, ["name"]);
    assert_eq!(people.hits[0].shown, vec![("id".to_owned(), Value::Int(1))]);
    assert_eq!(report.with_hits().count(), 1);
}

#[tokio::test]
async fn rows_past_the_per_table_limit_are_dropped_and_flagged() {
    let rows: Vec<Vec<Value>> = (0..=MAX_HITS_PER_TABLE as i64).map(|id| hit_row(id, true)).collect();
    let source = fake(move |step, _| match step {
        "query" => Behaviour::Rows(rows.clone()),
        _ => Behaviour::Rows(Vec::new()),
    });
    let report = run_search(
        &source,
        request(targets(&["people"]), SearchLimits::default()),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("run");
    let table = &report.tables[0];
    assert_eq!(table.hits.len() as u64, MAX_HITS_PER_TABLE);
    assert!(table.more_rows_exist);
}

#[tokio::test]
async fn more_tables_than_the_limit_are_counted_and_never_searched() {
    let names: Vec<String> = (0..MAX_SEARCH_TABLES + 7).map(|index| format!("t{index}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let source = fake(|_, _| Behaviour::Rows(Vec::new()));
    let report = run_search(
        &source,
        request(targets(&refs), SearchLimits::default()),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("run");
    assert_eq!(report.tables.len(), MAX_SEARCH_TABLES);
    assert_eq!(report.tables_over_limit, 7);
    assert_eq!(source.calls.lock().expect("calls").len(), MAX_SEARCH_TABLES * 2);
}

#[tokio::test]
async fn a_policy_denial_stops_the_run_and_later_tables_are_not_touched() {
    let source = fake(|step, table| match (step, table) {
        ("query", "orders") => Behaviour::Fail(DriverError::PolicyDenied("not allowed".into())),
        _ => Behaviour::Rows(Vec::new()),
    });
    let report = run_search(
        &source,
        request(targets(&["people", "orders", "items"]), SearchLimits::default()),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("run");
    assert_eq!(report.stopped, Some(StopReason::Denied("not allowed".into())));
    assert_eq!(report.tables.len(), 1);
    assert_eq!(report.tables_not_reached, 2);
    assert!(
        !source
            .calls
            .lock()
            .expect("calls")
            .iter()
            .any(|call| call.contains("items"))
    );
}

#[tokio::test]
async fn cancelling_stops_before_the_next_table_and_records_what_was_left() {
    let token = CancellationToken::new();
    let mut source = fake(|_, _| Behaviour::Rows(Vec::new()));
    source.cancel_on = Some(("orders".into(), token.clone()));
    let report = run_search(
        &source,
        request(targets(&["people", "orders", "items", "more"]), SearchLimits::default()),
        &token,
        |_| {},
    )
    .await
    .expect("run");
    assert_eq!(report.stopped, Some(StopReason::Cancelled));
    assert_eq!(report.tables.len(), 1);
    assert_eq!(report.tables_not_reached, 3);
}

#[tokio::test]
async fn a_token_cancelled_up_front_searches_nothing() {
    let token = CancellationToken::new();
    token.cancel();
    let source = fake(|_, _| Behaviour::Rows(Vec::new()));
    let report = run_search(
        &source,
        request(targets(&["people"]), SearchLimits::default()),
        &token,
        |_| {},
    )
    .await
    .expect("run");
    assert_eq!(report.stopped, Some(StopReason::Cancelled));
    assert!(source.calls.lock().expect("calls").is_empty());
    assert_eq!(report.tables_not_reached, 1);
}

#[tokio::test]
async fn a_table_that_times_out_is_recorded_and_the_search_continues() {
    let source = fake(|step, table| match (step, table) {
        ("query", "slow") => Behaviour::Fail(DriverError::TimedOut),
        _ => Behaviour::Rows(Vec::new()),
    });
    let report = run_search(
        &source,
        request(targets(&["slow", "fast"]), SearchLimits::default()),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("run");
    assert_eq!(report.stopped, None);
    assert!(matches!(report.tables[0].status, TableStatus::Failed(_)));
    assert_eq!(report.tables[1].status, TableStatus::Searched);
}

#[tokio::test]
async fn a_run_that_passes_its_time_budget_stops_and_counts_the_tables_left() {
    let source = fake(|step, table| match (step, table) {
        ("query", "slow") => Behaviour::Hang,
        _ => Behaviour::Rows(Vec::new()),
    });
    let limits = SearchLimits {
        table_timeout: Duration::from_millis(20),
        run_timeout: Duration::from_millis(20),
    };
    let report = run_search(
        &source,
        request(targets(&["slow", "after"]), limits),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("run");
    assert_eq!(report.stopped, Some(StopReason::TimedOut));
    assert_eq!(report.tables_not_reached, 2);
}

#[tokio::test]
async fn another_failure_marks_only_its_table() {
    let source = fake(|step, table| match (step, table) {
        ("columns", "gone") => Behaviour::Fail(DriverError::Internal("dropped".into())),
        _ => Behaviour::Rows(Vec::new()),
    });
    let report = run_search(
        &source,
        request(targets(&["gone", "here"]), SearchLimits::default()),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("run");
    assert!(matches!(report.tables[0].status, TableStatus::Failed(_)));
    assert_eq!(report.tables[1].status, TableStatus::Searched);
}

#[tokio::test]
async fn an_empty_term_is_refused_before_any_statement() {
    let source = fake(|_, _| Behaviour::Rows(Vec::new()));
    let mut bad = request(targets(&["people"]), SearchLimits::default());
    bad.term = "  ";
    let error = run_search(&source, bad, &CancellationToken::new(), |_| {}).await;
    assert_eq!(error, Err(PlanError::EmptyTerm));
    assert!(source.calls.lock().expect("calls").is_empty());
}

#[tokio::test]
async fn progress_names_each_table_before_it_is_searched() {
    let source = fake(|_, _| Behaviour::Rows(Vec::new()));
    let mut seen = Vec::new();
    run_search(
        &source,
        request(targets(&["a", "b"]), SearchLimits::default()),
        &CancellationToken::new(),
        |progress| seen.push((progress.searched, progress.total, progress.current)),
    )
    .await
    .expect("run");
    assert_eq!(seen, vec![(0, 2, "a".to_owned()), (1, 2, "b".to_owned())]);
}

#[tokio::test]
async fn the_notes_say_what_was_left_out_and_why() {
    let source = fake(|step, table| match (step, table) {
        ("query", "orders") => Behaviour::Fail(DriverError::PolicyDenied("blocked".into())),
        ("query", "bad") => Behaviour::Fail(DriverError::Internal("boom".into())),
        _ => Behaviour::Rows(Vec::new()),
    });
    let report = run_search(
        &source,
        request(targets(&["bad", "orders", "later"]), SearchLimits::default()),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("run");
    let notes = report.notes();
    assert_eq!(notes.len(), 2);
    assert!(notes[0].contains("blocked") && notes[0].contains('2'));
    assert!(notes[1].contains('1'));
}
