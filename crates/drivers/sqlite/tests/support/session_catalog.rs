use super::*;

#[tokio::test]
async fn temporary_catalog_is_session_local_and_excludes_other_object_kinds() {
    let directory = TempDir::new().unwrap();
    let connection = connect_file(&directory).await;
    connection.execute("CREATE TABLE permanent (id INT)").await.unwrap();
    let mut session = connection.open_session().await.unwrap();
    let control = OperationControl::with_timeout(Duration::from_secs(30));
    for sql in [
        "CREATE TEMP TABLE zeta (id INTEGER PRIMARY KEY AUTOINCREMENT)",
        "CREATE TEMP TABLE alpha (id INT)",
        "CREATE TEMP VIEW temporary_view AS SELECT id FROM alpha",
        "CREATE INDEX temp.temporary_index ON alpha(id)",
    ] {
        session.query_params_controlled(sql, &[], &control).await.unwrap();
    }
    let tables = session.list_temporary_tables_controlled(&control).await.unwrap();
    assert_eq!(
        tables,
        ["alpha", "zeta"].map(|name| bookie_core::TableInfo {
            schema: Some("temp".into()),
            name: name.into(),
        })
    );
    let mut other = connection.open_session().await.unwrap();
    assert!(
        other
            .list_temporary_tables_controlled(&control)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !connection
            .list_tables()
            .await
            .unwrap()
            .iter()
            .any(|table| table.name == "alpha")
    );
    other.close().await.unwrap();
    session.close().await.unwrap();
    let mut reopened = connection.open_session().await.unwrap();
    assert!(
        reopened
            .list_temporary_tables_controlled(&control)
            .await
            .unwrap()
            .is_empty()
    );
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn dropping_a_temporary_table_removes_it_from_the_session_catalog() {
    let directory = TempDir::new().unwrap();
    let connection = connect_file(&directory).await;
    let mut session = connection.open_session().await.unwrap();
    let control = OperationControl::with_timeout(Duration::from_secs(30));
    session
        .query_params_controlled("CREATE TEMP TABLE scratch (id INT)", &[], &control)
        .await
        .unwrap();
    assert_eq!(
        session.list_temporary_tables_controlled(&control).await.unwrap().len(),
        1
    );
    session
        .query_params_controlled("DROP TABLE temp.scratch", &[], &control)
        .await
        .unwrap();
    assert!(
        session
            .list_temporary_tables_controlled(&control)
            .await
            .unwrap()
            .is_empty()
    );
    session.close().await.unwrap();
}

#[tokio::test]
async fn a_cancelled_temporary_catalog_request_returns_cancellation() {
    let directory = TempDir::new().unwrap();
    let connection = connect_file(&directory).await;
    let mut session = connection.open_session().await.unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let result = session
        .list_temporary_tables_controlled(&OperationControl::new(token, None))
        .await;
    assert!(matches!(result, Err(DriverError::Cancelled)));
    session.close().await.unwrap();
}
