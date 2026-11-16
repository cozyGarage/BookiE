#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::sql_dialect::build_optimistic_keyed_update;
use bookie_core::{ColumnInfo, Value};
use bookie_policy::{StatementClass, classify};

fn column(name: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "text".into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[test]
fn the_update_a_grid_edit_generates_is_a_plain_update_on_every_sql_engine() {
    let columns = [column("id", true), column("note", false)];
    for driver in ["postgres", "sqlite", "duckdb", "mysql", "mssql"] {
        for old in [Value::Text("alpha".into()), Value::Null] {
            let (sql, _params) = build_optimistic_keyed_update(
                driver,
                None,
                "items",
                &columns,
                &[(1, old, Value::Text("beta".into()))],
                &[Value::Int(1)],
            )
            .unwrap();
            let facts = classify(&sql, driver);
            assert_eq!(
                facts.class,
                StatementClass::Update,
                "{driver} generated SQL the policy cannot parse, so every grid edit would ask for approval: {sql}"
            );
        }
    }
}

#[test]
fn a_default_reset_is_a_plain_update_and_a_dangerous_expression_is_not() {
    use bookie_core::sql_dialect::{ColumnReset, build_keyed_column_reset};
    let columns = [column("id", true), column("note", false)];
    let build = |driver: &str, reset: ColumnReset| {
        build_keyed_column_reset(driver, None, "items", &columns, 1, &reset, &[Value::Int(1)])
            .unwrap()
            .0
    };
    for driver in ["postgres", "mysql", "mssql"] {
        let facts = classify(&build(driver, ColumnReset::Default), driver);
        assert_eq!(facts.class, StatementClass::Update, "{driver}");
    }
    let file_read = build(
        "postgres",
        ColumnReset::Expression("pg_read_file('/etc/passwd')".into()),
    );
    assert_eq!(classify(&file_read, "postgres").class, StatementClass::Administrative);
    let injected = build("postgres", ColumnReset::Expression("1); DROP TABLE items; --".into()));
    assert_ne!(classify(&injected, "postgres").class, StatementClass::Update);
}
