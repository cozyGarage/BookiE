use super::{connect, start_mariadb, start_mysql};
use bookie_core::{ActivityQuery, Connection, Value, activity_sql};

async fn assert_variables_and_status(conn: &dyn Connection) {
    let sql = activity_sql("mysql", ActivityQuery::ServerVariables, None).unwrap();
    let variables = conn.query(&sql).await.unwrap();
    assert!(
        variables
            .rows
            .iter()
            .any(|row| row[0] == Value::Text("max_connections".into()))
    );

    let sql = activity_sql("mysql", ActivityQuery::ServerStatus, None).unwrap();
    let status = conn.query(&sql).await.unwrap();
    assert!(
        status
            .rows
            .iter()
            .any(|row| row[0] == Value::Text("Threads_connected".into()))
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_server_variables_and_status_statements_return_rows() {
    let (_container, opts) = start_mysql().await;
    assert_variables_and_status(connect(opts).await.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_server_variables_and_status_statements_return_rows() {
    let (_container, opts) = start_mariadb().await;
    assert_variables_and_status(connect(opts).await.as_ref()).await;
}
