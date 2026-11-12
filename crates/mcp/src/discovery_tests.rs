use bookie_core::{ColumnInfo, TableInfo};

use super::*;

fn table(name: &str) -> TableInfo {
    TableInfo {
        schema: Some("public".into()),
        name: name.into(),
    }
}

fn column(name: String, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name,
        data_type: data_type.into(),
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

fn columns(count: usize) -> Vec<ColumnInfo> {
    (0..count)
        .map(|index| column(format!("c{index}"), "integer", index == 0))
        .collect()
}

#[test]
fn a_small_snapshot_lists_columns_and_key_columns_untruncated() {
    let mut builder = ContextBuilder::new(1);
    assert!(builder.push(&table("orders"), &columns(3)));
    let context = builder.finish();
    assert_eq!(context["truncated"], false);
    assert_eq!(context["limit"], JsonValue::Null);
    assert_eq!(context["tables"][0]["primary_key"], json!(["c0"]));
    assert_eq!(context["tables"][0]["columns"].as_array().unwrap().len(), 3);
}

#[test]
fn the_table_cap_stops_the_snapshot_and_says_so() {
    let mut builder = ContextBuilder::new(SCHEMA_CONTEXT_MAX_TABLES + 5);
    let mut accepted = 0;
    for index in 0..SCHEMA_CONTEXT_MAX_TABLES + 5 {
        if builder.push(&table(&format!("t{index}")), &columns(1)) {
            accepted += 1;
        }
    }
    let context = builder.finish();
    assert_eq!(accepted, SCHEMA_CONTEXT_MAX_TABLES);
    assert_eq!(context["limit"], "tables");
    assert_eq!(context["truncated"], true);
    assert_eq!(context["table_count"], SCHEMA_CONTEXT_MAX_TABLES + 5);
}

#[test]
fn a_wide_table_is_cut_to_the_per_table_column_cap() {
    let mut builder = ContextBuilder::new(1);
    assert!(builder.push(&table("wide"), &columns(SCHEMA_CONTEXT_MAX_COLUMNS_PER_TABLE + 20)));
    let context = builder.finish();
    assert_eq!(
        context["tables"][0]["columns"].as_array().unwrap().len(),
        SCHEMA_CONTEXT_MAX_COLUMNS_PER_TABLE
    );
    assert_eq!(context["tables"][0]["columns_truncated"], true);
    assert_eq!(context["truncated"], true);
}

#[test]
fn the_total_column_cap_refuses_the_table_that_would_cross_it() {
    let mut builder = ContextBuilder::new(100);
    let mut accepted = 0;
    for index in 0..100 {
        if builder.push(
            &table(&format!("t{index}")),
            &columns(SCHEMA_CONTEXT_MAX_COLUMNS_PER_TABLE),
        ) {
            accepted += 1;
        }
    }
    assert!(accepted * SCHEMA_CONTEXT_MAX_COLUMNS_PER_TABLE <= SCHEMA_CONTEXT_MAX_COLUMNS);
    let limit = builder.finish()["limit"].clone();
    assert!(limit == "columns" || limit == "bytes", "{limit}");
}

#[test]
fn the_byte_cap_bounds_the_rendered_snapshot() {
    let long_name = "n".repeat(250);
    let wide: Vec<ColumnInfo> = (0..SCHEMA_CONTEXT_MAX_COLUMNS_PER_TABLE)
        .map(|index| column(format!("{long_name}{index}"), "text", false))
        .collect();
    let mut builder = ContextBuilder::new(50);
    for index in 0..50 {
        builder.push(&table(&format!("t{index}")), &wide);
    }
    let context = builder.finish();
    assert_eq!(context["limit"], "bytes");
    let tables = context["tables"].as_array().unwrap();
    assert!(!tables.is_empty());
    assert!(tables.iter().map(|table| table.to_string().len()).sum::<usize>() <= SCHEMA_CONTEXT_MAX_BYTES);
}

#[test]
fn routine_arguments_are_bounded_and_free_of_control_characters() {
    assert_eq!(optional_arguments(&json!({})).unwrap(), None);
    assert_eq!(
        optional_arguments(&json!({"arguments": "a integer"}))
            .unwrap()
            .as_deref(),
        Some("a integer")
    );
    assert!(optional_arguments(&json!({"arguments": "a\u{0}b"})).is_err());
    assert!(optional_arguments(&json!({"arguments": "x".repeat(MAX_ARGUMENTS_BYTES + 1)})).is_err());
    assert!(optional_arguments(&json!({"arguments": 5})).is_err());
}
