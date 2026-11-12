#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bookie_app_services::export_pages::{PagePlan, PageRunner};
use bookie_app_services::snapshot_export::SnapshotPages;
use bookie_core::export::{CsvOptions, ExportError, ResultExport, ResultFormat, write_paged_file};
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Environment, FilterSet, Value};
use bookie_policy::{
    AuditError, AuditEvent, AuditRecordPhase, AuditSink, AuditState, AuditTerminalStatus, AutoApproveSink, GuardContext,
    PolicyConfig, PolicyGuard, Principal,
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
