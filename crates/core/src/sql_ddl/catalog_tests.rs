use super::{
    build_create_database, build_create_schema, build_drop_database, build_drop_schema, protects_database,
    protects_schema, supports_create_database, supports_create_schema, supports_drop_database, supports_drop_schema,
};

#[test]
fn postgres_schema_round_trip_sql() {
    assert_eq!(
        build_create_schema("postgres", "reporting").unwrap(),
        "CREATE SCHEMA \"reporting\""
    );
    assert_eq!(
        build_drop_schema("postgres", "reporting", false).unwrap(),
        "DROP SCHEMA \"reporting\""
    );
    assert_eq!(
        build_drop_schema("postgres", "reporting", true).unwrap(),
        "DROP SCHEMA \"reporting\" CASCADE"
    );
}

#[test]
fn mssql_schema_drop_omits_unsupported_cascade() {
    assert_eq!(
        build_create_schema("mssql", "reporting").unwrap(),
        "CREATE SCHEMA [reporting]"
    );
    assert_eq!(
        build_drop_schema("mssql", "reporting", true).unwrap(),
        "DROP SCHEMA [reporting]"
    );
    assert_eq!(
        build_drop_schema("mssql", "reporting", false).unwrap(),
        "DROP SCHEMA [reporting]"
    );
}

#[test]
fn duckdb_schema_drop_cascades_when_requested() {
    assert_eq!(
        build_create_schema("duckdb", "reporting").unwrap(),
        "CREATE SCHEMA \"reporting\""
    );
    assert_eq!(
        build_drop_schema("duckdb", "reporting", false).unwrap(),
        "DROP SCHEMA \"reporting\""
    );
    assert_eq!(
        build_drop_schema("duckdb", "reporting", true).unwrap(),
        "DROP SCHEMA \"reporting\" CASCADE"
    );
}

#[test]
fn schema_ddl_is_refused_on_engines_without_schemas() {
    assert!(build_create_schema("sqlite", "reporting").is_err());
    assert!(build_drop_schema("mysql", "reporting", false).is_err());
    assert!(build_create_schema("postgres", "").is_err());
    assert!(build_drop_schema("postgres", "", false).is_err());
}

#[test]
fn database_create_and_drop_sql() {
    assert_eq!(
        build_create_database("postgres", "analytics").unwrap(),
        "CREATE DATABASE \"analytics\""
    );
    assert_eq!(
        build_drop_database("postgres", "analytics").unwrap(),
        "DROP DATABASE \"analytics\""
    );
    assert_eq!(
        build_create_database("mysql", "analytics").unwrap(),
        "CREATE DATABASE `analytics`"
    );
    assert_eq!(
        build_drop_database("mysql", "analytics").unwrap(),
        "DROP DATABASE `analytics`"
    );
    assert_eq!(
        build_create_database("mssql", "analytics").unwrap(),
        "CREATE DATABASE [analytics]"
    );
    assert_eq!(
        build_create_database("clickhouse", "analytics").unwrap(),
        "CREATE DATABASE `analytics`"
    );
}

#[test]
fn database_ddl_is_refused_on_file_engines_and_empty_names() {
    assert!(build_create_database("sqlite", "analytics").is_err());
    assert!(build_drop_database("duckdb", "analytics").is_err());
    assert!(build_create_database("mongodb", "analytics").is_err());
    assert!(build_create_database("postgres", "").is_err());
}

#[test]
fn capability_helpers_match_supported_engines() {
    assert!(supports_create_schema("postgres"));
    assert!(supports_drop_schema("mssql"));
    assert!(!supports_create_schema("mysql"));
    assert!(supports_create_database("mysql"));
    assert!(supports_drop_database("clickhouse"));
    assert!(!supports_create_database("sqlite"));
}

#[test]
fn protected_catalog_names_are_refused_for_drop_offers() {
    assert!(protects_schema("postgres", "information_schema"));
    assert!(protects_schema("postgres", "pg_catalog"));
    assert!(!protects_schema("postgres", "public"));
    assert!(protects_schema("mssql", "dbo"));
    assert!(protects_schema("duckdb", "main"));
    assert!(protects_database("postgres", "template0"));
    assert!(protects_database("postgres", "template1"));
    assert!(!protects_database("postgres", "app"));
    assert!(protects_database("mysql", "mysql"));
    assert!(protects_database("mysql", "information_schema"));
    assert!(protects_database("mssql", "master"));
}
