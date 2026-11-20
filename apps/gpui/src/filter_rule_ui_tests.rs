use bookie_core::{ColumnInfo, FilterOp, FilterRule, FilterValue, TableInfo, Value};

use super::filter_statement;

fn column(name: &str, data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn table() -> TableInfo {
    TableInfo {
        schema: Some("public".into()),
        name: "orders".into(),
    }
}

fn rule(column: &str, op: FilterOp, value: Option<FilterValue>) -> FilterRule {
    FilterRule {
        column: column.into(),
        op,
        value,
    }
}

#[test]
fn a_rule_becomes_a_select_over_the_qualified_table_with_bound_values() {
    let columns = [column("id", "integer"), column("customer", "text")];
    let (sql, params) = filter_statement(
        "postgres",
        &table(),
        &columns,
        &rule("customer", FilterOp::StartsWith, Some(FilterValue::Single("Ad".into()))),
    )
    .unwrap();
    assert!(sql.starts_with("SELECT * FROM \"public\".\"orders\" WHERE "), "{sql}");
    assert!(sql.contains("\"customer\""), "{sql}");
    assert_eq!(params, vec![Value::Text("Ad%".into())]);
}

#[test]
fn between_and_in_bind_one_value_per_bound_and_member() {
    let columns = [column("id", "integer")];
    let between = rule("id", FilterOp::Between, Some(FilterValue::Pair("2".into(), "5".into())));
    assert_eq!(
        filter_statement("postgres", &table(), &columns, &between)
            .unwrap()
            .1
            .len(),
        2
    );
    let list = rule(
        "id",
        FilterOp::In,
        Some(FilterValue::List(vec!["1".into(), "2".into(), "3".into()])),
    );
    assert_eq!(
        filter_statement("postgres", &table(), &columns, &list).unwrap().1.len(),
        3
    );
}

#[test]
fn a_null_test_binds_nothing() {
    let columns = [column("id", "integer")];
    let (sql, params) = filter_statement("postgres", &table(), &columns, &rule("id", FilterOp::IsNull, None)).unwrap();
    assert!(sql.ends_with("IS NULL"), "{sql}");
    assert!(params.is_empty());
}

#[test]
fn an_unknown_column_or_a_value_of_the_wrong_type_is_refused() {
    let columns = [column("id", "integer")];
    let unknown = rule("nope", FilterOp::Eq, Some(FilterValue::Single("1".into())));
    assert!(filter_statement("postgres", &table(), &columns, &unknown).is_err());
    let wrong = rule("id", FilterOp::Eq, Some(FilterValue::Single("abc".into())));
    assert!(filter_statement("postgres", &table(), &columns, &wrong).is_err());
}
