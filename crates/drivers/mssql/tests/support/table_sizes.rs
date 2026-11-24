#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{connect, start_mssql};

#[tokio::test]
#[ignore = "requires docker"]
async fn table_sizes_sum_used_pages_per_table_and_filter_by_schema() {
    let (_c, opts) = start_mssql().await;
    let conn = connect(opts).await;
    conn.execute("EXEC('CREATE SCHEMA sizes')").await.unwrap();
    conn.execute("CREATE TABLE sizes.filled (id int PRIMARY KEY, body nvarchar(400))")
        .await
        .unwrap();
    conn.execute(
        "INSERT INTO sizes.filled (id, body) SELECT TOP (3000) ROW_NUMBER() OVER (ORDER BY (SELECT 1)), REPLICATE(N'x', 400) FROM sys.all_objects a CROSS JOIN sys.all_objects b",
    )
    .await
    .unwrap();
    conn.execute("CREATE TABLE dbo.other (id int)").await.unwrap();

    let scoped = conn.table_sizes(Some("sizes")).await.unwrap();
    assert_eq!(scoped.len(), 1, "{scoped:?}");
    assert_eq!(scoped[0].schema.as_deref(), Some("sizes"));
    assert_eq!(scoped[0].name, "filled");
    assert!(scoped[0].bytes > 1_000_000, "3000 rows of 800 bytes: {:?}", scoped[0]);

    let everything = conn.table_sizes(None).await.unwrap();
    assert!(
        everything
            .iter()
            .any(|size| size.name == "other" && size.schema.as_deref() == Some("dbo"))
    );
}
