#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bookie_app_services::export_pages::{PagePlan, PageRunner};
use bookie_app_services::read_session::PostgresTableCursor;
use bookie_app_services::snapshot_export::SnapshotPages;
use bookie_core::export::{CsvOptions, ExportError, ResultExport, ResultFormat, write_paged_file};
use bookie_core::{
    ConnectOptions, Connection, DatabaseDriver, Environment, FilterSet, OperationControl, TableInfo, Value,
};
use bookie_policy::{
    AuditError, AuditEvent, AuditRecordPhase, AuditSink, AuditState, AuditTerminalStatus, AutoApproveSink,
    GuardContext, PolicyConfig, PolicyGuard, Principal,
};
use drivers_postgres::PgDriver;
use secrecy::SecretString;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const ROWS: i64 = 12_000;

#[derive(Default)]
struct CapturingAudit {
    events: Mutex<Vec<AuditEvent>>,
}

#[async_trait]
impl AuditSink for CapturingAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

impl CapturingAudit {
    fn outcomes(&self, status: AuditTerminalStatus) -> usize {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.phase == AuditRecordPhase::Outcome && event.terminal_status == status)
            .count()
    }
}

struct Fixture {
    _container: ContainerAsync<Postgres>,
    options: ConnectOptions,
    admin: Box<dyn Connection>,
}

async fn fixture() -> Fixture {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("start postgres container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: SecretString::from("postgres"),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    };
    let admin = PgDriver.connect(options.clone()).await.expect("connect");
    admin
        .execute("CREATE TABLE snap (id int PRIMARY KEY, label text NOT NULL)")
        .await
        .expect("create table");
    admin
        .execute(&format!(
            "INSERT INTO snap SELECT g, 'row ' || g FROM generate_series(1, {ROWS}) AS g"
        ))
        .await
        .expect("seed");
    Fixture {
        _container: container,
        options,
        admin,
    }
}

async fn guard(fixture: &Fixture, audit: Arc<CapturingAudit>, driver_id: &str) -> Arc<PolicyGuard> {
    let raw = PgDriver.connect(fixture.options.clone()).await.expect("connect");
    Arc::new(PolicyGuard::new(
        Arc::from(raw),
        GuardContext {
            connection_id: Uuid::nil(),
            connection_name: "snapshot-export-test".into(),
            driver_id: driver_id.into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit,
            audit_state: Arc::new(AuditState::new()),
        },
    ))
}

async fn plan(fixture: &Fixture) -> PagePlan {
    PagePlan {
        driver_id: "postgres".into(),
        schema: Some("public".into()),
        table: "snap".into(),
        columns: fixture.admin.fetch_columns(Some("public"), "snap").await.unwrap(),
        filter: FilterSet::default(),
        sort: None,
        row_id_order: false,
        timeout_secs: 60,
    }
}

fn runner() -> PageRunner {
    let handle = tokio::runtime::Handle::current();
    Arc::new(move |future| tokio::task::block_in_place(|| handle.block_on(future)))
}

fn ids(rows: &[Vec<Value>]) -> Vec<i64> {
    rows.iter()
        .map(|row| match row[0] {
            Value::Int(id) => id,
            ref other => panic!("unexpected key {other:?}"),
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn rows_changed_between_pages_do_not_reach_the_export() {
    let fixture = fixture().await;
    let audit = Arc::new(CapturingAudit::default());
    let guard = guard(&fixture, audit.clone(), "postgres").await;
    let plan = plan(&fixture).await;
    let mut pages = SnapshotPages::new(guard, plan, runner(), CancellationToken::new());
    let mut seen = Vec::new();
    let first = pages.next_page().unwrap().unwrap();
    assert_eq!(first.len(), 5_000);
    seen.extend(first);
    for statement in [
        "INSERT INTO snap VALUES (50000, 'late insert')",
        "UPDATE snap SET label = 'changed' WHERE id > 7000",
        "DELETE FROM snap WHERE id > 11000",
    ] {
        fixture.admin.execute(statement).await.expect(statement);
    }
    while let Some(page) = pages.next_page().unwrap() {
        seen.extend(page);
    }
    pages.finish().unwrap();
    assert_eq!(ids(&seen), (1..=ROWS).collect::<Vec<_>>());
    assert!(
        seen.iter()
            .all(|row| row[1] == Value::Text(format!("row {}", ids(std::slice::from_ref(row))[0])))
    );
    let live = fixture.admin.query("SELECT count(*) FROM snap").await.unwrap();
    assert_eq!(live.rows[0][0], Value::Int(11_000));
    assert!(audit.outcomes(AuditTerminalStatus::Succeeded) >= 1);
    assert_eq!(audit.outcomes(AuditTerminalStatus::Failed), 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn live_table_cursor_keeps_a_snapshot_when_the_table_changes_between_pages() {
    let fixture = fixture().await;
    let guard = guard(&fixture, Arc::new(CapturingAudit::default()), "postgres").await;
    let table = TableInfo {
        schema: Some("public".into()),
        name: "snap".into(),
    };
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(60));
    let mut cursor = PostgresTableCursor::open(&guard, &table, &control).await.unwrap();
    let first = cursor.next_page(&control).await.unwrap();
    assert_eq!(first.result.rows.len(), 5_000);
    assert!(first.has_next_page);
    let mut seen: Vec<_> = first.result.rows.iter().map(|row| row.to_vec()).collect();
    for statement in [
        "INSERT INTO snap VALUES (50000, 'late insert')",
        "UPDATE snap SET label = 'changed' WHERE id > 7000",
        "DELETE FROM snap WHERE id > 11000",
    ] {
        fixture.admin.execute(statement).await.expect(statement);
    }
    loop {
        let page = cursor.next_page(&control).await.unwrap();
        seen.extend(page.result.rows.iter().map(|row| row.to_vec()));
        if !page.has_next_page {
            break;
        }
    }
    cursor.close().await.unwrap();
    seen.sort_by_key(|row| match row.first() {
        Some(Value::Int(id)) => *id,
        other => panic!("unexpected row id {other:?}"),
    });
    let expected: Vec<_> = (1..=ROWS)
        .map(|id| vec![Value::Int(id), Value::Text(format!("row {id}"))])
        .collect();
    assert_eq!(seen, expected);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn a_cancelled_export_publishes_no_file_and_audits_the_cancellation() {
    let fixture = fixture().await;
    let audit = Arc::new(CapturingAudit::default());
    let guard = guard(&fixture, audit.clone(), "postgres").await;
    let plan = plan(&fixture).await;
    let cancel = CancellationToken::new();
    let mut pages = SnapshotPages::new(guard, plan, runner(), cancel.clone());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("out.csv");
    let options = CsvOptions::default();
    let export = ResultExport {
        format: ResultFormat::Csv,
        csv: &options,
        sql: None,
    };
    let columns = fixture.admin.fetch_columns(Some("public"), "snap").await.unwrap();
    let stop = cancel.clone();
    let outcome = write_paged_file(
        &path,
        &columns,
        0,
        &export,
        || pages.next_page(),
        || cancel.is_cancelled(),
        |rows| {
            if rows >= 100 {
                stop.cancel();
            }
        },
    );
    assert!(matches!(outcome, Err(ExportError::Cancelled)), "{outcome:?}");
    pages.finish().unwrap();
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    assert!(audit.outcomes(AuditTerminalStatus::Cancelled) >= 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn a_denied_snapshot_publishes_no_file_and_audits_the_denial() {
    let fixture = fixture().await;
    let audit = Arc::new(CapturingAudit::default());
    let guard = guard(&fixture, audit.clone(), "mysql").await;
    let plan = plan(&fixture).await;
    let mut pages = SnapshotPages::new(guard, plan, runner(), CancellationToken::new());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("out.csv");
    let options = CsvOptions::default();
    let export = ResultExport {
        format: ResultFormat::Csv,
        csv: &options,
        sql: None,
    };
    let columns = fixture.admin.fetch_columns(Some("public"), "snap").await.unwrap();
    let outcome = write_paged_file(&path, &columns, 0, &export, || pages.next_page(), || false, |_| {});
    assert!(matches!(outcome, Err(ExportError::Source(_))), "{outcome:?}");
    pages.finish().unwrap();
    assert!(!path.exists());
    assert!(audit.outcomes(AuditTerminalStatus::Denied) >= 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn multi_table_export_keeps_each_snapshot_and_never_replaces_racing_files() {
    use bookie_app_services::export_tables::{TableExportRequest, TableOutcome, TableTarget, export_tables};
    let fixture = fixture().await;
    let guard = guard(&fixture, Arc::new(CapturingAudit::default()), "postgres").await;
    let columns = fixture.admin.fetch_columns(Some("public"), "snap").await.unwrap();
    let targets = vec![TableTarget {
        schema: Some("public".into()),
        table: "snap".into(),
        columns,
    }];
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let request = TableExportRequest {
        driver_id: "postgres",
        tables: &targets,
        directory: directory.path(),
        format: ResultFormat::Csv,
        extension: "csv",
        csv: &csv,
        max_rows_per_table: Some(11_000),
        timeout_secs: 60,
    };
    let changed = std::cell::Cell::new(false);
    let handle = tokio::runtime::Handle::current();
    let results = export_tables(
        guard.clone(),
        &PgDriver,
        &request,
        &runner(),
        &CancellationToken::new(),
        |_, rows| {
            if rows == 1 && !changed.replace(true) {
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        fixture
                            .admin
                            .execute("UPDATE snap SET label = 'changed' WHERE id > 6000")
                            .await
                            .unwrap();
                        fixture
                            .admin
                            .execute("DELETE FROM snap WHERE id > 10000")
                            .await
                            .unwrap();
                    })
                });
            }
        },
    )
    .unwrap();
    assert_eq!(
        results[0].outcome,
        TableOutcome::Written {
            rows: 11_000,
            capped: true
        }
    );
    let path = directory.path().join("public.snap.csv");
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content.lines().count(), 11_001);
    assert!(!content.contains("changed"));
    std::fs::remove_file(&path).unwrap();
    let results = export_tables(
        guard,
        &PgDriver,
        &request,
        &runner(),
        &CancellationToken::new(),
        |_, rows| {
            if rows == 1 {
                std::fs::write(&path, "other writer").unwrap();
            }
        },
    )
    .unwrap();
    assert!(matches!(results[0].outcome, TableOutcome::Failed(_)));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "other writer");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn multi_table_export_stops_on_cancellation_and_keeps_completed_files() {
    use bookie_app_services::export_tables::{TableExportRequest, TableOutcome, TableTarget, export_tables};
    let fixture = fixture().await;
    fixture
        .admin
        .execute("CREATE TABLE small AS SELECT * FROM snap WHERE id <= 3")
        .await
        .unwrap();
    fixture
        .admin
        .execute("CREATE TABLE empty AS SELECT * FROM snap WHERE false")
        .await
        .unwrap();
    let columns = fixture.admin.fetch_columns(Some("public"), "snap").await.unwrap();
    let targets: Vec<_> = ["small", "snap", "empty"]
        .into_iter()
        .map(|table| TableTarget {
            schema: Some("public".into()),
            table: table.into(),
            columns: columns.clone(),
        })
        .collect();
    let guard = guard(&fixture, Arc::new(CapturingAudit::default()), "postgres").await;
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let request = TableExportRequest {
        driver_id: "postgres",
        tables: &targets,
        directory: directory.path(),
        format: ResultFormat::Csv,
        extension: "csv",
        csv: &csv,
        max_rows_per_table: Some(4),
        timeout_secs: 60,
    };
    let cancel = CancellationToken::new();
    let results = export_tables(guard, &PgDriver, &request, &runner(), &cancel, |index, rows| {
        if index == 1 && rows == 1 {
            cancel.cancel();
        }
    })
    .unwrap();
    assert_eq!(results[0].outcome, TableOutcome::Written { rows: 3, capped: false });
    assert!(matches!(results[1].outcome, TableOutcome::Failed(_)));
    assert_eq!(results[2].outcome, TableOutcome::NotStarted);
    assert!(directory.path().join("public.small.csv").exists());
    assert!(!directory.path().join("public.snap.csv").exists());
    assert!(!directory.path().join("public.empty.csv").exists());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn multi_table_export_applies_the_limit_independently_to_each_table() {
    use bookie_app_services::export_tables::{TableExportRequest, TableOutcome, TableTarget, export_tables};
    let fixture = fixture().await;
    fixture
        .admin
        .execute("CREATE TABLE small AS SELECT * FROM snap WHERE id <= 3")
        .await
        .unwrap();
    let columns = fixture.admin.fetch_columns(Some("public"), "snap").await.unwrap();
    let tables: Vec<_> = ["small", "snap"]
        .into_iter()
        .map(|table| TableTarget {
            schema: Some("public".into()),
            table: table.into(),
            columns: columns.clone(),
        })
        .collect();
    let guard = guard(&fixture, Arc::new(CapturingAudit::default()), "postgres").await;
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let request = TableExportRequest {
        driver_id: "postgres",
        tables: &tables,
        directory: directory.path(),
        format: ResultFormat::Csv,
        extension: "csv",
        csv: &csv,
        max_rows_per_table: Some(2),
        timeout_secs: 60,
    };
    let results = export_tables(
        guard,
        &PgDriver,
        &request,
        &runner(),
        &CancellationToken::new(),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(results.len(), 2);
    for (result, table) in results.iter().zip(["small", "snap"]) {
        assert_eq!(result.outcome, TableOutcome::Written { rows: 2, capped: true });
        assert_eq!(
            std::fs::read_to_string(directory.path().join(format!("public.{table}.csv")))
                .unwrap()
                .lines()
                .count(),
            3
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn multi_table_export_reports_empty_missing_and_existing_destinations() {
    use bookie_app_services::export_tables::{TableExportRequest, TableOutcome, TableTarget, export_tables};
    let fixture = fixture().await;
    fixture
        .admin
        .execute("CREATE TABLE empty AS SELECT * FROM snap WHERE false")
        .await
        .unwrap();
    let columns = fixture.admin.fetch_columns(Some("public"), "snap").await.unwrap();
    let tables: Vec<_> = ["snap", "missing", "empty"]
        .into_iter()
        .map(|table| TableTarget {
            schema: Some("public".into()),
            table: table.into(),
            columns: columns.clone(),
        })
        .collect();
    let guard = guard(&fixture, Arc::new(CapturingAudit::default()), "postgres").await;
    let directory = tempfile::tempdir().unwrap();
    let existing = directory.path().join("public.snap.csv");
    std::fs::write(&existing, "precious").unwrap();
    let csv = CsvOptions::default();
    let request = TableExportRequest {
        driver_id: "postgres",
        tables: &tables,
        directory: directory.path(),
        format: ResultFormat::Csv,
        extension: "csv",
        csv: &csv,
        max_rows_per_table: Some(4),
        timeout_secs: 60,
    };
    let results = export_tables(
        guard,
        &PgDriver,
        &request,
        &runner(),
        &CancellationToken::new(),
        |_, _| {},
    )
    .unwrap();
    assert!(matches!(&results[0].outcome, TableOutcome::Failed(message) if message.contains("already exists")));
    assert!(matches!(results[1].outcome, TableOutcome::Failed(_)));
    assert_eq!(results[2].outcome, TableOutcome::Written { rows: 0, capped: false });
    assert_eq!(std::fs::read_to_string(existing).unwrap(), "precious");
    assert!(!directory.path().join("public.missing.csv").exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn multi_table_export_without_a_row_limit_writes_the_full_snapshot() {
    use bookie_app_services::export_tables::{TableExportRequest, TableOutcome, TableTarget, export_tables};
    let fixture = fixture().await;
    let guard = guard(&fixture, Arc::new(CapturingAudit::default()), "postgres").await;
    let targets = vec![TableTarget {
        schema: Some("public".into()),
        table: "snap".into(),
        columns: fixture.admin.fetch_columns(Some("public"), "snap").await.unwrap(),
    }];
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let request = TableExportRequest {
        driver_id: "postgres",
        tables: &targets,
        directory: directory.path(),
        format: ResultFormat::Csv,
        extension: "csv",
        csv: &csv,
        max_rows_per_table: None,
        timeout_secs: 60,
    };
    let results = export_tables(
        guard,
        &PgDriver,
        &request,
        &runner(),
        &CancellationToken::new(),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(
        results[0].outcome,
        TableOutcome::Written {
            rows: 12_000,
            capped: false
        }
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("public.snap.csv"))
            .unwrap()
            .lines()
            .count(),
        12_001
    );
}

struct UnavailableAudit;

#[async_trait]
impl AuditSink for UnavailableAudit {
    async fn record(&self, _event: AuditEvent) -> Result<(), AuditError> {
        Err(AuditError::Unavailable("test audit unavailable".into()))
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test snapshot_export_postgres -- --include-ignored"]
async fn multi_table_export_does_not_publish_when_snapshot_audit_fails() {
    use bookie_app_services::export_tables::{TableExportRequest, TableOutcome, TableTarget, export_tables};
    let fixture = fixture().await;
    let raw = PgDriver.connect(fixture.options.clone()).await.unwrap();
    let guard = Arc::new(PolicyGuard::new(
        Arc::from(raw),
        GuardContext {
            connection_id: Uuid::nil(),
            connection_name: "snapshot-export-test".into(),
            driver_id: "postgres".into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit: Arc::new(UnavailableAudit),
            audit_state: Arc::new(AuditState::new()),
        },
    ));
    let tables = [TableTarget {
        schema: Some("public".into()),
        table: "snap".into(),
        columns: fixture.admin.fetch_columns(Some("public"), "snap").await.unwrap(),
    }];
    let directory = tempfile::tempdir().unwrap();
    let csv = CsvOptions::default();
    let request = TableExportRequest {
        driver_id: "postgres",
        tables: &tables,
        directory: directory.path(),
        format: ResultFormat::Csv,
        extension: "csv",
        csv: &csv,
        max_rows_per_table: Some(4),
        timeout_secs: 60,
    };
    let results = export_tables(
        guard,
        &PgDriver,
        &request,
        &runner(),
        &CancellationToken::new(),
        |_, _| {},
    )
    .unwrap();
    assert!(matches!(results[0].outcome, TableOutcome::Failed(_)));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
