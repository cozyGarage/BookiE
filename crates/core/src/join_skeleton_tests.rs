use super::*;
use crate::ForeignKeyInfo;

fn key(
    name: &str,
    columns: &[&str],
    ref_schema: Option<&str>,
    ref_table: &str,
    ref_columns: &[&str],
) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.into(),
        columns: columns.iter().map(|column| column.to_string()).collect(),
        ref_schema: ref_schema.map(str::to_string),
        ref_table: ref_table.into(),
        ref_columns: ref_columns.iter().map(|column| column.to_string()).collect(),
        on_delete: None,
        on_update: None,
    }
}

#[test]
fn a_two_table_join_is_quoted_for_postgres() {
    let keys = [key("orders_user_fk", &["user_id"], Some("public"), "users", &["id"])];
    assert_eq!(
        join_skeleton("postgres", Some("public"), "orders", &keys).unwrap(),
        "SELECT t0.*, t1.*\nFROM \"public\".\"orders\" AS t0\nLEFT JOIN \"public\".\"users\" AS t1 ON t0.\"user_id\" = t1.\"id\"\nLIMIT 100;"
    );
}

#[test]
fn mysql_quotes_with_backticks_and_joins_every_foreign_key() {
    let keys = [
        key("a", &["user_id"], None, "users", &["id"]),
        key("b", &["shop_id"], None, "shops", &["id"]),
    ];
    assert_eq!(
        join_skeleton("mysql", None, "orders", &keys).unwrap(),
        "SELECT t0.*, t1.*, t2.*\nFROM `orders` AS t0\nLEFT JOIN `users` AS t1 ON t0.`user_id` = t1.`id`\nLEFT JOIN `shops` AS t2 ON t0.`shop_id` = t2.`id`\nLIMIT 100;"
    );
}

#[test]
fn a_composite_key_joins_on_every_column_pair() {
    let keys = [key("fk", &["tenant", "user_id"], None, "users", &["tenant", "id"])];
    let sql = join_skeleton("postgres", None, "orders", &keys).unwrap();
    assert!(
        sql.contains("ON t0.\"tenant\" = t1.\"tenant\" AND t0.\"user_id\" = t1.\"id\""),
        "{sql}"
    );
}

#[test]
fn a_self_reference_gets_its_own_alias_instead_of_a_cycle() {
    let keys = [key("parent_fk", &["parent_id"], None, "nodes", &["id"])];
    let sql = join_skeleton("postgres", None, "nodes", &keys).unwrap();
    assert!(
        sql.contains("FROM \"nodes\" AS t0\nLEFT JOIN \"nodes\" AS t1 ON t0.\"parent_id\" = t1.\"id\""),
        "{sql}"
    );
}

#[test]
fn identifiers_with_quotes_cannot_break_out() {
    let keys = [key("fk", &["a\"b"], None, "we\"ird", &["id"])];
    let sql = join_skeleton("postgres", None, "t\"x", &keys).unwrap();
    assert!(
        sql.contains("\"t\"\"x\"") && sql.contains("\"we\"\"ird\"") && sql.contains("t0.\"a\"\"b\""),
        "{sql}"
    );
}

#[test]
fn no_foreign_keys_and_malformed_keys_are_refused_with_a_reason() {
    assert_eq!(
        join_skeleton("postgres", None, "orders", &[]),
        Err(JoinSkeletonError::NoForeignKeys)
    );
    for bad in [
        key("empty", &[], None, "users", &[]),
        key("uneven", &["a", "b"], None, "users", &["id"]),
        key("unnamed", &["a"], None, "", &["id"]),
    ] {
        assert!(matches!(
            join_skeleton("postgres", None, "orders", &[bad]),
            Err(JoinSkeletonError::InvalidKey(_))
        ));
    }
}

#[test]
fn only_engines_with_the_reviewed_syntax_are_offered_a_skeleton() {
    let keys = [key("fk", &["a"], None, "b", &["id"])];
    for driver in ["sqlite", "mssql", "clickhouse", "duckdb", "mongodb", "redis"] {
        assert!(
            matches!(
                join_skeleton(driver, None, "t", &keys),
                Err(JoinSkeletonError::Unsupported(_))
            ),
            "{driver}"
        );
    }
}

#[test]
fn more_joins_than_the_limit_are_refused_rather_than_truncated() {
    let keys: Vec<ForeignKeyInfo> = (0..=MAX_JOINS)
        .map(|index| key(&format!("fk{index}"), &["a"], None, &format!("t{index}"), &["id"]))
        .collect();
    assert_eq!(
        join_skeleton("postgres", None, "base", &keys),
        Err(JoinSkeletonError::TooManyForeignKeys)
    );
}
