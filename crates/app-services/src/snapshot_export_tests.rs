use bookie_core::{DatabaseDriver, DriverCapabilities};

fn capabilities(driver: &dyn DatabaseDriver) -> DriverCapabilities {
    driver.capabilities()
}

#[test]
fn only_postgresql_declares_snapshot_export() {
    assert!(capabilities(&drivers_postgres::PgDriver).supports_snapshot_export);
    assert!(!capabilities(&drivers_mysql::MysqlDriver).supports_snapshot_export);
    assert!(!capabilities(&drivers_mssql::MssqlDriver).supports_snapshot_export);
    assert!(!capabilities(&drivers_sqlite::SqliteDriver).supports_snapshot_export);
}
