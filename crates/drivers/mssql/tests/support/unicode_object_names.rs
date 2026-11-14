#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{connect, start_mssql};
use bookie_core::sql_ddl::{DraftColumn, build_alter_column, build_rename_column, build_rename_table};

#[tokio::test]
#[ignore = "requires docker"]
async fn non_latin_object_names_survive_rename_default_and_comment_on_a_latin1_database() {
    let (_c, opts) = start_mssql().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE [bảng] ([cột] int NOT NULL DEFAULT 1)")
        .await
        .unwrap();

    conn.execute(&build_rename_column("mssql", None, "bảng", "cột", "cột_mới").unwrap())
        .await
        .unwrap();
    conn.execute(&build_rename_table("mssql", None, "bảng", "bảng_mới").unwrap())
        .await
        .unwrap();
    let renamed = conn.fetch_columns(None, "bảng_mới").await.unwrap();
    assert_eq!(renamed.len(), 1);
    assert_eq!(renamed[0].name, "cột_mới");

    let mut column = DraftColumn::from_info(renamed[0].clone());
    column.default_value = Some("7".into());
    column.comment = Some("ghi chú".into());
    for statement in build_alter_column("mssql", None, "bảng_mới", &column).unwrap() {
        conn.execute(&statement).await.unwrap();
    }
    let after = conn.fetch_columns(None, "bảng_mới").await.unwrap();
    assert_eq!(after[0].comment.as_deref(), Some("ghi chú"));
    let defaults = conn
        .query(
            "SELECT dc.definition FROM sys.default_constraints dc \
             JOIN sys.columns c ON c.object_id = dc.parent_object_id AND c.column_id = dc.parent_column_id \
             WHERE c.name = N'cột_mới'",
        )
        .await
        .unwrap();
    assert_eq!(defaults.rows.len(), 1, "the old default is replaced, not duplicated");
}
