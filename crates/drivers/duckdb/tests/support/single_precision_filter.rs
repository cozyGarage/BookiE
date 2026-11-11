#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{ColumnInfo, ConnectOptions, DatabaseDriver, FilterOp, FilterRule, FilterSet, FilterValue, Value};
use drivers_duckdb::DuckdbDriver;

#[tokio::test]
async fn filtering_a_float_column_by_its_own_value_finds_the_row() {
    let connection = DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE readings (id INTEGER PRIMARY KEY, f FLOAT)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO readings VALUES (1, 0.1), (2, 0.5)")
        .await
        .unwrap();
    let columns = [column("id", "INTEGER", true), column("f", "FLOAT", false)];
    let filter = FilterSet {
        rules: vec![FilterRule {
            column: "f".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single("0.1".into())),
        }],
        ..Default::default()
    };
    let (clause, params) = bookie_core::build_filter_where("duckdb", &columns, &filter)
        .unwrap()
        .unwrap();
    let rows = connection
        .query_params(&format!("SELECT id FROM readings WHERE {clause} ORDER BY id"), &params)
        .await
        .unwrap();

    assert_eq!(rows.rows, vec![vec![Value::Int(1)]]);
}

fn column(name: &str, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
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
