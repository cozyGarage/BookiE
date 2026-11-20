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
        max_rows_per_table: Some(max_rows),
        timeout_secs: 30,
    }
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
fn a_request_with_a_zero_cap_no_table_or_no_folder_is_refused_before_anything_runs() {
    let fixture = fixture(&[("t", 1)]);
    let tables = targets(&fixture, &["t"]);
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let run = |request: TableExportRequest<'_>| {
        export_tables(
            test_guard(fixture.conn.clone()),
            &drivers_sqlite::SqliteDriver,
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
    let too_many = vec![tables[0].clone(); MAX_TABLES_PER_EXPORT + 1];
    assert!(
        run(request(&too_many, directory.path(), &csv, 5))
            .unwrap_err()
            .contains("256")
    );
    let missing = directory.path().join("nope");
    assert!(run(request(&tables, &missing, &csv, 5)).unwrap_err().contains("folder"));
    let mut invalid = request(&tables, directory.path(), &csv, 5);
    invalid.extension = "../escaped";
    assert!(run(invalid).unwrap_err().contains("extension"));
}

#[test]
fn a_driver_without_snapshot_support_does_not_publish_any_files() {
    let fixture = fixture(&[("items", 3)]);
    let tables = targets(&fixture, &["items"]);
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let error = export_tables(
        test_guard(fixture.conn.clone()),
        &drivers_sqlite::SqliteDriver,
        &request(&tables, directory.path(), &csv, 10),
        &fixture.run,
        &CancellationToken::new(),
        |_, _| {},
    )
    .expect_err("an unsupported snapshot must be refused");
    assert!(error.to_lowercase().contains("snapshot"), "{error}");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

fn test_guard(conn: Arc<dyn Connection>) -> Arc<bookie_policy::PolicyGuard> {
    Arc::new(bookie_policy::PolicyGuard::new(
        conn,
        bookie_policy::GuardContext {
            connection_id: uuid::Uuid::nil(),
            connection_name: "test".into(),
            driver_id: "sqlite".into(),
            environment: bookie_core::Environment::Local,
            read_only: false,
            principal: bookie_policy::Principal::human_gui(),
            policy: Arc::new(bookie_policy::PolicyConfig::default()),
            approval: Arc::new(bookie_policy::AutoApproveSink),
            audit: Arc::new(bookie_policy::NullAuditSink),
            audit_state: Arc::new(bookie_policy::AuditState::new()),
        },
    ))
}
