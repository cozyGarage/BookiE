use std::sync::Arc;

use bookie_core::DriverRegistry;

pub fn default_registry() -> DriverRegistry {
    let mut registry = DriverRegistry::new();
    registry.register(Arc::new(drivers_cassandra::CassandraDriver));
    registry.register(Arc::new(drivers_clickhouse::ClickhouseDriver));
    registry.register(Arc::new(drivers_duckdb::DuckdbDriver));
    registry.register(Arc::new(drivers_mongodb::MongodbDriver));
    registry.register(Arc::new(drivers_mssql::MssqlDriver));
    registry.register(Arc::new(drivers_mysql::MysqlDriver));
    registry.register(Arc::new(drivers_oracle::OracleDriver));
    registry.register(Arc::new(drivers_postgres::PgDriver));
    registry.register(Arc::new(drivers_redis::RedisDriver));
    registry.register(Arc::new(drivers_sqlite::SqliteDriver));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_engine_is_registered_under_its_own_id() {
        let registry = default_registry();
        for id in [
            "cassandra",
            "clickhouse",
            "duckdb",
            "mongodb",
            "mssql",
            "mysql",
            "oracle",
            "postgres",
            "redis",
            "sqlite",
        ] {
            let driver = registry.get(id).unwrap_or_else(|| panic!("{id} is not registered"));
            assert_eq!(driver.id(), id);
        }
    }
}
