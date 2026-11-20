use super::*;
use bookie_core::FilterOp;

fn foreign_key(columns: &[&str], ref_columns: &[&str], ref_schema: Option<&str>) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: "fk".into(),
        columns: columns.iter().map(|name| name.to_string()).collect(),
        ref_schema: ref_schema.map(str::to_owned),
        ref_table: "customers".into(),
        ref_columns: ref_columns.iter().map(|name| name.to_string()).collect(),
        on_delete: None,
        on_update: None,
    }
}

fn row(cells: &[(&str, Value)]) -> Vec<(String, Value)> {
    cells
        .iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect()
}

#[test]
fn a_single_column_reference_filters_the_referenced_column_by_the_cell_value() {
    let keys = [foreign_key(&["customer_id"], &["id"], Some("sales"))];
    let target = reference_targets(
        &keys,
        Some("public"),
        "customer_id",
        &row(&[("order_id", Value::Int(7)), ("customer_id", Value::Int(42))]),
    )
    .unwrap();
    assert_eq!(target[0].schema.as_deref(), Some("sales"));
    assert_eq!(target[0].table, "customers");
    assert_eq!(target[0].filter.rules.len(), 1);
    assert_eq!(target[0].filter.rules[0].column, "id");
    assert_eq!(target[0].filter.rules[0].op, FilterOp::Eq);
}

#[test]
fn a_usable_foreign_key_is_still_offered_when_a_sibling_key_has_a_null_column() {
    let keys = [
        foreign_key(&["tenant", "customer"], &["tenant_id", "id"], None),
        foreign_key(&["tenant", "org"], &["tenant_id", "id"], None),
    ];
    let targets = reference_targets(
        &keys,
        None,
        "tenant",
        &row(&[
            ("tenant", Value::Int(1)),
            ("customer", Value::Null),
            ("org", Value::Int(5)),
        ]),
    )
    .unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].filter.rules.len(), 2);
}

#[test]
fn a_reference_without_a_schema_stays_in_the_current_schema() {
    let keys = [foreign_key(&["customer_id"], &["id"], None)];
    let target = reference_targets(
        &keys,
        Some("public"),
        "customer_id",
        &row(&[("customer_id", Value::Int(1))]),
    )
    .unwrap();
    assert_eq!(target[0].schema.as_deref(), Some("public"));
}

#[test]
fn a_composite_reference_filters_every_referenced_column_from_the_same_row() {
    let keys = [foreign_key(&["tenant", "customer"], &["tenant_id", "id"], None)];
    let target = reference_targets(
        &keys,
        None,
        "customer",
        &row(&[("tenant", Value::Int(3)), ("customer", Value::Int(9))]),
    )
    .unwrap();
    let columns: Vec<&str> = target[0].filter.rules.iter().map(|rule| rule.column.as_str()).collect();
    assert_eq!(columns, ["tenant_id", "id"]);
}

#[test]
fn a_shared_column_returns_every_matching_foreign_key_target() {
    let mut organization_key = foreign_key(&["tenant", "organization"], &["tenant_id", "id"], Some("orgs"));
    organization_key.ref_table = "organizations".into();
    let keys = [
        foreign_key(&["tenant", "customer"], &["tenant_id", "id"], Some("sales")),
        organization_key,
    ];
    let targets = reference_targets(
        &keys,
        Some("public"),
        "tenant",
        &row(&[
            ("tenant", Value::Int(3)),
            ("customer", Value::Int(9)),
            ("organization", Value::Int(12)),
        ]),
    )
    .unwrap();

    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].schema.as_deref(), Some("sales"));
    assert_eq!(
        targets[0]
            .filter
            .rules
            .iter()
            .map(|rule| rule.column.as_str())
            .collect::<Vec<_>>(),
        ["tenant_id", "id"]
    );
    assert_eq!(targets[1].schema.as_deref(), Some("orgs"));
    assert_eq!(targets[1].table, "organizations");
    assert_eq!(
        targets[1]
            .filter
            .rules
            .iter()
            .map(|rule| rule.column.as_str())
            .collect::<Vec<_>>(),
        ["tenant_id", "id"]
    );
}

#[test]
fn refusals_say_why_and_never_build_a_filter() {
    let keys = [foreign_key(&["tenant", "customer"], &["tenant_id", "id"], None)];
    assert!(reference_targets(&keys, None, "other", &row(&[])).is_err());
    assert!(reference_targets(&keys, None, "customer", &row(&[("customer", Value::Int(9))])).is_err());
    assert!(
        reference_targets(
            &keys,
            None,
            "customer",
            &row(&[("tenant", Value::Null), ("customer", Value::Int(9))])
        )
        .is_err()
    );
}

fn inbound_foreign_key(
    name: &str,
    source_schema: Option<&str>,
    source_table: &str,
    source_columns: &[&str],
    target_columns: &[&str],
) -> InboundForeignKeyInfo {
    InboundForeignKeyInfo {
        name: name.into(),
        source_schema: source_schema.map(str::to_owned),
        source_table: source_table.into(),
        source_columns: source_columns.iter().map(|s| s.to_string()).collect(),
        target_columns: target_columns.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn referencing_targets_filters_referencing_table_by_local_row_values() {
    let keys = [inbound_foreign_key(
        "orders_customer_id_fkey",
        Some("sales"),
        "orders",
        &["customer_id"],
        &["id"],
    )];
    let targets = referencing_targets(
        &keys,
        &row(&[("id", Value::Int(42)), ("name", Value::Text("Alice".into()))]),
    )
    .unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].foreign_key, "orders_customer_id_fkey");
    assert_eq!(targets[0].schema.as_deref(), Some("sales"));
    assert_eq!(targets[0].table, "orders");
    assert_eq!(targets[0].filter.rules.len(), 1);
    assert_eq!(targets[0].filter.rules[0].column, "customer_id");
    assert_eq!(targets[0].filter.rules[0].op, FilterOp::Eq);
}

#[test]
fn referencing_targets_handles_composite_keys() {
    let keys = [inbound_foreign_key(
        "child_composite_fkey",
        None,
        "child",
        &["parent_tenant", "parent_id"],
        &["tenant_id", "id"],
    )];
    let targets = referencing_targets(&keys, &row(&[("tenant_id", Value::Int(1)), ("id", Value::Int(99))])).unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].filter.rules.len(), 2);
    assert_eq!(targets[0].filter.rules[0].column, "parent_tenant");
    assert_eq!(targets[0].filter.rules[1].column, "parent_id");
}

#[test]
fn referencing_targets_offers_all_valid_referencing_tables() {
    let keys = [
        inbound_foreign_key("orders_fk", Some("sales"), "orders", &["cid"], &["id"]),
        inbound_foreign_key("invoices_fk", Some("billing"), "invoices", &["client_id"], &["id"]),
    ];
    let targets = referencing_targets(&keys, &row(&[("id", Value::Int(10))])).unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].table, "orders");
    assert_eq!(targets[1].table, "invoices");
}

#[test]
fn referencing_targets_refusals_on_empty_null_or_missing() {
    assert!(referencing_targets(&[], &row(&[("id", Value::Int(1))])).is_err());
    let keys = [inbound_foreign_key("orders_fk", None, "orders", &["cid"], &["id"])];
    assert!(referencing_targets(&keys, &row(&[("other", Value::Int(1))])).is_err());
    assert!(referencing_targets(&keys, &row(&[("id", Value::Null)])).is_err());
}
