#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The MySQL half of the 100,000-row import baseline for #3741 (PostgreSQL and SQLite sit next to
//! their own tests). It needs Docker and prints the time. Run the baseline on its own
//! (`-- --ignored --nocapture a_hundred`) on an idle machine: running it next to the other
//! ignored test in this file makes the number meaningless.

use std::sync::Arc;

use async_trait::async_trait;
use bookie_core::import::{CsvImportOptions, ImportTarget, build_insert_plan, read_csv};
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Environment, OperationControl, TlsConfig, Value};
use bookie_policy::{
    AuditError, AuditEvent, AuditSink, AuditState, AutoApproveSink, BulkBatch, BulkInsertEnd, BulkInsertRequest,
    GuardContext, PolicyConfig, PolicyGuard, Principal,
};
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const BASELINE_ROWS: usize = 100_000;

struct DiscardAudit;

#[async_trait]
impl AuditSink for DiscardAudit {
    async fn record(&self, _event: AuditEvent) -> Result<(), AuditError> {
        Ok(())
    }
}

async fn start_mysql() -> (
    testcontainers::ContainerAsync<testcontainers_modules::mysql::Mysql>,
    ConnectOptions,
) {
    start_mysql_with(&[]).await
}

async fn start_mysql_with(
    extra: &[&str],
) -> (
    testcontainers::ContainerAsync<testcontainers_modules::mysql::Mysql>,
    ConnectOptions,
) {
    let mut cmd = vec![
        "--mysql-native-password=ON".to_string(),
        "--authentication-policy=mysql_native_password".to_string(),
    ];
    cmd.extend(extra.iter().map(|arg| arg.to_string()));
    let container = testcontainers_modules::mysql::Mysql::default()
        .with_tag("8.4.13")
        .with_env_var("MYSQL_ROOT_PASSWORD", "bookie_test")
        .with_cmd(cmd)
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .unwrap();
    let options = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(3306).await.unwrap(),
        database: "test".into(),
        username: "root".into(),
        password: SecretString::from("bookie_test"),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    (container, options)
}

fn guard_for(raw: Box<dyn bookie_core::Connection>) -> PolicyGuard {
    PolicyGuard::new(
        Arc::from(raw),
        GuardContext {
            connection_id: Uuid::nil(),
            connection_name: "mysql guarded test".into(),
            driver_id: "mysql".into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit: Arc::new(DiscardAudit),
            audit_state: Arc::new(AuditState::new()),
        },
    )
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires docker; prints the import baseline measured for #3741"]
async fn a_hundred_thousand_row_import_lands_complete_and_reports_its_time() {
    let (_container, options) = start_mysql().await;
    let raw = drivers_mysql::MysqlDriver.connect(options.clone()).await.unwrap();
    raw.execute("CREATE TABLE import_bulk (id BIGINT PRIMARY KEY, name VARCHAR(100) NOT NULL, amount DECIMAL(10,2))")
        .await
        .unwrap();
    let mut csv = String::from("id,name,amount\n");
    for id in 1..=BASELINE_ROWS {
        csv.push_str(&format!("{id},name {id},{}.25\n", id % 1000));
    }
    let import_options = CsvImportOptions::default();
    let sheet = read_csv(csv.as_bytes(), &import_options, None).unwrap();
    let columns = raw.fetch_columns(None, "import_bulk").await.unwrap();
    let target = ImportTarget {
        driver_id: "mysql",
        capabilities: drivers_mysql::MysqlDriver.capabilities(),
        schema: None,
        table: "import_bulk",
        columns: &columns,
        mapping: &[Some(0), Some(1), Some(2)],
    };
    let plan = build_insert_plan(&target, &sheet, &import_options).unwrap();
    let guard = guard_for(raw);
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
                &OperationControl::new(CancellationToken::new(), None),
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
        "mysql import baseline: {BASELINE_ROWS} rows in {batches} batches took {:.2?} ({:.0} rows/s)",
        elapsed,
        BASELINE_ROWS as f64 / elapsed.as_secs_f64()
    );
    let checker = drivers_mysql::MysqlDriver.connect(options).await.unwrap();
    let result = checker
        .query("SELECT CAST(count(*) AS CHAR) FROM import_bulk")
        .await
        .unwrap();
    assert_eq!(result.rows, vec![vec![Value::Text(BASELINE_ROWS.to_string())]]);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires docker"]
async fn a_slow_write_outlives_the_read_deadline_and_a_slow_read_does_not() {
    let (_container, options) = start_mysql().await;
    let raw = drivers_mysql::MysqlDriver.connect(options.clone()).await.unwrap();
    raw.execute("CREATE TABLE n (id INT PRIMARY KEY)").await.unwrap();
    let values: Vec<String> = (1..=200).map(|id| format!("({id})")).collect();
    raw.execute(&format!("INSERT INTO n VALUES {}", values.join(",")))
        .await
        .unwrap();
    let guard = guard_for(raw);
    let short = || OperationControl::with_timeout(std::time::Duration::from_millis(200));

    let read = guard
        .query_controlled(
            "SELECT count(*) FROM n a CROSS JOIN n b CROSS JOIN n c CROSS JOIN n d WHERE (a.id + b.id * c.id) % d.id = 0",
            &short(),
        )
        .await;
    assert!(
        matches!(read, Err(bookie_core::DriverError::TimedOut)),
        "a read keeps its deadline: {read:?}"
    );

    let write = guard
        .execute_controlled(
            "CREATE TABLE slow_copy AS SELECT a.id FROM n a CROSS JOIN n b CROSS JOIN n c",
            &short(),
        )
        .await;
    assert!(
        write.is_ok(),
        "a write is not cancelled by the read deadline: {write:?}"
    );
    let checker = drivers_mysql::MysqlDriver.connect(options).await.unwrap();
    let count = checker
        .query("SELECT CAST(count(*) AS CHAR) FROM slow_copy")
        .await
        .unwrap();
    assert_eq!(count.rows, vec![vec![Value::Text("8000000".into())]]);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires docker"]
async fn a_non_strict_server_never_stores_a_default_in_place_of_an_imported_null() {
    let (_container, options) = start_mysql_with(&["--sql-mode="]).await;
    let raw = drivers_mysql::MysqlDriver.connect(options.clone()).await.unwrap();
    raw.execute("CREATE TABLE import_strict (id INT PRIMARY KEY, name VARCHAR(20) NOT NULL) ENGINE=InnoDB")
        .await
        .unwrap();
    let import_options = CsvImportOptions {
        null_marker: "NULL".into(),
        ..CsvImportOptions::default()
    };
    let sheet = read_csv(b"id,name\n1,a\n2,NULL\n3,c\n", &import_options, None).unwrap();
    let columns = raw.fetch_columns(None, "import_strict").await.unwrap();
    let target = ImportTarget {
        driver_id: "mysql",
        capabilities: drivers_mysql::MysqlDriver.capabilities(),
        schema: None,
        table: "import_strict",
        columns: &columns,
        mapping: &[Some(0), Some(1)],
    };
    let plan = build_insert_plan(&target, &sheet, &import_options).unwrap();
    let guard = guard_for(raw);
    let mut scope = guard
        .begin_bulk_insert(BulkInsertRequest {
            schema: None,
            table: "import_strict".into(),
            statement: plan.statement.clone(),
            row_budget: plan.row_count(),
            sequence_columns: Vec::new(),
        })
        .await
        .unwrap();
    let mut failed = false;
    for rows in plan.batches(bookie_core::import::DEFAULT_IMPORT_BATCH_ROWS) {
        failed |= guard
            .execute_bulk_batch(
                &mut scope,
                BulkBatch {
                    schema: None,
                    table: "import_strict",
                    statement: &plan.statement,
                    rows,
                },
                &OperationControl::new(CancellationToken::new(), None),
            )
            .await
            .is_err();
    }
    assert!(
        failed,
        "a NULL for a NOT NULL column must fail the batch, not be coerced"
    );
    let checker = drivers_mysql::MysqlDriver.connect(options).await.unwrap();
    let stored = checker
        .query("SELECT CAST(count(*) AS CHAR) FROM import_strict WHERE name = ''")
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![vec![Value::Text("0".into())]],
        "the server's default must never stand in for an imported NULL (ADR 0007)"
    );
}
