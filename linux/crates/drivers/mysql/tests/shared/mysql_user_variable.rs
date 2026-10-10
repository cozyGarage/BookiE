use super::*;

async fn assert_failed_batch_keeps_user_variable_side_effect(connection: Box<dyn Connection>) {
    connection
        .execute(
            "CREATE TABLE user_variable_rollback (
                id INT PRIMARY KEY,
                amount INT NOT NULL
            ) ENGINE=InnoDB",
        )
        .await
        .unwrap();
    connection
        .execute("INSERT INTO user_variable_rollback VALUES (1, 0)")
        .await
        .unwrap();

    let batch = vec![
        (
            "UPDATE user_variable_rollback \
             SET amount = (@bookie_rollback_var := CONNECTION_ID()) WHERE id = 1"
                .into(),
            vec![],
        ),
        ("INSERT INTO user_variable_rollback VALUES (1, 2)".into(), vec![]),
    ];

    let error = connection
        .execute_in_transaction(&batch)
        .await
        .expect_err("the duplicate insert must fail the DML batch");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "got {error:?}"
    );

    let rows = connection
        .query("SELECT amount FROM user_variable_rollback WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.rows, vec![vec![Value::Int(0)]], "InnoDB changes roll back");

    let session_state = connection
        .query("SELECT CONNECTION_ID(), @bookie_rollback_var")
        .await
        .unwrap();
    let [row] = session_state.rows.as_slice() else {
        panic!("expected one session-state row, got {:?}", session_state.rows);
    };
    assert_eq!(
        row[0], row[1],
        "the returned pooled session retains the connection ID assigned inside the batch"
    );
    assert!(matches!(row[0], Value::Int(_)), "got {:?}", row[0]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_failed_batch_keeps_user_variable_side_effect() {
    let (_container, opts) = start_mysql().await;
    assert_failed_batch_keeps_user_variable_side_effect(connect(opts).await).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_failed_batch_keeps_user_variable_side_effect() {
    let (_container, opts) = start_mariadb().await;
    assert_failed_batch_keeps_user_variable_side_effect(connect(opts).await).await;
}
