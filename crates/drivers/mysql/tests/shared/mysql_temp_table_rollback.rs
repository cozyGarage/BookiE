use super::*;

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_temporary_table_creation_survives_rollback_but_its_rows_do_not() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    assert_temporary_table_ddl_survives_rollback(conn).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_temporary_table_creation_survives_rollback_but_its_rows_do_not() {
    let (_container, opts) = start_mariadb().await;
    let conn = connect(opts).await;
    assert_temporary_table_ddl_survives_rollback(conn).await;
}

async fn assert_temporary_table_ddl_survives_rollback(conn: Box<dyn Connection>) {
    let mut session = conn.open_session().await.unwrap();
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    for sql in [
        "START TRANSACTION",
        "CREATE TEMPORARY TABLE rollback_temp_rows (id INT PRIMARY KEY) ENGINE=InnoDB",
        "INSERT INTO rollback_temp_rows VALUES (1)",
        "ROLLBACK",
    ] {
        session.query_params_controlled(sql, &[], &control).await.unwrap();
    }

    let rows = session
        .query_params_controlled("SELECT COUNT(*) FROM rollback_temp_rows", &[], &control)
        .await
        .unwrap();
    assert_eq!(rows.rows, vec![vec![Value::Int(0)]]);
    session.close().await.unwrap();
}
