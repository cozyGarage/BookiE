use super::*;

async fn start_clickhouse_row_stream(
    tag: &'static str,
) -> (
    ContainerAsync<GenericImage>,
    ConnectOptions,
    std::sync::Arc<dyn tablepro_core::Connection>,
    tokio::task::JoinHandle<Result<tablepro_core::QueryResult, DriverError>>,
) {
    let (container, opts) = super::start_clickhouse().await;
    let connection: std::sync::Arc<dyn tablepro_core::Connection> =
        ClickhouseDriver.connect(opts.clone()).await.expect("connect").into();
    let observer = super::connect(opts.clone()).await;
    let running = connection.clone();
    let task = tokio::spawn(async move {
        running
            .query(&format!(
                "SELECT sleepEachRow(0.05) FROM numbers(1000) \
                 SETTINGS max_block_size = 1 /* {tag} */"
            ))
            .await
    });

    let mut running_query = false;
    for _ in 0..200 {
        let result = observer
            .query(&format!(
                "SELECT query_id FROM system.processes \
                 WHERE query LIKE '%{tag}%' AND query NOT LIKE '%system.processes%'"
            ))
            .await
            .expect("inspect active ClickHouse query");
        if !result.rows.is_empty() {
            running_query = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(running_query, "tagged row-stream query must reach system.processes");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    (container, opts, connection, task)
}

async fn restart_clickhouse_and_query(container: &ContainerAsync<GenericImage>, opts: ConnectOptions) {
    container.start().await.expect("restart ClickHouse");
    let mut replacement_options = opts;
    replacement_options.host = container.get_host().await.expect("restarted host").to_string();
    replacement_options.port = container
        .get_host_port_ipv4(8123)
        .await
        .expect("restarted ClickHouse port");
    let recovered = super::server_restart::retry_operation("ClickHouse", || {
        let options = replacement_options.clone();
        async move {
            let replacement = ClickhouseDriver.connect(options).await?;
            replacement.query("SELECT 1").await
        }
    })
    .await
    .expect("ClickHouse restart accepts a fresh query");
    assert_eq!(recovered.rows, vec![vec![Value::Int(1)]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn server_loss_during_row_stream_fails_the_whole_query_as_disconnected() {
    let (container, opts, connection, task) =
        start_clickhouse_row_stream("bookie_clickhouse_hard_stop_midstream").await;
    container
        .stop_with_timeout(Some(0))
        .await
        .expect("forcibly stop ClickHouse during row stream");

    let error = tokio::time::timeout(std::time::Duration::from_secs(10), task)
        .await
        .expect("interrupted ClickHouse row stream must not hang")
        .expect("query task")
        .expect_err("interrupted row stream must not return a partial result");
    assert!(
        matches!(error, DriverError::Disconnected),
        "mid-stream ClickHouse server loss must be disconnected, got {error:?}"
    );

    assert!(matches!(
        connection.query("SELECT 1").await,
        Err(DriverError::Disconnected)
    ));
    restart_clickhouse_and_query(&container, opts).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn graceful_server_stop_during_row_stream_returns_an_error_not_partial_rows() {
    let (container, _opts, _connection, task) =
        start_clickhouse_row_stream("bookie_clickhouse_graceful_stop_midstream").await;
    container
        .stop()
        .await
        .expect("gracefully stop ClickHouse during row stream");

    let error = tokio::time::timeout(std::time::Duration::from_secs(10), task)
        .await
        .expect("interrupted ClickHouse row stream must not hang")
        .expect("query task")
        .expect_err("a terminal ClickHouse exception row must not be returned as data");
    assert!(
        matches!(
            error,
            DriverError::Query { ref message, .. }
                if message.contains("Code: 394") && message.contains("Query was cancelled")
        ),
        "graceful server shutdown must preserve the server's cancellation error, got {error:?}"
    );
}
