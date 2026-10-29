use super::*;

#[derive(Clone, Default)]
struct Capture(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn cleanup_warning_keeps_source_payloads_out_of_logs_and_preserves_the_primary_error() {
    let capture = Capture::default();
    let writer = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let outcome = finish_pages(
        Err(ExportError::Source("primary-private-marker".into())),
        Err(ExportError::Source("cleanup-private-marker".into())),
    );
    assert!(matches!(outcome, Err(ExportError::Source(message)) if message == "primary-private-marker"));
    let output = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(!output.contains("private-marker"));
    assert!(output.contains("cleanup failed"));
}

fn job(path: std::path::PathBuf, source: JobSource) -> ExportJob {
    ExportJob {
        path,
        source,
        format: super::super::ExportFormat::Csv,
        options: CsvOptions::default(),
        driver_id: "postgres".into(),
        schema: None,
        table: "t".into(),
    }
}

#[test]
fn an_unverified_paged_export_never_opens_its_source_or_replaces_a_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("saved.csv");
    std::fs::write(&path, "existing export").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let opened = calls.clone();
    let source = JobSource::Paged {
        mode: super::super::PagedExportMode::FullTable,
        columns: Vec::new(),
        total: None,
        open: Arc::new(move |_| {
            opened.fetch_add(1, Ordering::SeqCst);
            super::super::PageFetcher::new(|| Ok(None), || Ok(()))
        }),
    };
    let outcome = write_export(
        job(path.clone(), source),
        &CancellationToken::new(),
        &AtomicUsize::new(0),
    );
    assert!(matches!(outcome, Err(ExportError::Source(_))));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing export");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn a_loaded_result_still_exports_successfully() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("result.csv");
    let source = JobSource::Loaded(Arc::new(tablepro_core::QueryResult {
        columns: vec![tablepro_core::ColumnInfo {
            name: "label".into(),
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
        }],
        rows: vec![vec![tablepro_core::Value::Text("hello, export".into())]],
        truncated: false,
    }));
    write_export(
        job(path.clone(), source),
        &CancellationToken::new(),
        &AtomicUsize::new(0),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "label\n\"hello, export\"\n");
}

#[test]
fn a_guarded_query_export_finishes_its_source_after_writing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("query.csv");
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished_source = finished.clone();
    let source = JobSource::Paged {
        mode: super::super::PagedExportMode::GuardedQuery,
        columns: vec![tablepro_core::ColumnInfo {
            name: "id".into(),
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
        }],
        total: None,
        open: Arc::new(move |_| {
            let mut sent = false;
            let finished = finished_source.clone();
            super::super::PageFetcher::new(
                move || {
                    if sent {
                        return Ok(None);
                    }
                    sent = true;
                    Ok(Some(vec![vec![tablepro_core::Value::Int(42)]]))
                },
                move || {
                    finished.store(true, Ordering::SeqCst);
                    Ok(())
                },
            )
        }),
    };

    write_export(
        job(path.clone(), source),
        &CancellationToken::new(),
        &AtomicUsize::new(0),
    )
    .unwrap();

    assert!(finished.load(Ordering::SeqCst));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "id\n42\n");
}

#[test]
fn a_cancelled_guarded_query_export_still_finishes_its_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("query.csv");
    let cancel = CancellationToken::new();
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished_source = finished.clone();
    let source = JobSource::Paged {
        mode: super::super::PagedExportMode::GuardedQuery,
        columns: vec![tablepro_core::ColumnInfo {
            name: "id".into(),
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
        }],
        total: None,
        open: Arc::new(move |cancel| {
            let finished = finished_source.clone();
            let mut sent = false;
            super::super::PageFetcher::new(
                move || {
                    if sent {
                        return Ok(None);
                    }
                    sent = true;
                    cancel.cancel();
                    Ok(Some(vec![vec![tablepro_core::Value::Int(42)]]))
                },
                move || {
                    finished.store(true, Ordering::SeqCst);
                    Ok(())
                },
            )
        }),
    };
    let rows = Arc::new(AtomicUsize::new(0));
    let rows_written = rows.clone();
    let outcome = write_export(job(path.clone(), source), &cancel, &rows);
    assert!(matches!(outcome, Err(ExportError::Cancelled)));
    assert!(finished.load(Ordering::SeqCst));
    assert_eq!(rows_written.load(Ordering::SeqCst), 0);
    assert!(!path.exists());
}

#[test]
fn a_failed_guarded_query_export_finishes_its_source_and_keeps_the_source_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("query.csv");
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished_source = finished.clone();
    let source = JobSource::Paged {
        mode: super::super::PagedExportMode::GuardedQuery,
        columns: Vec::new(),
        total: None,
        open: Arc::new(move |_| {
            let finished = finished_source.clone();
            super::super::PageFetcher::new(
                || Err(ExportError::Source("source page failed".into())),
                move || {
                    finished.store(true, Ordering::SeqCst);
                    Ok(())
                },
            )
        }),
    };

    let outcome = write_export(
        job(path.clone(), source),
        &CancellationToken::new(),
        &AtomicUsize::new(0),
    );

    assert!(matches!(outcome, Err(ExportError::Source(message)) if message == "source page failed"));
    assert!(finished.load(Ordering::SeqCst));
    assert!(!path.exists());
}
