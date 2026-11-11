use super::*;
use bookie_app_services::stream_result_spool::StreamResultSpoolManager;
use bookie_core::{ColumnInfo, QueryResult, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn column() -> ColumnInfo {
    ColumnInfo {
        name: "value".into(),
        data_type: "BIGINT".into(),
        nullable: false,
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

fn fixture() -> (
    StreamResultSpool,
    Vec<ColumnInfo>,
    crate::services::export_pages::PageRunner,
) {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let spool = StreamResultSpoolManager::default().create().unwrap();
    let columns = vec![column()];
    for (index, value) in [11, 22].into_iter().enumerate() {
        let result = Arc::new(QueryResult {
            columns: columns.clone(),
            rows: vec![vec![Value::Int(value)]].into(),
            truncated: false,
        });
        runtime.block_on(spool.append_page(index as u64, result)).unwrap();
    }
    let run = Arc::new(move |future: bookie_app_services::export_pages::PageFuture| runtime.block_on(future));
    (spool, columns, run)
}

#[test]
fn spool_export_reads_original_pages_in_order_without_a_database_connection() {
    let (spool, columns, run) = fixture();
    let factory = spool_export_factory_with_runner(spool, columns, 2, run);
    let mut pages = factory(CancellationToken::new());
    assert_eq!(pages.next_page().unwrap(), Some(vec![vec![Value::Int(11)]]));
    assert_eq!(pages.next_page().unwrap(), Some(vec![vec![Value::Int(22)]]));
    assert!(pages.next_page().unwrap().is_none());
    pages.finish().unwrap();
}

#[test]
fn complete_spool_is_written_in_order_to_a_csv_file() {
    let (spool, columns, run) = fixture();
    let factory = spool_export_factory_with_runner(spool, columns.clone(), 2, run);
    let mut pages = factory(CancellationToken::new());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("result.csv");
    let options = bookie_core::export::CsvOptions::default();
    let export = bookie_core::export::ResultExport {
        format: bookie_core::export::ResultFormat::Csv,
        csv: &options,
        sql: None,
    };
    bookie_core::export::write_paged_file(&path, &columns, 2, &export, || pages.next_page(), || false, |_| {}).unwrap();
    pages.finish().unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "value\n11\n22\n");
}

#[test]
fn spool_export_refuses_a_row_count_mismatch() {
    let (spool, columns, run) = fixture();
    let factory = spool_export_factory_with_runner(spool, columns, 3, run);
    let mut pages = factory(CancellationToken::new());
    assert!(pages.next_page().unwrap().is_some());
    assert!(pages.next_page().unwrap().is_some());
    assert!(matches!(pages.next_page(), Err(ExportError::Source(_))));
}

#[test]
fn incomplete_spool_does_not_replace_an_existing_export() {
    let (spool, columns, run) = fixture();
    let factory = spool_export_factory_with_runner(spool, columns.clone(), 3, run);
    let mut pages = factory(CancellationToken::new());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("result.csv");
    std::fs::write(&path, "existing").unwrap();
    let options = bookie_core::export::CsvOptions::default();
    let export = bookie_core::export::ResultExport {
        format: bookie_core::export::ResultFormat::Csv,
        csv: &options,
        sql: None,
    };
    let result =
        bookie_core::export::write_paged_file(&path, &columns, 3, &export, || pages.next_page(), || false, |_| {});
    assert!(matches!(result, Err(ExportError::Source(_))));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "existing");
}

#[test]
fn spool_export_honors_cancellation_before_reading() {
    let (spool, columns, run) = fixture();
    let factory = spool_export_factory_with_runner(spool, columns, 2, run);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut pages = factory(cancel);
    assert!(matches!(pages.next_page(), Err(ExportError::Cancelled)));
}

#[test]
fn cancelled_spool_export_releases_its_reservation_when_closed() {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let manager = StreamResultSpoolManager::default();
    let spool = manager.create().unwrap();
    runtime
        .block_on(spool.append_page(
            0,
            Arc::new(QueryResult {
                columns: vec![column()],
                rows: vec![vec![Value::Int(11)]].into(),
                truncated: false,
            }),
        ))
        .unwrap();
    let run_runtime = runtime.clone();
    let cancel = CancellationToken::new();
    let run_cancel = cancel.clone();
    let run = Arc::new(move |future| {
        let result = run_runtime.block_on(future);
        run_cancel.cancel();
        result
    });
    let factory = spool_export_factory_with_runner(spool.clone(), vec![column()], 1, run);
    drop(spool);
    let mut pages = factory(cancel);
    drop(factory);
    assert!(matches!(pages.next_page(), Err(ExportError::Cancelled)));
    assert!(manager.reserved_bytes() > 0);
    pages.finish().unwrap();
    assert_eq!(manager.reserved_bytes(), 0);
}
