use bookie_core::{ConnectOptions, DatabaseDriver};
use drivers_duckdb::DuckdbDriver;

#[tokio::test]
async fn a_view_is_listed_as_a_view_and_not_as_a_table() {
    let connection = DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE t (id INTEGER PRIMARY KEY, name VARCHAR)")
        .await
        .unwrap();
    connection.execute("CREATE VIEW v AS SELECT * FROM t").await.unwrap();

    let tables: Vec<_> = connection
        .list_tables()
        .await
        .unwrap()
        .into_iter()
        .map(|info| info.name)
        .collect();
    let views: Vec<_> = connection
        .list_views()
        .await
        .unwrap()
        .into_iter()
        .map(|info| info.name)
        .collect();
    let columns = connection.fetch_columns(Some("main"), "v").await.unwrap();

    assert_eq!(tables, ["t"]);
    assert_eq!(views, ["v"]);
    assert_eq!(
        columns.iter().map(|column| column.name.as_str()).collect::<Vec<_>>(),
        ["id", "name"]
    );
}
