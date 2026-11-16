use super::{connect, start_pg};
use bookie_core::{ActivityQuery, Value, activity_sql};

#[tokio::test]
#[ignore = "requires docker"]
async fn server_variables_and_status_statements_return_typed_rows() {
    let (_container, opts) = start_pg().await;
    let conn = connect(opts).await;

    let sql = activity_sql("postgres", ActivityQuery::ServerVariables, None).unwrap();
    let variables = conn.query(&sql).await.unwrap();
    let name = variables
        .columns
        .iter()
        .position(|column| column.name == "name")
        .unwrap();
    assert!(variables.rows.len() >= 100);
    assert!(
        variables
            .rows
            .iter()
            .any(|row| row[name] == Value::Text("max_connections".into()))
    );

    let sql = activity_sql("postgres", ActivityQuery::ServerStatus, None).unwrap();
    let status = conn.query(&sql).await.unwrap();
    let database = status
        .columns
        .iter()
        .position(|column| column.name == "datname")
        .unwrap();
    assert!(status.rows.iter().any(|row| matches!(&row[database], Value::Text(_))));
}
