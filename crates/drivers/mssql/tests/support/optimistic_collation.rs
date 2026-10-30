#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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
    let (_container, options) = crate::start_mssql().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TABLE [dbo].[people] (id INT PRIMARY KEY, name NVARCHAR(20))")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO [dbo].[people] VALUES (1, N'ADA'), (2, N'Zoe')")
        .await
        .unwrap();
    let columns = [column("id", "int", true), column("name", "nvarchar(20)", false)];
    let edit = |id: i64, old: &str, new: &str| {
        build_optimistic_keyed_update(
            "mssql",
            Some("dbo"),
            "people",
            &columns,
            &[(1, Value::Text(old.into()), Value::Text(new.into()))],
            &[Value::Int(id)],
        )
        .unwrap()
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
    let stored = connection
        .query("SELECT name FROM [dbo].[people] ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        stored.rows,
        vec![vec![Value::Text("ADA".into())], vec![Value::Text("Zoë".into())]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_varchar_column_is_compared_exactly_too() {
    let (_container, options) = crate::start_mssql().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TABLE [dbo].[codes] (id INT PRIMARY KEY, code VARCHAR(20))")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO [dbo].[codes] VALUES (1, 'AbC')")
        .await
        .unwrap();
    let columns = [column("id", "int", true), column("code", "varchar(20)", false)];
    let edit = |old: &str| {
        build_optimistic_keyed_update(
            "mssql",
            Some("dbo"),
            "codes",
            &columns,
            &[(1, Value::Text(old.into()), Value::Text("new".into()))],
            &[Value::Int(1)],
        )
        .unwrap()
    };

    let stale = connection.execute_in_transaction_checked(&[edit("abc")], &[0]).await;
    let exact = connection.execute_in_transaction_checked(&[edit("AbC")], &[0]).await;

    assert!(matches!(stale, Err(DriverError::Transaction { .. })), "{stale:?}");
    assert_eq!(exact.unwrap(), vec![1]);
}
