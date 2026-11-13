use super::*;
use bookie_core::import::CsvImportOptions;

fn report(error: Option<&str>, cancelled: bool, committed: u64, skipped: u64) -> super::super::CsvImportReport {
    super::super::CsvImportReport {
        schema: None,
        table: "people".into(),
        committed,
        total: 10,
        error: error.map(str::to_owned),
        skipped,
        skipped_summary: String::new(),
        updates_existing_rows: false,
        cancelled,
    }
}

#[test]
fn a_clean_report_becomes_an_imported_file_with_its_skipped_rows() {
    let token = CancellationToken::new();
    assert_eq!(
        outcome_of(report(None, false, 8, 2), &token),
        FileOutcome::Imported { rows: 8, skipped: 2 }
    );
}

#[test]
fn a_cancelled_report_or_token_becomes_a_cancelled_file() {
    let token = CancellationToken::new();
    assert_eq!(
        outcome_of(report(Some("stopped"), true, 4, 0), &token),
        FileOutcome::Cancelled { rows: 4 }
    );
    token.cancel();
    assert_eq!(
        outcome_of(report(Some("stopped"), false, 4, 0), &token),
        FileOutcome::Cancelled { rows: 4 }
    );
}

#[test]
fn a_failed_report_keeps_the_committed_rows_and_the_message() {
    let token = CancellationToken::new();
    assert_eq!(
        outcome_of(report(Some("duplicate key"), false, 3, 0), &token),
        FileOutcome::Failed {
            rows: 3,
            message: "duplicate key".into()
        }
    );
}

#[test]
fn files_share_a_shape_only_when_their_headers_match() {
    let options = CsvImportOptions::default();
    let first = file_shape(b"id,name\n1,a\n", &options).expect("first shape");
    let same = file_shape(b"id,name\n2,b\n3,c\n", &options).expect("same shape");
    let other = file_shape(b"id,title\n2,b\n", &options).expect("other shape");
    assert_eq!(first, same);
    assert_ne!(first, other);
}

#[test]
fn a_file_without_a_header_is_shaped_by_its_field_count() {
    let options = CsvImportOptions {
        has_header: false,
        ..CsvImportOptions::default()
    };
    assert_eq!(file_shape(b"1,a\n2,b\n", &options).expect("shape").len(), 2);
    assert_eq!(file_shape(b"1,a,x\n", &options).expect("shape").len(), 3);
}

#[test]
fn a_missing_later_file_is_rejected_before_the_queue_can_start() {
    let directory = tempfile::tempdir().expect("directory");
    let first = directory.path().join("first.csv");
    std::fs::write(&first, b"id,name\n1,Ada\n").expect("first file");
    let missing = directory.path().join("missing.csv");

    assert!(super::super::check_file_queue(&first, &[missing]).is_err());
}

#[test]
fn an_over_limit_later_file_is_rejected_before_the_queue_can_start() {
    let directory = tempfile::tempdir().expect("directory");
    let first = directory.path().join("first.csv");
    let oversized = directory.path().join("oversized.csv");
    std::fs::write(&first, b"id,name\n1,Ada\n").expect("first file");
    std::fs::File::create(&oversized)
        .expect("oversized file")
        .set_len(bookie_core::import::MAX_FILE_BYTES + 1)
        .expect("set sparse file size");

    assert!(super::super::check_file_queue(&first, &[oversized]).is_err());
}

#[test]
fn the_toast_names_the_rows_the_files_and_the_table() {
    let queue = QueueReport {
        files: vec![
            bookie_app_services::import_queue::FileReport {
                path: "a.csv".into(),
                result: bookie_app_services::import_queue::FileResult::Imported { rows: 3, skipped: 0 },
            },
            bookie_app_services::import_queue::FileReport {
                path: "b.csv".into(),
                result: bookie_app_services::import_queue::FileResult::Imported { rows: 4, skipped: 0 },
            },
        ],
    };
    let text = queue_toast(&queue, "people", false);
    assert!(text.contains('7') && text.contains('2') && text.contains("people"));
    assert!(queue_toast(&queue, "people", true).contains("updated"));
}
