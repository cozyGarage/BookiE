use super::list_tables_excluding;
use bookie_core::{AttachedDatabase, ConnectOptions, DatabaseDriver, OperationControl};

#[test]
fn hidden_schema_scope_is_applied_before_catalog_results_reach_consumers() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let driver = drivers_sqlite::SqliteDriver;
        let attached = tempfile::NamedTempFile::new().unwrap();
        let options = ConnectOptions {
            attached_databases: vec![AttachedDatabase::new("hidden", attached.path().to_string_lossy())],
            ..Default::default()
        };
        let connection = driver.connect(options).await.unwrap();
        let control = OperationControl::with_timeout(std::time::Duration::from_secs(5));
        connection
            .execute_controlled("CREATE TABLE visible(id INTEGER)", &control)
            .await
            .unwrap();
        connection
            .execute_controlled("CREATE TABLE hidden.visible(id INTEGER)", &control)
            .await
            .unwrap();
        let tables = list_tables_excluding(connection.as_ref(), &["hidden".to_string()], &control)
            .await
            .unwrap();
        assert!(
            tables
                .iter()
                .any(|table| table.name == "visible" && table.schema.is_none())
        );
        assert!(tables.iter().all(|table| table.schema.as_deref() != Some("hidden")));
    });
}
