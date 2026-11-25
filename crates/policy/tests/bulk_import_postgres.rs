#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bookie_core::import::{CsvImportOptions, ImportTarget, WriteMode, build_insert_plan, read_csv};
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, DriverError, Environment, OperationControl, Value};
use bookie_policy::{
    ApprovalSink, AuditError, AuditEvent, AuditSink, AuditState, AuditTerminalStatus, AutoApproveSink, BulkBatch,
    BulkInsertEnd, BulkInsertRequest, DenyApprovalSink, GuardContext, PolicyConfig, PolicyGuard, Principal,
};
use drivers_postgres::PgDriver;
use rust_decimal::Decimal;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct CapturingAudit {
    events: Mutex<Vec<AuditEvent>>,
}

#[async_trait]
impl AuditSink for CapturingAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.events.lock().expect("event lock").push(event);
        Ok(())
    }
}

async fn start_pg() -> (ContainerAsync<Postgres>, ConnectOptions) {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("start postgres container");
    let opts = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    };
    (container, opts)
}

fn guard_for(
    connection: Box<dyn Connection>,
    environment: Environment,
    approval: Arc<dyn ApprovalSink>,
    audit: Arc<CapturingAudit>,
) -> PolicyGuard {
    PolicyGuard::new(
        Arc::from(connection),
        GuardContext {
            connection_id: Uuid::nil(),
            connection_name: "bulk-import-postgres-test".into(),
            driver_id: "postgres".into(),
            environment,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval,
            audit,
            audit_state: Arc::new(AuditState::new()),
        },
    )
}

fn control() -> OperationControl {
    OperationControl::new(CancellationToken::new(), None)
}

async fn upsert_plan(connection: &dyn Connection) -> bookie_core::import::InsertPlan {
    let options = CsvImportOptions::default();
    let sheet = read_csv(
        b"id,name,amount,seen\n1,renamed,2.50,2024-05-06 07:08:09\n3,fresh,7.25,2024-06-01 00:00:00\n",
        &options,
        None,
    )
    .unwrap();
    let columns = connection.fetch_columns(None, "import_upsert").await.unwrap();
    let target = ImportTarget {
        driver_id: "postgres",
        capabilities: PgDriver.capabilities(),
        schema: None,
        table: "import_upsert",
        columns: &columns,
        mapping: &[Some(0), Some(1), Some(2), Some(3)],
    };
    let plan = build_insert_plan(&target, &sheet, &options).unwrap();
    WriteMode::UpdateOnConflict(vec!["id".into()])
        .apply(plan, &target)
        .unwrap()
}

async fn prepare_table(connection: &dyn Connection) {
    connection
        .execute("CREATE TABLE import_upsert (id bigint PRIMARY KEY, name text NOT NULL, amount numeric(10,2), seen timestamp)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO import_upsert VALUES (1, 'original', 1.00, '2020-01-01 00:00:00'), (2, 'untouched', 9.99, NULL)")
        .await
        .unwrap();
}

fn request_for(plan: &bookie_core::import::InsertPlan) -> BulkInsertRequest {
    BulkInsertRequest {
        schema: None,
        table: "import_upsert".into(),
        statement: plan.statement.clone(),
        row_budget: plan.row_count(),
        sequence_columns: Vec::new(),
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_approved_upsert_updates_the_existing_row_inserts_the_new_one_and_keeps_types() {
    let (_container, opts) = start_pg().await;
    let raw = PgDriver.connect(opts.clone()).await.unwrap();
    prepare_table(raw.as_ref()).await;
    let plan = upsert_plan(raw.as_ref()).await;
    assert!(plan.statement.contains("ON CONFLICT (\"id\") DO UPDATE SET"));
    let audit = Arc::new(CapturingAudit {
        events: Mutex::new(Vec::new()),
    });
    let guard = guard_for(raw, Environment::Local, Arc::new(AutoApproveSink), audit.clone());

    let mut scope = guard.begin_bulk_insert(request_for(&plan)).await.unwrap();
    for rows in plan.batches(500) {
        guard
            .execute_bulk_batch(
                &mut scope,
                BulkBatch {
                    schema: None,
                    table: "import_upsert",
                    statement: &plan.statement,
                    rows,
                },
                &control(),
            )
            .await
            .unwrap();
    }
    guard
        .finish_bulk_insert(&mut scope, BulkInsertEnd::Completed)
        .await
        .unwrap();

    let checker = PgDriver.connect(opts).await.unwrap();
    let result = checker
        .query("SELECT id, name, amount, seen FROM import_upsert ORDER BY id")
        .await
        .unwrap();
    let seen = |text: &str| Value::DateTime(chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").unwrap());
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("renamed".into()),
                Value::Decimal(Decimal::from_str("2.50").unwrap()),
                seen("2024-05-06 07:08:09"),
            ],
            vec![
                Value::Int(2),
                Value::Text("untouched".into()),
                Value::Decimal(Decimal::from_str("9.99").unwrap()),
                Value::Null,
            ],
            vec![
                Value::Int(3),
                Value::Text("fresh".into()),
                Value::Decimal(Decimal::from_str("7.25").unwrap()),
                seen("2024-06-01 00:00:00"),
            ],
        ]
    );
    let events = audit.events.lock().unwrap();
    let terminal: Vec<_> = events
        .iter()
        .filter(|event| event.terminal_status == AuditTerminalStatus::Succeeded)
        .collect();
    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0].rows_affected, Some(2));
    assert!(terminal[0].targets[0].contains("existing rows may be updated"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_denied_upsert_changes_nothing_and_leaves_a_denied_audit_record() {
    let (_container, opts) = start_pg().await;
    let raw = PgDriver.connect(opts.clone()).await.unwrap();
    prepare_table(raw.as_ref()).await;
    let plan = upsert_plan(raw.as_ref()).await;
    let audit = Arc::new(CapturingAudit {
        events: Mutex::new(Vec::new()),
    });
    let guard = guard_for(raw, Environment::Prod, Arc::new(DenyApprovalSink), audit.clone());

    let error = guard.begin_bulk_insert(request_for(&plan)).await.err().unwrap();

    assert!(matches!(error, DriverError::PolicyDenied(_)));
    let checker = PgDriver.connect(opts).await.unwrap();
    let result = checker
        .query("SELECT name FROM import_upsert ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Text("original".into())],
            vec![Value::Text("untouched".into())]
        ]
    );
    let events = audit.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].terminal_status, AuditTerminalStatus::Denied);
}

const BASELINE_ROWS: usize = 100_000;

#[tokio::test]
#[ignore = "requires docker; prints the import baseline measured for #3741"]
async fn a_hundred_thousand_row_import_lands_complete_and_reports_its_time() {
    let (_container, opts) = start_pg().await;
    let raw = PgDriver.connect(opts.clone()).await.unwrap();
    raw.execute("CREATE TABLE import_bulk (id bigint PRIMARY KEY, name text NOT NULL, amount numeric(10,2))")
        .await
        .unwrap();
    let mut csv = String::from("id,name,amount\n");
    for id in 1..=BASELINE_ROWS {
        csv.push_str(&format!("{id},name {id},{}.25\n", id % 1000));
    }
    let options = CsvImportOptions::default();
    let sheet = read_csv(csv.as_bytes(), &options, None).unwrap();
    let columns = raw.fetch_columns(None, "import_bulk").await.unwrap();
    let target = ImportTarget {
        driver_id: "postgres",
        capabilities: PgDriver.capabilities(),
        schema: None,
        table: "import_bulk",
        columns: &columns,
        mapping: &[Some(0), Some(1), Some(2)],
    };
    let plan = build_insert_plan(&target, &sheet, &options).unwrap();
    assert_eq!(plan.row_count() as usize, BASELINE_ROWS);
    let audit = Arc::new(CapturingAudit {
        events: Mutex::new(Vec::new()),
    });
    let guard = guard_for(raw, Environment::Local, Arc::new(AutoApproveSink), audit);
    let mut scope = guard
        .begin_bulk_insert(BulkInsertRequest {
            schema: None,
            table: "import_bulk".into(),
            statement: plan.statement.clone(),
            row_budget: plan.row_count(),
            sequence_columns: Vec::new(),
        })
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let mut batches = 0_u32;
    for rows in plan.batches(bookie_core::import::DEFAULT_IMPORT_BATCH_ROWS) {
        guard
            .execute_bulk_batch(
                &mut scope,
                BulkBatch {
                    schema: None,
                    table: "import_bulk",
                    statement: &plan.statement,
                    rows,
                },
                &control(),
            )
            .await
            .unwrap();
        batches += 1;
    }
    let elapsed = started.elapsed();
    guard
        .finish_bulk_insert(&mut scope, BulkInsertEnd::Completed)
        .await
        .unwrap();
    println!(
        "import baseline: {BASELINE_ROWS} rows in {batches} batches took {:.2?} ({:.0} rows/s)",
        elapsed,
        BASELINE_ROWS as f64 / elapsed.as_secs_f64()
    );
    let checker = PgDriver.connect(opts).await.unwrap();
    let result = checker.query("SELECT count(*)::text FROM import_bulk").await.unwrap();
    assert_eq!(result.rows, vec![vec![Value::Text(BASELINE_ROWS.to_string())]]);
}

async fn next_generated_id_after_importing_1200_explicit_keys(batches_to_run: usize, cancel: bool) -> String {
    let (_container, opts) = start_pg().await;
    let raw = PgDriver.connect(opts.clone()).await.unwrap();
    raw.execute("CREATE TABLE import_seq (id bigserial PRIMARY KEY, name text NOT NULL)")
        .await
        .unwrap();
    let mut csv = String::from("id,name\n");
    for id in 1..=1_200 {
        csv.push_str(&format!("{id},name {id}\n"));
    }
    let options = CsvImportOptions::default();
    let sheet = read_csv(csv.as_bytes(), &options, None).unwrap();
    let columns = raw.fetch_columns(None, "import_seq").await.unwrap();
    let target = ImportTarget {
        driver_id: "postgres",
        capabilities: PgDriver.capabilities(),
        schema: None,
        table: "import_seq",
        columns: &columns,
        mapping: &[Some(0), Some(1)],
    };
    let plan = build_insert_plan(&target, &sheet, &options).unwrap();
    assert!(plan.statement.contains("OVERRIDING SYSTEM VALUE"));
    let audit = Arc::new(CapturingAudit {
        events: Mutex::new(Vec::new()),
    });
    let guard = guard_for(raw, Environment::Local, Arc::new(AutoApproveSink), audit);
    let mut scope = guard
        .begin_bulk_insert(BulkInsertRequest {
            schema: None,
            table: "import_seq".into(),
            statement: plan.statement.clone(),
            row_budget: plan.row_count(),
            sequence_columns: vec!["id".into()],
        })
        .await
        .unwrap();
    for rows in plan.batches(500).take(batches_to_run) {
        guard
            .execute_bulk_batch(
                &mut scope,
                BulkBatch {
                    schema: None,
                    table: "import_seq",
                    statement: &plan.statement,
                    rows,
                },
                &control(),
            )
            .await
            .unwrap();
    }
    guard
        .finish_bulk_insert(&mut scope, BulkInsertEnd::Completed)
        .await
        .unwrap();
    let checker = PgDriver.connect(opts).await.unwrap();
    let next = checker
        .query("INSERT INTO import_seq (name) VALUES ('generated') RETURNING id::text")
        .await
        .unwrap();
    match &next.rows[0][0] {
        Value::Text(id) => id.clone(),
        other => panic!("unexpected id value {other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_import_with_explicit_keys_leaves_the_sequence_at_the_largest_key() {
    assert_eq!(
        next_generated_id_after_importing_1200_explicit_keys(3, false).await,
        "1201"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_cancelled_import_still_moves_the_sequence_past_the_rows_that_committed() {
    assert_eq!(
        next_generated_id_after_importing_1200_explicit_keys(1, true).await,
        "501"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_slow_write_outlives_the_read_deadline_and_a_slow_read_does_not() {
    let (_container, opts) = start_pg().await;
    let raw = PgDriver.connect(opts.clone()).await.unwrap();
    raw.execute("CREATE TABLE slow (n integer)").await.unwrap();
    let audit = Arc::new(CapturingAudit {
        events: Mutex::new(Vec::new()),
    });
    let guard = guard_for(raw, Environment::Local, Arc::new(AutoApproveSink), audit);
    let short = || OperationControl::with_timeout(std::time::Duration::from_millis(100));

    let read = guard
        .query_controlled("SELECT count(*) FROM generate_series(1, 2000000000)", &short())
        .await;
    assert!(
        matches!(read, Err(DriverError::TimedOut)),
        "a read keeps its deadline: {read:?}"
    );

    let write = guard
        .execute_controlled("INSERT INTO slow SELECT g FROM generate_series(1, 5000000) g", &short())
        .await;
    assert!(
        write.is_ok(),
        "a write is not cancelled by the read deadline: {write:?}"
    );
    let checker = PgDriver.connect(opts).await.unwrap();
    let count = checker.query("SELECT count(*)::text FROM slow").await.unwrap();
    assert_eq!(count.rows, vec![vec![Value::Text("5000000".into())]]);
}
