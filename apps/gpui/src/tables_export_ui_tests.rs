use std::path::PathBuf;

use bookie_app_services::export_tables::{TableOutcome, TableResult};
use bookie_gpui::messages::MessageKind;

use super::{export_folder, export_report, row_limit};

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("bookie-tables-export-{}-{name}", std::process::id()))
}

#[test]
fn the_folder_must_be_an_existing_absolute_directory() {
    let directory = scratch("dir");
    std::fs::create_dir_all(&directory).unwrap();
    assert_eq!(export_folder(&directory.display().to_string()).unwrap(), directory);
    assert!(export_folder("").is_err());
    assert!(export_folder("exports").unwrap_err().contains("absolute"));
    assert!(export_folder("/definitely/not/here").unwrap_err().contains("folder"));
    let file = scratch("file");
    std::fs::write(&file, b"x").unwrap();
    assert!(
        export_folder(&file.display().to_string()).is_err(),
        "a file is not a folder"
    );
    std::fs::remove_dir_all(&directory).ok();
    std::fs::remove_file(&file).ok();
}

#[test]
fn the_row_limit_is_empty_for_no_limit_or_a_positive_whole_number() {
    assert_eq!(row_limit("").unwrap(), None);
    assert_eq!(row_limit("  ").unwrap(), None);
    assert_eq!(row_limit(" 500 ").unwrap(), Some(500));
    for bad in ["0", "-3", "ten", "1.5"] {
        assert!(row_limit(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_report_has_one_line_per_table_and_marks_failures_and_limits() {
    let results = vec![
        TableResult {
            table: "orders".into(),
            path: None,
            outcome: TableOutcome::Written { rows: 50, capped: true },
        },
        TableResult {
            table: "notes".into(),
            path: None,
            outcome: TableOutcome::Written { rows: 3, capped: false },
        },
        TableResult {
            table: "broken".into(),
            path: None,
            outcome: TableOutcome::Failed("permission denied".into()),
        },
        TableResult {
            table: "later".into(),
            path: None,
            outcome: TableOutcome::NotStarted,
        },
    ];
    let lines = export_report(&results);
    assert_eq!(lines.len(), 4);
    assert_eq!(
        lines[0],
        (MessageKind::Success, "orders: 50 rows (limited)".to_string())
    );
    assert_eq!(lines[1], (MessageKind::Success, "notes: 3 rows".to_string()));
    assert_eq!(lines[2], (MessageKind::Error, "broken: permission denied".to_string()));
    assert_eq!(lines[3], (MessageKind::Info, "later: not started".to_string()));
}
