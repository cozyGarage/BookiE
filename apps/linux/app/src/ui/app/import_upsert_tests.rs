use super::*;
use bookie_core::import::{CsvImportOptions, WriteMode};
use bookie_core::{ConnectOptions, DatabaseDriver, Environment, Value};
use bookie_policy::{
    AuditApprovalOutcome, AuditError, AuditErrorCategory, AuditEvent, AuditSink, AuditState, AuditTerminalStatus,
    AutoApproveSink, BulkBatch, BulkInsertEnd, BulkInsertRequest, DenyApprovalSink, GuardContext, PolicyConfig,
    Principal,
};
use std::time::Duration;
use uuid::Uuid;

struct QuietAudit;

#[async_trait::async_trait]
impl AuditSink for QuietAudit {
    async fn record(&self, _event: AuditEvent) -> Result<(), AuditError> {
        Ok(())
    }
}

#[derive(Default)]
struct RecordedAudit(std::sync::Mutex<Vec<AuditEvent>>);

#[async_trait::async_trait]
impl AuditSink for RecordedAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.0.lock().expect("audit lock").push(event);
        Ok(())
    }
}

async fn people_guard() -> PolicyGuard {
    let raw: Arc<dyn Connection> = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .expect("connect to SQLite")
        .into();
    PolicyGuard::new(
        raw,
        GuardContext {
            connection_id: Uuid::new_v4(),
            connection_name: "upsert import test".into(),
            driver_id: "sqlite".into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit: Arc::new(QuietAudit),
            audit_state: Arc::new(AuditState::new()),
        },
    )
}

async fn choice_for(guard: &PolicyGuard, bytes: &[u8], write: WriteMode) -> ImportChoice {
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    let columns = guard
        .fetch_columns_controlled(None, "people", &control)
        .await
        .expect("read table columns");
    let options = CsvImportOptions::default();
    let sheet = read_csv(bytes, &options, None).expect("read CSV");
    let capabilities = drivers_sqlite::SqliteDriver.capabilities();
    let mapping = crate::ui::import_dialog::suggested_existing_mapping(capabilities, &sheet.headers, &columns);
    ImportChoice {
        schema: None,
        table: "people".into(),
        bytes: bytes.to_vec().into(),
        options,
        mapping,
        columns,
        capabilities,
        create: None,
        skip_rejected_rows: false,
        write,
        source: PathBuf::new(),
        more_files: Vec::new(),
    }
}

#[tokio::test]
async fn an_update_on_conflict_import_updates_the_existing_row_and_inserts_the_new_one() {
    let guard = people_guard().await;
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    guard
        .execute_controlled(
            "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, score REAL)",
            &control,
        )
        .await
        .expect("create table");
    guard
        .execute_controlled("INSERT INTO people VALUES (1, 'old', 1.5), (2, 'kept', 2.5)", &control)
        .await
        .expect("seed rows");
    let choice = choice_for(
        &guard,
        b"id,name,score\n1,Ada,9.5\n3,Grace,3.5\n",
        WriteMode::UpdateOnConflict(vec!["id".into()]),
    )
    .await;

    let (plan, _) = prepare_csv_plan(Uuid::new_v4(), "sqlite".into(), choice.clone())
        .await
        .result
        .expect("build import plan");
    assert!(plan.statement.contains("ON CONFLICT (\"id\") DO UPDATE SET"));
    let mut scope = guard
        .begin_bulk_insert(BulkInsertRequest {
            schema: None,
            table: choice.table.clone(),
            statement: plan.statement.clone(),
            row_budget: plan.row_count(),
            sequence_columns: Vec::new(),
        })
        .await
        .expect("approve import");
    for rows in plan.batches(10) {
        guard
            .execute_bulk_batch(
                &mut scope,
                BulkBatch {
                    schema: None,
                    table: &choice.table,
                    statement: &plan.statement,
                    rows,
                },
                &control,
            )
            .await
            .expect("write rows");
    }
    guard
        .finish_bulk_insert(&mut scope, BulkInsertEnd::Completed)
        .await
        .expect("finish import");

    let result = guard
        .query_controlled("SELECT id, name, score FROM people ORDER BY id", &control)
        .await
        .expect("read rows back");
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Int(1), Value::Text("Ada".into()), Value::Float(9.5)],
            vec![Value::Int(2), Value::Text("kept".into()), Value::Float(2.5)],
            vec![Value::Int(3), Value::Text("Grace".into()), Value::Float(3.5)],
        ]
    );
}

#[tokio::test]
async fn denied_import_leaves_the_table_unchanged_and_records_denial() {
    let raw: Arc<dyn Connection> = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .expect("connect to SQLite")
        .into();
    let setup_guard = PolicyGuard::new(
        raw.clone(),
        GuardContext {
            connection_id: Uuid::new_v4(),
            connection_name: "denied import test".into(),
            driver_id: "sqlite".into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit: Arc::new(QuietAudit),
            audit_state: Arc::new(AuditState::new()),
        },
    );
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    setup_guard
        .execute_controlled("CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT)", &control)
        .await
        .expect("create table");
    setup_guard
        .execute_controlled("INSERT INTO people VALUES (1, 'Existing')", &control)
        .await
        .expect("seed table");
    let choice = choice_for(&setup_guard, b"id,name\n1,Changed\n2,New\n", WriteMode::Insert).await;
    let plan = plan_for("sqlite", &choice).expect("build import plan");
    let row_budget = plan.row_count();
    let audit = Arc::new(RecordedAudit::default());
    let deny_guard = PolicyGuard::new(
        raw,
        GuardContext {
            connection_id: Uuid::new_v4(),
            connection_name: "denied import test".into(),
            driver_id: "sqlite".into(),
            environment: Environment::Prod,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(DenyApprovalSink),
            audit: audit.clone(),
            audit_state: Arc::new(AuditState::new()),
        },
    );

    let denied = deny_guard
        .begin_bulk_insert(BulkInsertRequest {
            schema: None,
            table: choice.table,
            statement: plan.statement,
            row_budget,
            sequence_columns: Vec::new(),
        })
        .await;

    assert!(denied.is_err());
    let result = setup_guard
        .query_controlled("SELECT id, name FROM people ORDER BY id", &control)
        .await
        .expect("read rows");
    assert_eq!(result.rows, vec![vec![Value::Int(1), Value::Text("Existing".into())]]);
    let events = audit.0.lock().expect("audit lock");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].terminal_status, AuditTerminalStatus::Denied);
    assert_eq!(events[0].approval_outcome, AuditApprovalOutcome::Denied);
    assert_eq!(events[0].error_category, Some(AuditErrorCategory::Policy));
}

#[tokio::test]
async fn an_insert_only_import_still_stops_at_a_duplicate_key() {
    let guard = people_guard().await;
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    guard
        .execute_controlled(
            "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, score REAL)",
            &control,
        )
        .await
        .expect("create table");
    guard
        .execute_controlled("INSERT INTO people VALUES (1, 'old', 1.5)", &control)
        .await
        .expect("seed row");
    let choice = choice_for(&guard, b"id,name,score\n1,Ada,9.5\n", WriteMode::Insert).await;
    let (plan, _) = prepare_csv_plan(Uuid::new_v4(), "sqlite".into(), choice.clone())
        .await
        .result
        .expect("build import plan");
    assert!(!plan.statement.contains("ON CONFLICT"));
}

#[tokio::test]
async fn a_key_that_the_file_does_not_supply_stops_the_plan_with_a_readable_message() {
    let guard = people_guard().await;
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    guard
        .execute_controlled(
            "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, score REAL)",
            &control,
        )
        .await
        .expect("create table");
    let choice = choice_for(
        &guard,
        b"name,score\nAda,9.5\n",
        WriteMode::UpdateOnConflict(vec!["id".into()]),
    )
    .await;

    let message = prepare_csv_plan(Uuid::new_v4(), "sqlite".into(), choice)
        .await
        .result
        .expect_err("the key is not imported");
    assert!(message.contains("id"));
}
