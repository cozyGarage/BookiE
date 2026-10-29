use super::*;
use std::time::Duration;
use tablepro_core::import::CsvImportOptions;
use tablepro_core::sql_ddl::DraftColumn;
use tablepro_core::{ConnectOptions, DatabaseDriver, Environment, Value};
use tablepro_policy::{
    AuditError, AuditEvent, AuditSink, AuditState, AutoApproveSink, BulkBatch, BulkInsertEnd, BulkInsertRequest,
    GuardContext, PolicyConfig, Principal,
};
use uuid::Uuid;

struct TestAudit;

#[async_trait::async_trait]
impl AuditSink for TestAudit {
    async fn record(&self, _event: AuditEvent) -> Result<(), AuditError> {
        Ok(())
    }
}

fn draft(name: &str, data_type: &str) -> DraftColumn {
    DraftColumn {
        original: None,
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable: true,
        primary_key: false,
        auto_increment: false,
        default_value: None,
        comment: None,
        collation: None,
    }
}

fn choice(create: Option<Vec<DraftColumn>>) -> ImportChoice {
    ImportChoice {
        schema: None,
        table: "people".into(),
        bytes: Vec::new(),
        options: CsvImportOptions::default(),
        mapping: Vec::new(),
        columns: Vec::new(),
        capabilities: tablepro_core::DriverCapabilities::default(),
        create,
    }
}

#[tokio::test]
async fn generated_csv_column_is_skipped_in_preview_and_database_supplies_its_value() {
    let raw: Arc<dyn Connection> = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .expect("connect to SQLite")
        .into();
    let guard = PolicyGuard::new(
        raw,
        GuardContext {
            connection_id: Uuid::new_v4(),
            connection_name: "CSV import preview test".into(),
            driver_id: "sqlite".into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit: Arc::new(TestAudit),
            audit_state: Arc::new(AuditState::new()),
        },
    );
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    guard
        .execute_controlled("CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT)", &control)
        .await
        .expect("create table");
    let columns = guard
        .fetch_columns_controlled(None, "people", &control)
        .await
        .expect("read table columns");
    let bytes = b"id,name\n51,Ada\n52,Grace\n".to_vec();
    let options = CsvImportOptions::default();
    let sheet = read_csv(&bytes, &options, None).expect("read CSV");
    let capabilities = tablepro_core::DriverCapabilities {
        supports_explicit_auto_increment_values: true,
        ..tablepro_core::DriverCapabilities::default()
    };
    let mapping = crate::ui::import_dialog::suggested_existing_mapping(capabilities, &sheet.headers, &columns);
    assert_eq!(mapping, [Some(0), Some(1)]);
    let choice = ImportChoice {
        schema: None,
        table: "people".into(),
        bytes,
        options,
        mapping,
        columns,
        capabilities,
        create: None,
    };
    let plan = plan_for("sqlite", &choice).expect("build import plan");
    assert_eq!(
        plan.columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "name"]
    );
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
            .expect("insert rows");
    }
    assert_eq!(
        guard
            .finish_bulk_insert(&mut scope, BulkInsertEnd::Completed)
            .await
            .expect("finish import"),
        2
    );
    let result = guard
        .query_controlled("SELECT id, name FROM people ORDER BY id", &control)
        .await
        .expect("verify imported rows");
    assert_eq!(
        result.rows,
        [
            vec![Value::Int(51), Value::Text("Ada".into())],
            vec![Value::Int(52), Value::Text("Grace".into())]
        ]
    );
}

#[test]
fn an_import_into_an_existing_table_creates_nothing() {
    assert_eq!(create_statement("postgres", &choice(None)), Ok(None));
}

#[test]
fn a_new_table_is_created_by_one_statement_with_quoted_identifiers() {
    let drafts = vec![draft("id", "BIGINT"), draft("na\"me", "TEXT")];

    let sql = create_statement("postgres", &choice(Some(drafts)))
        .expect("built")
        .expect("a statement");

    assert!(sql.starts_with("CREATE TABLE \"people\""));
    assert!(sql.contains("\"na\"\"me\" TEXT"));
    assert!(!sql.contains(';'));
}

#[test]
fn a_file_name_becomes_a_table_name_that_can_be_an_identifier() {
    assert_eq!(
        suggested_table_name(std::path::Path::new("/tmp/People List 2024.csv")),
        "people_list_2024"
    );
    assert_eq!(
        suggested_table_name(std::path::Path::new("/tmp/---.csv")),
        "imported_table"
    );
}
