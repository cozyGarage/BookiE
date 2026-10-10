use super::*;

async fn assert_failed_batch_releases_named_lock(conn: &dyn Connection) {
    const LOCK_NAME: &str = "bookie_b4_failed_batch_release_lock";
    conn.execute("CREATE TABLE rollback_released_lock (id INT PRIMARY KEY, lock_result INT NOT NULL) ENGINE=InnoDB")
        .await
        .unwrap();
    conn.execute("INSERT INTO rollback_released_lock VALUES (1, 0)")
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE rollback_released_lock_witness (step INT PRIMARY KEY, result INT NOT NULL) ENGINE=MyISAM",
    )
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
            format!("INSERT INTO rollback_released_lock_witness VALUES (1, GET_LOCK('{LOCK_NAME}', 0))"),
            vec![],
        ),
        (
            format!("INSERT INTO rollback_released_lock_witness VALUES (2, RELEASE_LOCK('{LOCK_NAME}'))"),
            vec![],
        ),
        ("INSERT INTO rollback_released_lock VALUES (1, 0)".into(), vec![]),
    ];
    let error = conn
        .execute_in_transaction(&batch)
        .await
        .expect_err("duplicate key must fail after the lock is released");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 2, .. }),
        "the failed statement index must be preserved, got {error:?}"
    );
    assert_eq!(
        conn.query("SELECT id, lock_result FROM rollback_released_lock ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1), Value::Int(0)]],
        "the transactional insert must roll back"
    );
    assert_eq!(
        conn.query("SELECT step, result FROM rollback_released_lock_witness ORDER BY step")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1), Value::Int(1)], vec![Value::Int(2), Value::Int(1)]],
        "the MyISAM witness records successful acquire and release calls on the batch session"
    );
    assert_eq!(
        conn.query(&format!("SELECT IS_FREE_LOCK('{LOCK_NAME}')"))
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]],
        "the named lock release survives transaction rollback"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_failed_batch_preserves_named_lock_release_session_state() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    assert_failed_batch_releases_named_lock(conn.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_failed_batch_preserves_named_lock_release_session_state() {
    let (_container, opts) = start_mariadb().await;
    let conn = connect(opts).await;
    assert_failed_batch_releases_named_lock(conn.as_ref()).await;
}
