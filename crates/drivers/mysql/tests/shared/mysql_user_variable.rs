use super::*;

async fn assert_failed_batch_drops_user_variable_side_effect(connection: &dyn Connection, controlled: bool) {
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

    let result = if controlled {
        connection
            .execute_in_transaction_controlled(
                &batch,
                &OperationControl::with_timeout(std::time::Duration::from_secs(30)),
            )
            .await
    } else {
        connection.execute_in_transaction(&batch).await
    };
    let error = result.expect_err("the duplicate insert must fail the DML batch");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "got {error:?}"
    );

    let rows = connection
        .query("SELECT amount FROM user_variable_rollback WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.rows, vec![vec![Value::Int(0)]], "InnoDB changes roll back");

    // The pool holds up to four connections and returns the batch's connection in the background, so a
    // single query may land on a fresh one. Hold all four at once and look for the one that kept the variable.
    let probe = || connection.query("SELECT CONNECTION_ID(), @bookie_rollback_var, SLEEP(0.3)");
    let (first, second, third, fourth) = tokio::join!(probe(), probe(), probe(), probe());
    let mut kept = 0;
    for result in [first, second, third, fourth] {
        let result = result.unwrap();
        assert_eq!(
            result.rows.len(),
            1,
            "expected one session-state row, got {:?}",
            result.rows
        );
        let row = &result.rows[0];
        assert!(matches!(row[0], Value::Int(_)), "got {:?}", row[0]);
        if row[1] != Value::Null {
            assert_eq!(row[0], row[1], "a session holds only its own connection ID");
            kept += 1;
        }
    }
    assert_eq!(
        kept, 0,
        "the connection that ran the failed batch is retired, so no pooled session keeps its variable"
    );
    connection.execute("DROP TABLE user_variable_rollback").await.unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_failed_batch_drops_user_variable_side_effect() {
    let (_container, opts) = start_mysql().await;
    let connection = connect(opts).await;
    for controlled in [false, true] {
        assert_failed_batch_drops_user_variable_side_effect(connection.as_ref(), controlled).await;
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_failed_batch_drops_user_variable_side_effect() {
    let (_container, opts) = start_mariadb().await;
    let connection = connect(opts).await;
    for controlled in [false, true] {
        assert_failed_batch_drops_user_variable_side_effect(connection.as_ref(), controlled).await;
    }
}

async fn assert_pooled_query_does_not_leak_user_variable(connection: &dyn Connection) {
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    let sql = "SELECT @bookie_pooled_marker := CONNECTION_ID()";
    for method in 0..5 {
        match method {
            0 => {
                connection.query(sql).await.unwrap();
            }
            1 => {
                connection.query_controlled(sql, &control).await.unwrap();
            }
            2 => {
                connection
                    .query_result_sets_controlled(sql, &[], &control)
                    .await
                    .unwrap();
            }
            3 => {
                connection.execute(sql).await.unwrap();
            }
            _ => {
                connection.execute_controlled(sql, &control).await.unwrap();
            }
        }
        let probe = || connection.query("SELECT @bookie_pooled_marker, SLEEP(0.3)");
        let (first, second, third, fourth) = tokio::join!(probe(), probe(), probe(), probe());
        for result in [first, second, third, fourth] {
            assert_eq!(
                result.unwrap().rows[0][0],
                Value::Null,
                "method {method} leaked session state"
            );
        }
    }
    connection
        .execute("SET SESSION sql_mode = '', time_zone = '+05:00'")
        .await
        .unwrap();
    let probe_defaults =
        || connection.query("SELECT '雪' AS text_value, 'a' || 'b' AS joined, @@session.time_zone, SLEEP(0.3)");
    let (first, second, third, fourth) =
        tokio::join!(probe_defaults(), probe_defaults(), probe_defaults(), probe_defaults());
    for result in [first, second, third, fourth] {
        assert_eq!(
            result.unwrap().rows[0][..3],
            [
                Value::Text("雪".into()),
                Value::Text("ab".into()),
                Value::Text("+00:00".into())
            ]
        );
    }
    let acquired = connection
        .query("SELECT GET_LOCK('bookie_pool_reset_test', 0)")
        .await
        .unwrap();
    assert_eq!(acquired.rows[0][0], Value::Int(1));
    let free = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let result = connection
                .query("SELECT IS_FREE_LOCK('bookie_pool_reset_test')")
                .await
                .unwrap();
            if result.rows[0][0] == Value::Int(1) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(free.is_ok(), "a named lock outlived its pooled query");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_pooled_query_does_not_leak_user_variable() {
    let (_container, opts) = start_mysql().await;
    assert_pooled_query_does_not_leak_user_variable(connect(opts).await.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_pooled_query_does_not_leak_user_variable() {
    let (_container, opts) = start_mariadb().await;
    assert_pooled_query_does_not_leak_user_variable(connect(opts).await.as_ref()).await;
}
