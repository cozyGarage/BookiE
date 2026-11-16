#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::sql_dialect::build_optimistic_keyed_update;
use bookie_core::{ColumnInfo, FilterOp, FilterRule, FilterSet, FilterValue, Value, build_filter_where};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_real_cell_can_be_edited_and_filtered_after_it_was_read() {
    let (_container, options) = crate::start_mssql().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TABLE [dbo].[readings] (id INT PRIMARY KEY, f REAL)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO [dbo].[readings] VALUES (1, 0.1), (2, 0.5)")
        .await
        .unwrap();
    let columns = [column("id", "int", true), column("f", "real", false)];
    let read = connection
        .query("SELECT f FROM [dbo].[readings] WHERE id = 1")
        .await
        .unwrap();
    let original = read.rows[0][0].clone();
    let (sql, params) = build_optimistic_keyed_update(
        "mssql",
        Some("dbo"),
        "readings",
        &columns,
        &[(1, original, Value::Float(0.2))],
        &[Value::Int(1)],
    )
    .unwrap();
    let affected = connection
        .execute_in_transaction_checked(&[(sql, params)], &[0])
        .await
        .unwrap();
    assert_eq!(affected, vec![1]);

    let filter = FilterSet {
        rules: vec![FilterRule {
            column: "f".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single("0.2".into())),
        }],
        ..Default::default()
    };
    let (clause, params) = build_filter_where("mssql", &columns, &filter).unwrap().unwrap();
    let rows = connection
        .query_params(
            &format!("SELECT id FROM [dbo].[readings] WHERE {clause} ORDER BY id"),
            &params,
        )
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
