use super::*;
use secrecy::ExposeSecret;

#[tokio::test]
#[ignore = "requires docker"]
async fn empty_query_results_preserve_mysql_projection_metadata() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE metadata_items (id INT, label VARCHAR(10))")
        .await
        .unwrap();
    let projection = "SELECT 1 AS duplicate, 'text' AS duplicate";

    let populated = conn.query(&format!("{projection} WHERE true")).await.unwrap();
    assert_eq!(
        populated
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["duplicate", "duplicate"]
    );
    assert_eq!(populated.rows, vec![vec![Value::Int(1), Value::Text("text".into())]]);

    let shared = conn.query(&format!("{projection} WHERE false")).await.unwrap();
    assert_eq!(
        shared
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["duplicate", "duplicate"]
    );
    assert_eq!(shared.columns, populated.columns);
    assert!(shared.rows.is_empty());

    let params = conn
        .query_params(
            "SELECT 1 AS duplicate, 'text' AS duplicate WHERE ?",
            &[Value::Bool(false)],
        )
        .await
        .unwrap();
    assert_eq!(
        params
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["duplicate", "duplicate"]
    );
    assert_eq!(params.columns, populated.columns);
    assert!(params.rows.is_empty());

    let mut transaction = conn.begin().await.unwrap();
    let transactional = transaction.query(&format!("{projection} WHERE false")).await.unwrap();
    transaction.rollback().await.unwrap();
    assert_eq!(
        transactional
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["duplicate", "duplicate"]
    );
    assert_eq!(transactional.columns, populated.columns);
    assert!(transactional.rows.is_empty());

    let paged = conn.fetch_rows(None, "metadata_items", 0, 50).await.unwrap();
    assert_eq!(
        paged
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "label"]
    );
    assert!(paged.rows.is_empty());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_transaction_queries_are_bounded_and_mark_truncation() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    let mut transaction = conn.begin().await.unwrap();
    let result = transaction
        .query(
            "SELECT 7 AS value FROM information_schema.columns a CROSS JOIN information_schema.columns b LIMIT 1000001",
        )
        .await
        .unwrap();
    transaction.rollback().await.unwrap();

    assert!(!result.rows.is_empty());
    assert!(result.rows.len() < tablepro_core::MAX_QUERY_ROWS);
    assert!(result.truncated);
    assert_eq!(result.columns[0].name, "value");
    assert!(result.rows.iter().all(|row| row == &[Value::Int(7)]));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_transaction_queries_obey_result_byte_budget_and_remain_usable() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    let mut transaction = conn.begin().await.unwrap();
    let payload_len = 4 * 1024 * 1024;
    let payload = "x".repeat(payload_len);
    let result = transaction
        .query(&format!(
            "SELECT REPEAT('x', {payload_len}) AS payload \
             FROM information_schema.columns a CROSS JOIN information_schema.columns b LIMIT 20"
        ))
        .await
        .unwrap();

    assert!(result.truncated, "omitted rows must be reported");
    assert!(!result.rows.is_empty());
    assert!(result.rows.len() < 20);
    assert!(result.rows.len() * payload_len <= tablepro_core::MAX_QUERY_RESULT_BYTES);
    assert!(result.rows.iter().all(|row| {
        matches!(row.first(), Some(Value::Text(value)) if value.len() == payload_len && value == &payload)
    }));

    let next = transaction.query("SELECT 42 AS usable").await.unwrap();
    assert_eq!(next.rows, vec![vec![Value::Int(42)]]);
    transaction.rollback().await.unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn implicit_commit_batch_is_refused_before_mysql_dispatch() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE atomic_items (id INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.execute("INSERT INTO atomic_items VALUES (1)").await.unwrap();
    let statements = vec![
        ("INSERT INTO atomic_items VALUES (2)".into(), vec![]),
        ("CREATE TABLE atomic_implicit_ddl (id INT)".into(), vec![]),
        ("INSERT INTO missing_atomic_items VALUES (3)".into(), vec![]),
    ];

    let error = conn
        .execute_in_transaction(&statements)
        .await
        .expect_err("implicit commit must be refused");
    assert!(matches!(error, DriverError::Unsupported(_)), "got {error:?}");
    let rows = conn.query("SELECT id FROM atomic_items ORDER BY id").await.unwrap();
    assert_eq!(rows.rows, vec![vec![Value::Int(1)]]);
    assert!(
        !conn
            .list_tables()
            .await
            .unwrap()
            .iter()
            .any(|table| table.name == "atomic_implicit_ddl")
    );

    let dml = vec![
        ("INSERT INTO atomic_items VALUES (2)".into(), vec![]),
        ("UPDATE atomic_items SET id = 3 WHERE id = 2".into(), vec![]),
        ("DELETE FROM atomic_items WHERE id = 3".into(), vec![]),
    ];
    assert_eq!(conn.execute_in_transaction(&dml).await.unwrap(), [1, 1, 1]);
    assert_eq!(
        conn.query("SELECT id FROM atomic_items ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_failed_mysql_dml_batch_confirms_rollback_and_preserves_neighbor_rows() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE rollback_items (id INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.execute("INSERT INTO rollback_items VALUES (1)").await.unwrap();
    let statements = vec![
        ("INSERT INTO rollback_items VALUES (2)".into(), vec![]),
        ("INSERT INTO rollback_items VALUES (1)".into(), vec![]),
    ];

    let error = conn
        .execute_in_transaction(&statements)
        .await
        .expect_err("duplicate insert must fail");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "got {error:?}"
    );
    assert_eq!(
        conn.query("SELECT id FROM rollback_items ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_batch_rollback_does_not_claim_to_reverse_nontransactional_engine_effects() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts.clone()).await;
    conn.execute("CREATE TABLE atomic_parent (id INT PRIMARY KEY) ENGINE=InnoDB")
        .await
        .unwrap();
    conn.execute("CREATE TABLE atomic_innodb_effects (id INT PRIMARY KEY) ENGINE=InnoDB")
        .await
        .unwrap();
    conn.execute("CREATE TABLE atomic_effects (id INT PRIMARY KEY) ENGINE=MyISAM")
        .await
        .unwrap();
    conn.execute("CREATE TABLE atomic_memory_effects (id INT PRIMARY KEY) ENGINE=MEMORY")
        .await
        .unwrap();
    conn.execute("CREATE TABLE atomic_csv_effects (id INT NOT NULL) ENGINE=CSV")
        .await
        .unwrap();
    conn.execute("CREATE TABLE atomic_archive_effects (id INT NOT NULL) ENGINE=ARCHIVE")
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE atomic_auto_increment (id INT AUTO_INCREMENT PRIMARY KEY, value INT UNIQUE) ENGINE=InnoDB",
    )
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
        "CREATE TRIGGER atomic_parent_after_insert AFTER INSERT ON atomic_parent \
         FOR EACH ROW INSERT INTO atomic_effects VALUES (NEW.id)",
    )
    .execute(&fixture)
    .await
    .unwrap();
    sqlx::raw_sql(
        "CREATE TRIGGER atomic_parent_after_insert_innodb AFTER INSERT ON atomic_parent \
         FOR EACH ROW INSERT INTO atomic_innodb_effects VALUES (NEW.id)",
    )
    .execute(&fixture)
    .await
    .unwrap();
    sqlx::raw_sql(
        "CREATE TRIGGER atomic_parent_after_insert_csv AFTER INSERT ON atomic_parent \
         FOR EACH ROW INSERT INTO atomic_csv_effects VALUES (NEW.id)",
    )
    .execute(&fixture)
    .await
    .unwrap();
    sqlx::raw_sql(
        "CREATE TRIGGER atomic_parent_after_insert_archive AFTER INSERT ON atomic_parent \
         FOR EACH ROW INSERT INTO atomic_archive_effects VALUES (NEW.id)",
    )
    .execute(&fixture)
    .await
    .unwrap();
    sqlx::raw_sql(
        "CREATE TRIGGER atomic_parent_after_insert_memory AFTER INSERT ON atomic_parent \
         FOR EACH ROW INSERT INTO atomic_memory_effects VALUES (NEW.id)",
    )
    .execute(&fixture)
    .await
    .unwrap();
    fixture.close().await;
    conn.execute("INSERT INTO atomic_parent VALUES (1)").await.unwrap();

    let batch = vec![
        ("INSERT INTO atomic_parent VALUES (2)".into(), vec![]),
        ("INSERT INTO atomic_parent VALUES (1)".into(), vec![]),
    ];
    let error = conn
        .execute_in_transaction(&batch)
        .await
        .expect_err("the duplicate key must fail the DML batch");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "got {error:?}"
    );

    assert_eq!(
        conn.query("SELECT id FROM atomic_parent ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]],
        "InnoDB rows are rolled back"
    );
    assert_eq!(
        conn.query("SELECT id FROM atomic_innodb_effects ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]],
        "the transactional InnoDB trigger effect is rolled back with its parent"
    );
    assert_eq!(
        conn.query("SELECT id FROM atomic_effects ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        "the trigger's MyISAM side effect survives rollback"
    );
    assert_eq!(
        conn.query("SELECT id FROM atomic_memory_effects ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        "the trigger's MEMORY side effect survives rollback"
    );
    assert_eq!(
        conn.query("SELECT id FROM atomic_csv_effects ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        "the trigger's CSV side effect survives rollback"
    );
    assert_eq!(
        conn.query("SELECT id FROM atomic_archive_effects ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        "the trigger's ARCHIVE side effect survives rollback"
    );

    conn.execute("INSERT INTO atomic_auto_increment (value) VALUES (1)")
        .await
        .unwrap();
    let auto_increment_batch = vec![
        ("INSERT INTO atomic_auto_increment (value) VALUES (2)".into(), vec![]),
        (
            "INSERT INTO atomic_auto_increment (id, value) VALUES (1, 1)".into(),
            vec![],
        ),
    ];
    let error = conn
        .execute_in_transaction(&auto_increment_batch)
        .await
        .expect_err("the duplicate primary key must roll back the auto-increment batch");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "got {error:?}"
    );
    assert_eq!(
        conn.query("SELECT id, value FROM atomic_auto_increment ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1), Value::Int(1)]],
        "the InnoDB row is rolled back"
    );
    conn.execute("INSERT INTO atomic_auto_increment (value) VALUES (3)")
        .await
        .unwrap();
    assert_eq!(
        conn.query("SELECT id, value FROM atomic_auto_increment ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1), Value::Int(1)], vec![Value::Int(3), Value::Int(3)]],
        "the InnoDB row rolls back, but its auto-increment allocation is not restored"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_batch_rollback_preserves_nontransactional_update_and_delete_trigger_effects() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts.clone()).await;
    conn.execute("CREATE TABLE dml_parent (id INT PRIMARY KEY) ENGINE=InnoDB")
        .await
        .unwrap();
    conn.execute("INSERT INTO dml_parent VALUES (1), (2)").await.unwrap();
    conn.execute("CREATE TABLE update_effects (id INT PRIMARY KEY) ENGINE=MyISAM")
        .await
        .unwrap();
    conn.execute("CREATE TABLE update_transactional_effects (id INT PRIMARY KEY) ENGINE=InnoDB")
        .await
        .unwrap();
    conn.execute("CREATE TABLE delete_effects (id INT PRIMARY KEY) ENGINE=MyISAM")
        .await
        .unwrap();
    conn.execute("CREATE TABLE delete_transactional_effects (id INT PRIMARY KEY) ENGINE=InnoDB")
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
        "CREATE TRIGGER dml_parent_after_update AFTER UPDATE ON dml_parent \
         FOR EACH ROW BEGIN \
             INSERT INTO update_effects VALUES (NEW.id); \
             INSERT INTO update_transactional_effects VALUES (NEW.id); \
         END",
    )
    .execute(&fixture)
    .await
    .unwrap();
    sqlx::raw_sql(
        "CREATE TRIGGER dml_parent_after_delete AFTER DELETE ON dml_parent \
         FOR EACH ROW BEGIN \
             INSERT INTO delete_effects VALUES (OLD.id); \
             INSERT INTO delete_transactional_effects VALUES (OLD.id); \
         END",
    )
    .execute(&fixture)
    .await
    .unwrap();
    fixture.close().await;

    let update_batch = vec![
        ("UPDATE dml_parent SET id = 3 WHERE id = 1".into(), vec![]),
        ("INSERT INTO dml_parent VALUES (2)".into(), vec![]),
    ];
    let error = conn
        .execute_in_transaction(&update_batch)
        .await
        .expect_err("duplicate key must fail the UPDATE batch");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "got {error:?}"
    );
    assert_eq!(
        conn.query("SELECT id FROM dml_parent ORDER BY id").await.unwrap().rows,
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        "the InnoDB UPDATE is rolled back"
    );
    assert_eq!(
        conn.query("SELECT id FROM update_effects ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(3)]],
        "the MyISAM AFTER UPDATE trigger effect survives rollback"
    );
    assert_eq!(
        conn.query("SELECT id FROM update_transactional_effects ORDER BY id")
            .await
            .unwrap()
            .rows,
        Vec::<Vec<Value>>::new(),
        "the InnoDB AFTER UPDATE trigger effect is rolled back"
    );

    let delete_batch = vec![
        ("DELETE FROM dml_parent WHERE id = 1".into(), vec![]),
        ("INSERT INTO dml_parent VALUES (2)".into(), vec![]),
    ];
    let error = conn
        .execute_in_transaction(&delete_batch)
        .await
        .expect_err("duplicate key must fail the DELETE batch");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "got {error:?}"
    );
    assert_eq!(
        conn.query("SELECT id FROM dml_parent ORDER BY id").await.unwrap().rows,
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        "the InnoDB DELETE is rolled back"
    );
    assert_eq!(
        conn.query("SELECT id FROM delete_effects ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]],
        "the MyISAM AFTER DELETE trigger effect survives rollback"
    );
    assert_eq!(
        conn.query("SELECT id FROM delete_transactional_effects ORDER BY id")
            .await
            .unwrap()
            .rows,
        Vec::<Vec<Value>>::new(),
        "the InnoDB AFTER DELETE trigger effect is rolled back"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_lost_connection_during_a_batch_reports_rollback_failure() {
    let (_container, opts) = start_mysql().await;
    let connection: std::sync::Arc<dyn Connection> = MysqlDriver.connect(opts.clone()).await.unwrap().into();
    let observer = connect(opts).await;
    connection
        .execute("CREATE TABLE rollback_loss (id INT PRIMARY KEY)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO rollback_loss VALUES (1)")
        .await
        .unwrap();
    let tag = "tablepro_mysql_batch_rollback_loss";
    let running = connection.clone();
    let task = tokio::spawn(async move {
        running
            .execute_in_transaction(&[(
                format!("UPDATE rollback_loss SET id = SLEEP(30) WHERE id = 1 /* {tag} */"),
                vec![],
            )])
            .await
    });

    let mut connection_id = None;
    for _ in 0..100 {
        let result = observer
            .query(&format!(
                "SELECT ID FROM information_schema.PROCESSLIST WHERE INFO LIKE '%{tag}%' AND ID <> CONNECTION_ID()"
            ))
            .await
            .unwrap();
        connection_id = result.rows.first().and_then(|row| match row.first() {
            Some(Value::Int(id)) => Some(*id),
            _ => None,
        });
        if connection_id.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let connection_id = connection_id.expect("tagged batch statement must reach MySQL");
    observer
        .execute(&format!("KILL CONNECTION {connection_id}"))
        .await
        .unwrap();
    let error = tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .expect("killed batch must settle")
        .expect("batch task")
        .expect_err("killed batch must return an error");

    assert!(
        matches!(error, DriverError::TransactionRollbackFailed { statement_index: 0, .. }),
        "rollback failure must be reported separately, got {error:?}"
    );
}
