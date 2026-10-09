use super::*;

async fn assert_failed_batch_preserves_named_lock(conn: &dyn Connection) {
    const LOCK_NAME: &str = "bookie_b4_failed_batch_lock";
    conn.execute("CREATE TABLE rollback_named_lock (id INT PRIMARY KEY, lock_result INT NOT NULL) ENGINE=InnoDB")
        .await
        .unwrap();
    conn.execute("INSERT INTO rollback_named_lock VALUES (1, 0)")
        .await
        .unwrap();
    assert_eq!(
        conn.query(&format!("SELECT IS_FREE_LOCK('{LOCK_NAME}')"))
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]],
        "the advisory lock must start free"
    );

    let batch = vec![
        (
            format!("INSERT INTO rollback_named_lock VALUES (2, GET_LOCK('{LOCK_NAME}', 0))"),
            vec![],
        ),
        ("INSERT INTO rollback_named_lock VALUES (1, 0)".into(), vec![]),
    ];
    let error = conn
        .execute_in_transaction(&batch)
        .await
        .expect_err("duplicate key must fail after the lock is acquired");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "the failed statement index must be preserved, got {error:?}"
    );
    assert_eq!(
        conn.query("SELECT id, lock_result FROM rollback_named_lock ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1), Value::Int(0)]],
        "the transactional row must roll back"
    );
    assert_eq!(
        conn.query(&format!("SELECT IS_FREE_LOCK('{LOCK_NAME}')"))
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(0)]],
        "the named lock survives transaction rollback"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_failed_batch_preserves_named_lock_session_state() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts.clone()).await;
    assert_failed_batch_preserves_named_lock(conn.as_ref()).await;
    conn.close().await.unwrap();
    let observer = connect(opts).await;
    assert_eq!(
        observer
            .query("SELECT IS_FREE_LOCK('bookie_b4_failed_batch_lock')")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]],
        "closing the owning pool must release the named lock"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_failed_batch_preserves_named_lock_session_state() {
    let (_container, opts) = start_mariadb().await;
    let conn = connect(opts.clone()).await;
    assert_failed_batch_preserves_named_lock(conn.as_ref()).await;
    conn.close().await.unwrap();
    let observer = connect(opts).await;
    assert_eq!(
        observer
            .query("SELECT IS_FREE_LOCK('bookie_b4_failed_batch_lock')")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]],
        "closing the owning pool must release the named lock"
    );
}
