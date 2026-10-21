use super::*;

#[test]
fn only_reads_the_cursor_guard_accepts_and_known_functions_are_streamed() {
    assert!(super::super::stream_candidate(
        "SELECT id, lower(name) FROM items",
        "postgres"
    ));
    for sql in [
        "SELECT archive_rows()",
        "SELECT set_config('search_path', 'x', false)",
        "SELECT * FROM items FOR UPDATE",
        "SELECT 1; SELECT 2",
    ] {
        assert!(!super::super::stream_candidate(sql, "postgres"), "{sql}");
        assert!(
            !super::super::stream_candidate(sql, "postgres")
                || tablepro_policy::classify(sql, "postgres").is_single_read(),
            "{sql}"
        );
    }
    assert!(!super::super::stream_candidate("SELECT id FROM items", "mysql"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_select_that_calls_a_writing_function_runs_outside_a_read_only_cursor() {
    use testcontainers::ImageExt;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres;

    let container = Postgres::default().with_tag("16-alpine").start().await.unwrap();
    let connection: std::sync::Arc<dyn tablepro_core::Connection> = std::sync::Arc::from(
        drivers_postgres::PgDriver
            .connect(ConnectOptions {
                host: container.get_host().await.unwrap().to_string(),
                port: container.get_host_port_ipv4(5432).await.unwrap(),
                database: "postgres".into(),
                username: "postgres".into(),
                password: secrecy::SecretString::new("postgres".into()),
                ..Default::default()
            })
            .await
            .unwrap(),
    );
    let control = crate::services::operation_control::bounded(30);
    for sql in [
        "CREATE TABLE archived (id int)",
        "CREATE FUNCTION archive_rows() RETURNS int LANGUAGE sql AS 'INSERT INTO archived VALUES (1) RETURNING id'",
    ] {
        connection.execute_controlled(sql, &control).await.unwrap();
    }
    let streamed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = streamed.clone();
    let result = run_statements_with_stream(
        StatementRun {
            target: StatementTarget::Pool(connection.clone()),
            statements: vec!["SELECT archive_rows()".into()],
            driver_id: "postgres",
            parameter_values: &Default::default(),
            control: &control,
            error_policy: BatchErrorPolicy::StopScript,
            settings_check: None,
            succeeded: |_: &str| {},
        },
        move |_| {
            seen.store(true, std::sync::atomic::Ordering::SeqCst);
            async { true }
        },
    )
    .await;

    let ScriptRunResult::Completed(outcomes) = result else {
        panic!("the statement runs without a cursor");
    };
    assert!(
        matches!(&outcomes[0].kind, StatementOutcomeKind::Rows(sets) if sets[0].rows == [[tablepro_core::Value::Int(1)]]),
        "{:?}",
        outcomes[0].kind
    );
    assert!(!streamed.load(std::sync::atomic::Ordering::SeqCst));
    let stored = connection
        .query_controlled("SELECT count(*) FROM archived", &control)
        .await
        .unwrap();
    assert_eq!(stored.rows, [[tablepro_core::Value::Int(1)]]);
}

async fn fetch_failure(sql: &str, parameters: &std::collections::HashMap<String, tablepro_core::Value>) -> String {
    let error = tablepro_core::DriverError::Query {
        message: "division by zero".into(),
        sqlstate: Some("22012".into()),
        position: Some(8),
    };
    let result = run_statements_with_stream(
        StatementRun {
            target: cursor_target(vec![Err(error)], std::sync::Arc::new(CursorCounters::default())),
            statements: vec![sql.into()],
            driver_id: "postgres",
            parameter_values: parameters,
            control: &crate::services::operation_control::bounded(30),
            error_policy: BatchErrorPolicy::StopScript,
            settings_check: None,
            succeeded: |_: &str| {},
        },
        |_| async { true },
    )
    .await;
    match result {
        ScriptRunResult::Streamed(StreamTerminal::Failed(message)) => message,
        _ => panic!("the fetch failure ends the stream"),
    }
}

#[tokio::test]
async fn a_fetch_error_is_located_only_when_the_sent_sql_is_the_written_sql() {
    let plain = fetch_failure("SELECT 1 / id FROM items", &Default::default()).await;
    assert!(plain.contains("Line 1, column 8"), "{plain}");

    let parameters = [("id".to_string(), tablepro_core::Value::Int(1))].into();
    let bound = fetch_failure("SELECT 1 / id FROM items WHERE id = :id", &parameters).await;
    assert!(bound.contains("division by zero"), "{bound}");
    assert!(!bound.contains("Line "), "{bound}");
}
