use super::*;
use std::sync::{Arc, Mutex, atomic::Ordering};

#[test]
fn a_full_shared_page_budget_stops_the_cursor_without_fetching() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let manager = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default();
    let spool = manager.create().unwrap();
    let budget = spool.page_budget();
    let reservation = budget.reserve(budget.limit_bytes()).unwrap();
    let counters = Arc::new(CursorCounters::default());
    let result = runtime.block_on(super::super::run_statements_with_spool_manager(
        StatementRun {
            target: cursor_target(vec![Ok(cursor_page(&[1], false))], counters.clone()),
            statements: vec!["SELECT id FROM items".into()],
            driver_id: "postgres",
            parameter_values: &Default::default(),
            control: &crate::services::operation_control::bounded(30),
            error_policy: BatchErrorPolicy::StopScript,
            settings_check: None,
            succeeded: |_: &str| {},
        },
        manager,
        |_update| std::future::ready(true),
    ));
    assert!(matches!(result, ScriptRunResult::Streamed(StreamTerminal::Failed(_))));
    assert_eq!(counters.fetched.load(Ordering::SeqCst), 0);
    assert_eq!(counters.rolled_back.load(Ordering::SeqCst), 1);
    drop(reservation);
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn exporting_a_completed_stream_reuses_pages_without_executing_sql_again() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let counters = Arc::new(CursorCounters::default());
    let saved = Arc::new(Mutex::new(None));
    let columns = cursor_page(&[1], true).result.columns;
    let result = runtime.block_on(run_statements_with_stream(
        StatementRun {
            target: cursor_target(
                vec![Ok(cursor_page(&[1, 2], true)), Ok(cursor_page(&[3], false))],
                counters.clone(),
            ),
            statements: vec!["SELECT id FROM items".into()],
            driver_id: "postgres",
            parameter_values: &Default::default(),
            control: &crate::services::operation_control::bounded(30),
            error_policy: BatchErrorPolicy::StopScript,
            settings_check: None,
            succeeded: |_: &str| {},
        },
        {
            let saved = saved.clone();
            move |update| {
                if let StreamUpdate::Started { spool, .. } = update {
                    *saved.lock().unwrap() = Some(spool);
                }
                std::future::ready(true)
            }
        },
    ));
    assert!(matches!(result, ScriptRunResult::Streamed(StreamTerminal::Complete)));
    let spool = saved.lock().unwrap().take().unwrap();
    let factory = crate::ui::export_dialog::spool_export_factory(spool, columns, 3);
    let mut pages = factory(tokio_util::sync::CancellationToken::new());
    let mut rows = Vec::new();
    while let Some(page) = pages.next_page().unwrap() {
        rows.extend(page.into_iter().map(|row| row[0].clone()));
    }
    pages.finish().unwrap();
    assert_eq!(rows, [1, 2, 3].map(bookie_core::Value::Int));
    assert_eq!(counters.opened.load(Ordering::SeqCst), 1);
    assert_eq!(counters.fetched.load(Ordering::SeqCst), 2);
}

#[test]
fn a_spool_quota_failure_stops_the_cursor_and_fails_the_result() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let counters = Arc::new(CursorCounters::default());
    let delivered_page = Arc::new(Mutex::new(None));
    let succeeded = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let result = runtime.block_on(super::super::run_statements_with_spool_manager(
        StatementRun {
            target: cursor_target(
                vec![Ok(cursor_page(&[1], true)), Ok(cursor_page(&[2], false))],
                counters.clone(),
            ),
            statements: vec!["SELECT id FROM items".into()],
            driver_id: "postgres",
            parameter_values: &Default::default(),
            control: &crate::services::operation_control::bounded(30),
            error_policy: BatchErrorPolicy::StopScript,
            settings_check: None,
            succeeded: {
                let succeeded = succeeded.clone();
                move |_: &str| {
                    succeeded.fetch_add(1, Ordering::SeqCst);
                }
            },
        },
        bookie_app_services::stream_result_spool::StreamResultSpoolManager::with_limit(1),
        {
            let delivered_page = delivered_page.clone();
            move |update| {
                if let StreamUpdate::Page { spooled, .. } = update {
                    *delivered_page.lock().unwrap() = Some(spooled);
                }
                std::future::ready(true)
            }
        },
    ));

    assert!(matches!(result, ScriptRunResult::Streamed(StreamTerminal::Failed(_))));
    assert_eq!(*delivered_page.lock().unwrap(), Some(false));
    assert_eq!(counters.fetched.load(Ordering::SeqCst), 1);
    assert_eq!(counters.closed.load(Ordering::SeqCst), 1);
    assert!(!counters.cursor_open.load(Ordering::SeqCst));
    assert_eq!(succeeded.load(Ordering::SeqCst), 0);
}
