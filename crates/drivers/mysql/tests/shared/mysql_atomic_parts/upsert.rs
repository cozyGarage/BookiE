use super::*;

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_failed_upsert_keeps_nontransactional_trigger_effect() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts.clone()).await;
    assert_upsert_trigger_effect_survives_later_batch_rollback(conn, opts).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_failed_upsert_keeps_nontransactional_trigger_effect() {
    let (_container, opts) = start_mariadb().await;
    let conn = connect(opts.clone()).await;
    assert_upsert_trigger_effect_survives_later_batch_rollback(conn, opts).await;
}

async fn assert_upsert_trigger_effect_survives_later_batch_rollback(conn: Box<dyn Connection>, opts: ConnectOptions) {
    conn.execute("CREATE TABLE upsert_parent (id INT PRIMARY KEY, value INT NOT NULL) ENGINE=InnoDB")
        .await
        .unwrap();
    conn.execute("INSERT INTO upsert_parent VALUES (1, 10)").await.unwrap();
    conn.execute("CREATE TABLE upsert_myisam_effect (id INT, value INT) ENGINE=MyISAM")
        .await
        .unwrap();
    let fixture = sqlx::mysql::MySqlPoolOptions::new()
        .connect_with(
            sqlx::mysql::MySqlConnectOptions::new()
                .host(&opts.host)
                .port(opts.port)
                .username(&opts.username)
                .password(opts.password.expose_secret())
                .database(&opts.database),
        )
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TRIGGER upsert_parent_after_update AFTER UPDATE ON upsert_parent \
         FOR EACH ROW INSERT INTO upsert_myisam_effect VALUES (NEW.id, NEW.value)",
    )
    .execute(&fixture)
    .await
    .unwrap();
    fixture.close().await;

    let batch = vec![
        (
            "INSERT INTO upsert_parent VALUES (1, 11) ON DUPLICATE KEY UPDATE value = 11".into(),
            vec![],
        ),
        ("INSERT INTO missing_upsert_table VALUES (1)".into(), vec![]),
    ];
    let error = conn
        .execute_in_transaction(&batch)
        .await
        .expect_err("the second insert must fail after the upsert trigger runs");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "the second statement must be reported as the failure, got {error:?}"
    );

    assert_eq!(
        conn.query("SELECT id, value FROM upsert_parent").await.unwrap().rows,
        vec![vec![Value::Int(1), Value::Int(10)]],
        "the InnoDB upsert update is rolled back"
    );
    assert_eq!(
        conn.query("SELECT id, value FROM upsert_myisam_effect")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1), Value::Int(11)]],
        "the MyISAM AFTER UPDATE trigger effect survives rollback"
    );
}
