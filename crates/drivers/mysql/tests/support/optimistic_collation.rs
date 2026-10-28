#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{connect, start_mysql};
use tablepro_core::sql_dialect::build_optimistic_keyed_update;
use tablepro_core::{ColumnInfo, DriverError, Value};

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

#[tokio::test]
#[ignore = "requires docker"]
async fn a_stale_edit_is_refused_when_another_writer_changed_only_case_or_accents() {
    let (_container, options) = start_mysql().await;
    let connection = connect(options).await;
    connection
        .execute("CREATE TABLE people (id INT PRIMARY KEY, name VARCHAR(20))")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO people VALUES (1, 'ADA'), (2, 'Zoe')")
        .await
        .unwrap();
    let columns = [column("id", "int", true), column("name", "varchar(20)", false)];
    let edit = |id: i64, old: &str, new: &str| {
        let (sql, params) = build_optimistic_keyed_update(
            "mysql",
            None,
            "people",
            &columns,
            &[(1, Value::Text(old.into()), Value::Text(new.into()))],
            &[Value::Int(id)],
        )
        .unwrap();
        (sql, params)
    };

    let case_only = connection
        .execute_in_transaction_checked(&[edit(1, "Ada", "stale edit")], &[0])
        .await;
    let accent_only = connection
        .execute_in_transaction_checked(&[edit(2, "Zoé", "stale edit")], &[0])
        .await;
    let exact = connection
        .execute_in_transaction_checked(&[edit(2, "Zoe", "Zoë")], &[0])
        .await;

    assert!(
        matches!(case_only, Err(DriverError::Transaction { .. })),
        "{case_only:?}"
    );
    assert!(
        matches!(accent_only, Err(DriverError::Transaction { .. })),
        "{accent_only:?}"
    );
    assert_eq!(exact.unwrap(), vec![1]);
    let stored = connection.query("SELECT name FROM people ORDER BY id").await.unwrap();
    assert_eq!(
        stored.rows,
        vec![vec![Value::Text("ADA".into())], vec![Value::Text("Zoë".into())]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_single_precision_float_cell_can_be_edited_after_it_was_read() {
    let (_container, options) = start_mysql().await;
    let connection = connect(options).await;
    connection
        .execute("CREATE TABLE readings (id INT PRIMARY KEY, f FLOAT)")
        .await
        .unwrap();
    connection.execute("INSERT INTO readings VALUES (1, 0.1)").await.unwrap();
    let read = connection.query("SELECT f FROM readings").await.unwrap();
    let original = read.rows[0][0].clone();
    assert_eq!(original, Value::Float(0.1));
    let columns = [column("id", "int", true), column("f", "float", false)];
    let (sql, params) = build_optimistic_keyed_update(
        "mysql",
        None,
        "readings",
        &columns,
        &[(1, original, Value::Float(0.2))],
        &[Value::Int(1)],
    )
    .unwrap();

    let affected = connection
        .execute_in_transaction_checked(&[(sql, params)], &[0])
        .await
        .expect("the edit of an unchanged FLOAT cell is accepted");

    assert_eq!(affected, vec![1]);
    let stored = connection.query("SELECT f FROM readings").await.unwrap();
    assert_eq!(stored.rows, vec![vec![Value::Float(0.2)]]);
}
