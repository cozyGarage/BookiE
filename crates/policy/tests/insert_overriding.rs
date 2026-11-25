#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use bookie_policy::{StatementClass, classify};

fn class(sql: &str) -> StatementClass {
    classify(sql, "postgres").class
}

#[test]
fn an_insert_that_overrides_an_identity_column_is_still_one_insert() {
    for sql in [
        "INSERT INTO \"t\" (\"id\", \"name\") OVERRIDING SYSTEM VALUE VALUES ($1, $2)",
        "insert into t (id) overriding user value values ($1)",
        "INSERT INTO t (id) /* keep */ OVERRIDING SYSTEM VALUE SELECT id FROM s",
        "INSERT INTO t (id) OVERRIDING SYSTEM VALUE DEFAULT VALUES",
    ] {
        let facts = classify(sql, "postgres");
        assert_eq!(facts.class, StatementClass::Insert, "{sql}");
        assert!(facts.writes(), "{sql}");
        assert!(!facts.is_multi_statement, "{sql}");
    }
}

#[test]
fn the_clause_is_only_accepted_where_postgres_accepts_it() {
    for sql in [
        "SELECT 1 OVERRIDING SYSTEM VALUE",
        "INSERT INTO t (id) VALUES (1) OVERRIDING SYSTEM VALUE",
        "INSERT INTO t (id) OVERRIDING SYSTEM VALUE FROM x",
        "INSERT INTO t (id) OVERRIDING SYSTEM VALUE",
    ] {
        assert_eq!(class(sql), StatementClass::Unparseable, "{sql}");
    }
    assert_eq!(
        class("INSERT INTO t (id) VALUES ('OVERRIDING SYSTEM VALUE')"),
        StatementClass::Insert
    );
    assert_ne!(
        class("INSERT INTO t (id) OVERRIDING SYSTEM VALUE VALUES (1); DROP TABLE t"),
        StatementClass::Insert,
        "a second statement is still seen"
    );
}

#[test]
fn other_engines_keep_refusing_the_clause() {
    assert_eq!(
        classify("INSERT INTO t (id) OVERRIDING SYSTEM VALUE VALUES (1)", "mysql").class,
        StatementClass::Unparseable
    );
}
