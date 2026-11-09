use crate::sql_dialect::quote_ident;

use super::types::{BuildDdlError, validate_catalog_name};

pub fn supports_create_schema(driver_id: &str) -> bool {
    matches!(driver_id, "postgres" | "mssql" | "duckdb")
}

pub fn supports_drop_schema(driver_id: &str) -> bool {
    supports_create_schema(driver_id)
}

pub fn supports_create_database(driver_id: &str) -> bool {
    matches!(driver_id, "postgres" | "mysql" | "mssql" | "clickhouse")
}

pub fn supports_drop_database(driver_id: &str) -> bool {
    supports_create_database(driver_id)
}

pub fn protects_schema(driver_id: &str, name: &str) -> bool {
    match driver_id {
        "postgres" => name == "information_schema" || name.starts_with("pg_"),
        "mssql" => name.eq_ignore_ascii_case("dbo") || name.eq_ignore_ascii_case("sys"),
        "duckdb" => name == "main" || name == "information_schema" || name == "pg_catalog",
        _ => false,
    }
}

pub fn protects_database(driver_id: &str, name: &str) -> bool {
    match driver_id {
        "postgres" => matches!(name, "template0" | "template1"),
        "mysql" => matches!(name, "mysql" | "information_schema" | "performance_schema" | "sys"),
        "mssql" => ["master", "model", "msdb", "tempdb", "resource"]
            .iter()
            .any(|system| name.eq_ignore_ascii_case(system)),
        "clickhouse" => name == "system" || name == "INFORMATION_SCHEMA" || name == "information_schema",
        _ => false,
    }
}

pub fn build_create_schema(driver_id: &str, schema: &str) -> Result<String, BuildDdlError> {
    if !supports_create_schema(driver_id) {
        return Err(BuildDdlError::UnsupportedDriver(driver_id.to_string()));
    }
    let schema = validate_catalog_name(schema)?;
    Ok(format!("CREATE SCHEMA {}", quote_ident(driver_id, schema)))
}

pub fn build_drop_schema(driver_id: &str, schema: &str, cascade: bool) -> Result<String, BuildDdlError> {
    if !supports_drop_schema(driver_id) {
        return Err(BuildDdlError::UnsupportedDriver(driver_id.to_string()));
    }
    let schema = validate_catalog_name(schema)?;
    let mut sql = format!("DROP SCHEMA {}", quote_ident(driver_id, schema));
    if cascade && matches!(driver_id, "postgres" | "duckdb") {
        sql.push_str(" CASCADE");
    }
    Ok(sql)
}

pub fn build_create_database(driver_id: &str, database: &str) -> Result<String, BuildDdlError> {
    if !supports_create_database(driver_id) {
        return Err(BuildDdlError::UnsupportedDriver(driver_id.to_string()));
    }
    let database = validate_catalog_name(database)?;
    Ok(format!("CREATE DATABASE {}", quote_ident(driver_id, database)))
}

pub fn build_drop_database(driver_id: &str, database: &str) -> Result<String, BuildDdlError> {
    if !supports_drop_database(driver_id) {
        return Err(BuildDdlError::UnsupportedDriver(driver_id.to_string()));
    }
    let database = validate_catalog_name(database)?;
    Ok(format!("DROP DATABASE {}", quote_ident(driver_id, database)))
}
