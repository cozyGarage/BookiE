use bookie_core::{ConnectOptions, DatabaseDriver};
use drivers_duckdb::DuckdbDriver;

#[tokio::test]
async fn result_columns_report_duckdb_type_names() {
    let connection = DuckdbDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE t (id INTEGER PRIMARY KEY, name VARCHAR, price DECIMAL(10, 2), seen_at TIMESTAMPTZ, tags INTEGER[], pair STRUCT(a INTEGER, b VARCHAR), scores MAP(VARCHAR, DOUBLE), token UUID)")
        .await
        .unwrap();

    let result = connection.query("SELECT * FROM t").await.unwrap();
    let described = connection.fetch_columns(Some("main"), "t").await.unwrap();

    let types: Vec<_> = result.columns.iter().map(|column| column.data_type.as_str()).collect();
    assert_eq!(
        types,
        [
            "INTEGER",
            "VARCHAR",
            "DECIMAL(10,2)",
            "TIMESTAMP WITH TIME ZONE",
            "INTEGER[]",
            "STRUCT(a INTEGER, b VARCHAR)",
            "MAP(VARCHAR, DOUBLE)",
            "UUID"
        ]
    );
    assert_eq!(types[0], described[0].data_type);
    assert_eq!(types[1], described[1].data_type);
}
