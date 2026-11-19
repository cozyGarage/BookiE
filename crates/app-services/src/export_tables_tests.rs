use std::sync::Arc;

use bookie_core::export::{CsvOptions, ResultFormat};
use bookie_core::{ConnectOptions, Connection, DatabaseDriver};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::export_pages::PageRunner;

struct Fixture {
    runtime: Arc<tokio::runtime::Runtime>,
    conn: Arc<dyn Connection>,
    run: PageRunner,
    _database: tempfile::TempDir,
}

fn fixture(tables: &[(&str, usize)]) -> Fixture {
    let database = tempfile::tempdir().unwrap();
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let path = database.path().join("source.sqlite");
    std::fs::File::create(&path).unwrap();
    let conn: Arc<dyn Connection> = runtime.block_on(async {
        let conn = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: path.to_string_lossy().into_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
        for (name, rows) in tables {
            conn.execute(&format!("CREATE TABLE \"{name}\" (id INTEGER PRIMARY KEY, label TEXT)"))
                .await
                .unwrap();
            for id in 1..=*rows {
                conn.execute(&format!("INSERT INTO \"{name}\" VALUES ({id}, 'row {id}')"))
                    .await
                    .unwrap();
            }
        }
        Arc::from(conn)
    });
    let handle = runtime.clone();
    let run: PageRunner = Arc::new(move |future| handle.block_on(future));
    Fixture {
        runtime,
        conn,
        run,
        _database: database,
    }
}

fn targets(fixture: &Fixture, names: &[&str]) -> Vec<TableTarget> {
    names
        .iter()
        .map(|name| TableTarget {
            schema: None,
            table: (*name).to_string(),
            columns: fixture
                .runtime
                .block_on(fixture.conn.fetch_columns(None, name))
                .unwrap_or_default(),
        })
        .collect()
}

fn request<'a>(
    tables: &'a [TableTarget],
    directory: &'a Path,
    csv: &'a CsvOptions,
    max_rows: usize,
) -> TableExportRequest<'a> {
    TableExportRequest {
        driver_id: "sqlite",
        tables,
        directory,
        format: ResultFormat::Csv,
        extension: "csv",
        csv,
        max_rows_per_table: max_rows,
        timeout_secs: 30,
    }
}

#[test]
fn each_table_stops_at_the_cap_and_reports_whether_it_was_cut() {
    let fixture = fixture(&[("big", 5), ("small", 3), ("empty", 0)]);
    let tables = targets(&fixture, &["big", "small", "empty"]);
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let results = export_tables(
        fixture.conn.clone(),
        &request(&tables, directory.path(), &csv, 4),
        &fixture.run,
        &CancellationToken::new(),
        |_, _| {},
    )
    .unwrap();
    let outcomes: Vec<_> = results.iter().map(|result| &result.outcome).collect();
    assert_eq!(
        outcomes,
        [
            &TableOutcome::Written { rows: 4, capped: true },
            &TableOutcome::Written { rows: 3, capped: false },
            &TableOutcome::Written { rows: 0, capped: false },
        ]
    );
    let big = std::fs::read_to_string(directory.path().join("big.csv")).unwrap();
    assert_eq!(big.lines().count(), 5, "a header and four rows: {big}");
    assert!(big.contains("row 4") && !big.contains("row 5"));
}

#[test]
fn cancelling_stops_the_remaining_tables_and_leaves_no_partial_file() {
    let fixture = fixture(&[("one", 3), ("two", 3), ("three", 3)]);
    let tables = targets(&fixture, &["one", "two", "three"]);
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let cancel = CancellationToken::new();
    let results = export_tables(
        fixture.conn.clone(),
        &request(&tables, directory.path(), &csv, 100),
        &fixture.run,
        &cancel,
        |index, rows| {
            if index == 1 && rows == 1 {
                cancel.cancel();
            }
        },
    )
    .unwrap();
    assert!(matches!(results[0].outcome, TableOutcome::Written { rows: 3, .. }));
    assert!(
        matches!(results[1].outcome, TableOutcome::Failed(_)),
        "{:?}",
        results[1]
    );
    assert_eq!(results[2].outcome, TableOutcome::NotStarted);
    assert!(directory.path().join("one.csv").exists());
    assert!(
        !directory.path().join("two.csv").exists(),
        "a cancelled table leaves no file"
    );
    assert!(!directory.path().join("three.csv").exists());
}

#[test]
fn an_existing_file_is_never_replaced_and_a_missing_table_does_not_stop_the_rest() {
    let fixture = fixture(&[("keep", 2), ("other", 2)]);
    let mut tables = targets(&fixture, &["keep", "other"]);
    tables.insert(
        1,
        TableTarget {
            schema: None,
            table: "no_such_table".into(),
            columns: tables[0].columns.clone(),
        },
    );
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("keep.csv"), "precious").unwrap();
    let csv = CsvOptions::default();
    let results = export_tables(
        fixture.conn.clone(),
        &request(&tables, directory.path(), &csv, 10),
        &fixture.run,
        &CancellationToken::new(),
        |_, _| {},
    )
    .unwrap();
    assert!(matches!(&results[0].outcome, TableOutcome::Failed(message) if message.contains("already exists")));
    assert!(matches!(results[1].outcome, TableOutcome::Failed(_)));
    assert!(matches!(results[2].outcome, TableOutcome::Written { rows: 2, .. }));
    assert_eq!(
        std::fs::read_to_string(directory.path().join("keep.csv")).unwrap(),
        "precious"
    );
    assert!(
        !directory.path().join("no_such_table.csv").exists(),
        "a failed read writes no file"
    );
}

#[test]
fn file_names_are_safe_and_unique() {
    let mut used = HashSet::new();
    assert_eq!(file_stem("public.orders", &mut used), "public.orders");
    assert_eq!(file_stem("Public.Orders", &mut used), "Public.Orders-2");
    assert_eq!(file_stem("../etc/passwd", &mut used), "_.._etc_passwd");
    assert_eq!(file_stem("a b/c:d", &mut used), "a_b_c_d");
    assert_eq!(file_stem(&"x".repeat(300), &mut used).len(), MAX_FILE_STEM_CHARS);
}

#[test]
fn a_request_without_a_cap_a_table_or_a_folder_is_refused_before_anything_runs() {
    let fixture = fixture(&[("t", 1)]);
    let tables = targets(&fixture, &["t"]);
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let run = |request: TableExportRequest<'_>| {
        export_tables(
            fixture.conn.clone(),
            &request,
            &fixture.run,
            &CancellationToken::new(),
            |_, _| {},
        )
    };
    assert!(
        run(request(&tables, directory.path(), &csv, 0))
            .unwrap_err()
            .contains("row limit")
    );
    assert!(
        run(request(&[], directory.path(), &csv, 5))
            .unwrap_err()
            .contains("at least one table")
    );
    let missing = directory.path().join("nope");
    assert!(run(request(&tables, &missing, &csv, 5)).unwrap_err().contains("folder"));
}
