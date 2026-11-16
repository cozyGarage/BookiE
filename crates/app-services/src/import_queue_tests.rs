use std::sync::{Arc, Mutex};

use super::*;

type Seen = Arc<Mutex<Vec<(usize, usize, String, u64)>>>;

struct Script {
    outcomes: Vec<FileOutcome>,
    seen: Seen,
    cancel_after: Option<(usize, CancellationToken)>,
}

impl Script {
    fn new(outcomes: Vec<FileOutcome>) -> Self {
        Self {
            outcomes,
            seen: Arc::new(Mutex::new(Vec::new())),
            cancel_after: None,
        }
    }
}

impl FileImporter for Script {
    fn import(
        &mut self,
        index: usize,
        count: usize,
        path: &Path,
        budget: RunBudget,
    ) -> impl Future<Output = FileOutcome> + Send {
        self.seen
            .lock()
            .expect("seen lock")
            .push((index, count, file_name(path), budget.remaining_rows));
        if let Some((at, token)) = &self.cancel_after
            && *at == index
        {
            token.cancel();
        }
        let outcome = self
            .outcomes
            .get(index)
            .cloned()
            .unwrap_or(FileOutcome::Imported { rows: 0, skipped: 0 });
        async move { outcome }
    }
}

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

fn results(report: &QueueReport) -> Vec<FileResult> {
    report.files.iter().map(|file| file.result.clone()).collect()
}

#[tokio::test]
async fn files_run_in_the_order_chosen_and_the_run_reports_each_one() {
    let mut script = Script::new(vec![
        FileOutcome::Imported { rows: 3, skipped: 0 },
        FileOutcome::Imported { rows: 5, skipped: 0 },
    ]);
    let report = run_queue(paths(&["a.csv", "b.csv"]), &CancellationToken::new(), &mut script).await;

    assert_eq!(
        *script.seen.lock().expect("seen lock"),
        vec![
            (0, 2, "a.csv".to_owned(), MAX_RUN_ROWS),
            (1, 2, "b.csv".to_owned(), MAX_RUN_ROWS - 3),
        ]
    );
    assert_eq!(report.rows_written(), 8);
    assert!(report.succeeded());
    assert!(report.first_failure().is_none());
}

#[tokio::test]
async fn a_failing_file_stops_the_run_and_the_report_names_it() {
    let mut script = Script::new(vec![
        FileOutcome::Imported { rows: 2, skipped: 0 },
        FileOutcome::Failed {
            rows: 1,
            message: "bad row".into(),
        },
    ]);
    let report = run_queue(
        paths(&["a.csv", "b.csv", "c.csv"]),
        &CancellationToken::new(),
        &mut script,
    )
    .await;

    assert_eq!(script.seen.lock().expect("seen lock").len(), 2);
    assert_eq!(
        results(&report),
        vec![
            FileResult::Imported { rows: 2, skipped: 0 },
            FileResult::Failed {
                rows: 1,
                message: "bad row".into()
            },
            FileResult::NotStarted,
        ]
    );
    let failure = report.first_failure().expect("failure");
    assert_eq!(failure.path, PathBuf::from("b.csv"));
    assert_eq!(report.rows_written(), 3);
    assert!(!report.succeeded());
}

#[tokio::test]
async fn one_cancel_stops_the_file_in_flight_and_cancels_the_rest() {
    let token = CancellationToken::new();
    let mut script = Script::new(vec![
        FileOutcome::Imported { rows: 2, skipped: 0 },
        FileOutcome::Cancelled { rows: 4 },
    ]);
    let report = run_queue(paths(&["a.csv", "b.csv", "c.csv", "d.csv"]), &token, &mut script).await;

    assert_eq!(script.seen.lock().expect("seen lock").len(), 2);
    assert_eq!(
        results(&report),
        vec![
            FileResult::Imported { rows: 2, skipped: 0 },
            FileResult::Cancelled { rows: 4 },
            FileResult::Cancelled { rows: 0 },
            FileResult::Cancelled { rows: 0 },
        ]
    );
    assert!(report.was_cancelled());
    assert_eq!(report.rows_written(), 6);
}

#[tokio::test]
async fn a_cancel_between_files_never_starts_the_next_one() {
    let token = CancellationToken::new();
    let mut script = Script::new(vec![FileOutcome::Imported { rows: 1, skipped: 0 }]);
    script.cancel_after = Some((0, token.clone()));
    let report = run_queue(paths(&["a.csv", "b.csv"]), &token, &mut script).await;

    assert_eq!(script.seen.lock().expect("seen lock").len(), 1);
    assert_eq!(
        results(&report),
        vec![
            FileResult::Imported { rows: 1, skipped: 0 },
            FileResult::Cancelled { rows: 0 }
        ]
    );
}

#[tokio::test]
async fn a_token_cancelled_before_the_run_starts_no_file() {
    let token = CancellationToken::new();
    token.cancel();
    let mut script = Script::new(Vec::new());
    let report = run_queue(paths(&["a.csv", "b.csv"]), &token, &mut script).await;

    assert!(script.seen.lock().expect("seen lock").is_empty());
    assert!(report.was_cancelled());
}

#[tokio::test]
async fn the_row_budget_shrinks_with_every_file() {
    let mut script = Script::new(vec![
        FileOutcome::Imported {
            rows: MAX_RUN_ROWS - 10,
            skipped: 0,
        },
        FileOutcome::Imported { rows: 5, skipped: 0 },
    ]);
    run_queue(paths(&["a.csv", "b.csv"]), &CancellationToken::new(), &mut script).await;

    let seen = script.seen.lock().expect("seen lock");
    assert_eq!(seen[1].3, 10);
}

#[test]
fn a_run_needs_files_and_stays_within_the_file_and_byte_caps() {
    assert_eq!(check_queue(&[]), Err(QueueError::NoFiles));
    let many: Vec<(PathBuf, u64)> = (0..=MAX_QUEUED_FILES)
        .map(|index| (PathBuf::from(format!("{index}.csv")), 1))
        .collect();
    assert_eq!(
        check_queue(&many),
        Err(QueueError::TooManyFiles { max: MAX_QUEUED_FILES })
    );
    let exact: Vec<(PathBuf, u64)> = (0..MAX_QUEUED_FILES)
        .map(|index| (PathBuf::from(format!("{index}.csv")), 1))
        .collect();
    assert_eq!(check_queue(&exact), Ok(()));
    let heavy = vec![
        (PathBuf::from("a.csv"), MAX_RUN_FILE_BYTES),
        (PathBuf::from("b.csv"), 1),
    ];
    assert!(matches!(check_queue(&heavy), Err(QueueError::RunTooLarge { .. })));
}

#[test]
fn the_summary_lists_every_file_with_its_state() {
    let report = QueueReport {
        files: vec![
            FileReport {
                path: PathBuf::from("/data/a.csv"),
                result: FileResult::Imported { rows: 2, skipped: 0 },
            },
            FileReport {
                path: PathBuf::from("/data/b.csv"),
                result: FileResult::Failed {
                    rows: 0,
                    message: "bad".into(),
                },
            },
            FileReport {
                path: PathBuf::from("/data/c.csv"),
                result: FileResult::NotStarted,
            },
        ],
    };
    let lines: Vec<String> = report.summary().lines().map(str::to_owned).collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("a.csv") && lines[0].contains('2'));
    assert!(lines[1].contains("b.csv") && lines[1].contains("bad"));
    assert!(lines[2].contains("c.csv"));
}
