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
async fn mysql_failed_batch_keeps_direct_inserts_on_nontransactional_engines() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    let engines = [
        ("innodb", "InnoDB", true),
        ("myisam", "MyISAM", false),
        ("memory", "MEMORY", false),
        ("csv", "CSV", false),
        ("archive", "ARCHIVE", false),
    ];

    for (name, engine, transactional) in engines {
        let table = format!("direct_{name}_effects");
        conn.execute(&format!("CREATE TABLE {table} (id INT NOT NULL) ENGINE={engine}"))
            .await
            .unwrap();
        conn.execute(&format!("INSERT INTO {table} VALUES (1)")).await.unwrap();

        let missing_table = format!("missing_direct_{name}_target");
        let batch = vec![
            (format!("INSERT INTO {table} VALUES (2)"), vec![]),
            (format!("INSERT INTO {missing_table} VALUES (99)"), vec![]),
        ];
        let error = conn
            .execute_in_transaction(&batch)
            .await
            .expect_err("the absent table must fail after the direct engine write");
        assert!(
            matches!(error, DriverError::Transaction { statement_index: 1, .. }),
            "{engine} failure must identify statement 1, got {error:?}"
        );

        let expected = if transactional {
            vec![vec![Value::Int(1)]]
        } else {
            vec![vec![Value::Int(1)], vec![Value::Int(2)]]
        };
        assert_eq!(
            conn.query(&format!("SELECT id FROM {table} ORDER BY id"))
                .await
                .unwrap()
                .rows,
            expected,
            "{engine} direct INSERT effects must match its rollback contract"
        );
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_failed_batch_keeps_direct_updates_and_deletes_on_nontransactional_engines() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    let engines = [
        ("innodb", "InnoDB", true),
        ("myisam", "MyISAM", false),
        ("memory", "MEMORY", false),
        ("csv", "CSV", false),
    ];

    for (name, engine, transactional) in engines {
        for (operation, statement, persisted_rows) in [
            (
                "update",
                "UPDATE {table} SET value = 11 WHERE id = 1",
                vec![vec![Value::Int(1), Value::Int(11)], vec![Value::Int(2), Value::Int(20)]],
            ),
            (
                "delete",
                "DELETE FROM {table} WHERE id = 2",
                vec![vec![Value::Int(1), Value::Int(10)]],
            ),
        ] {
            let table = format!("direct_{name}_{operation}_effects");
            conn.execute(&format!(
                "CREATE TABLE {table} (id INT NOT NULL, value INT NOT NULL) ENGINE={engine}"
            ))
            .await
            .unwrap();
            conn.execute(&format!("INSERT INTO {table} VALUES (1, 10), (2, 20)"))
                .await
                .unwrap();

            let missing_table = format!("missing_direct_{name}_{operation}_target");
            let batch = vec![
                (statement.replace("{table}", &table), vec![]),
                (format!("INSERT INTO {missing_table} VALUES (99, 99)"), vec![]),
            ];
            let error = conn
                .execute_in_transaction(&batch)
                .await
                .expect_err("the absent table must fail after the direct engine write");
            assert!(
                matches!(error, DriverError::Transaction { statement_index: 1, .. }),
                "{engine} {operation} failure must identify statement 1, got {error:?}"
            );

            let expected_rows = if transactional {
                vec![vec![Value::Int(1), Value::Int(10)], vec![Value::Int(2), Value::Int(20)]]
            } else {
                persisted_rows
            };
            assert_eq!(
                conn.query(&format!("SELECT id, value FROM {table} ORDER BY id"))
                    .await
                    .unwrap()
                    .rows,
                expected_rows,
                "{engine} direct {operation} effects must match its rollback contract"
            );
        }
    }
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
    let update_effect_tables = [
        ("update_myisam_effects", "MyISAM", "id INT PRIMARY KEY"),
        ("update_memory_effects", "MEMORY", "id INT PRIMARY KEY"),
        ("update_csv_effects", "CSV", "id INT NOT NULL"),
        ("update_archive_effects", "ARCHIVE", "id INT NOT NULL"),
    ];
    for (table, engine, columns) in update_effect_tables {
        conn.execute(&format!("CREATE TABLE {table} ({columns}) ENGINE={engine}"))
            .await
            .unwrap();
    }
    conn.execute("CREATE TABLE update_transactional_effects (id INT PRIMARY KEY) ENGINE=InnoDB")
        .await
        .unwrap();
    let delete_effect_tables = [
        ("delete_myisam_effects", "MyISAM", "id INT PRIMARY KEY"),
        ("delete_memory_effects", "MEMORY", "id INT PRIMARY KEY"),
        ("delete_csv_effects", "CSV", "id INT NOT NULL"),
        ("delete_archive_effects", "ARCHIVE", "id INT NOT NULL"),
    ];
    for (table, engine, columns) in delete_effect_tables {
        conn.execute(&format!("CREATE TABLE {table} ({columns}) ENGINE={engine}"))
            .await
            .unwrap();
    }
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
    let update_effect_inserts = update_effect_tables
        .iter()
        .map(|(table, _, _)| format!("INSERT INTO {table} VALUES (NEW.id); "))
        .collect::<String>();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER dml_parent_after_update AFTER UPDATE ON dml_parent \
         FOR EACH ROW BEGIN \
             INSERT INTO update_transactional_effects VALUES (NEW.id); \
             {update_effect_inserts} \
         END"
    )))
    .execute(&fixture)
    .await
    .unwrap();
    let delete_effect_inserts = delete_effect_tables
        .iter()
        .map(|(table, _, _)| format!("INSERT INTO {table} VALUES (OLD.id); "))
        .collect::<String>();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER dml_parent_after_delete AFTER DELETE ON dml_parent \
         FOR EACH ROW BEGIN \
             INSERT INTO delete_transactional_effects VALUES (OLD.id); \
             {delete_effect_inserts} \
         END"
    )))
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
    for (table, engine, _) in update_effect_tables {
        assert_eq!(
            conn.query(&format!("SELECT id FROM {table} ORDER BY id"))
                .await
                .unwrap()
                .rows,
            vec![vec![Value::Int(3)]],
            "the {engine} AFTER UPDATE trigger effect survives rollback"
        );
    }
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
    for (table, engine, _) in delete_effect_tables {
        assert_eq!(
            conn.query(&format!("SELECT id FROM {table} ORDER BY id"))
                .await
                .unwrap()
                .rows,
            vec![vec![Value::Int(1)]],
            "the {engine} AFTER DELETE trigger effect survives rollback"
        );
    }
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
async fn mysql_batch_rollback_preserves_trigger_session_variable_effects() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts.clone()).await;
    conn.execute("CREATE TABLE session_effect_parent (id INT PRIMARY KEY) ENGINE=InnoDB")
        .await
        .unwrap();
    conn.execute("INSERT INTO session_effect_parent VALUES (1)")
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
        "CREATE TRIGGER session_effect_parent_after_insert AFTER INSERT ON session_effect_parent \
         FOR EACH ROW SET @rollback_side_effect_count = COALESCE(@rollback_side_effect_count, 0) + 1",
    )
    .execute(&fixture)
    .await
    .unwrap();
    fixture.close().await;

    conn.execute("SET @rollback_side_effect_count = 0").await.unwrap();
    let batch = vec![
        ("INSERT INTO session_effect_parent VALUES (2)".into(), vec![]),
        ("INSERT INTO session_effect_parent VALUES (1)".into(), vec![]),
    ];
    let error = conn
        .execute_in_transaction(&batch)
        .await
        .expect_err("duplicate key must fail the batch");
    assert!(
        matches!(error, DriverError::Transaction { statement_index: 1, .. }),
        "got {error:?}"
    );
    assert_eq!(
        conn.query("SELECT id FROM session_effect_parent ORDER BY id")
            .await
            .unwrap()
            .rows,
        vec![vec![Value::Int(1)]],
        "the InnoDB row from the failed batch is rolled back"
    );
    assert_eq!(
        conn.query("SELECT @rollback_side_effect_count").await.unwrap().rows,
        vec![vec![Value::Int(1)]],
        "the trigger's session variable side effect survives rollback"
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
