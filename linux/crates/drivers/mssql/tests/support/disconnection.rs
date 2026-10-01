#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_disconnected_session_is_retired_after_server_loss() {
    let (container, opts) = start_mssql().await;
    let connection = connect(opts.clone()).await;
    let observer = connect(opts).await;
    let mut session = connection.open_session().await.expect("open session");
    let tag = "tablepro_session_disconnect_probe";
    let query = tokio::spawn(async move {
        let result = session
            .query_params_controlled(
                &format!("/* {tag} */ WAITFOR DELAY '00:00:30'"),
                &[],
                &OperationControl::with_timeout(std::time::Duration::from_secs(40)),
            )
            .await;
        (session, result)
    });

    let mut active = false;
    for _ in 0..100 {
        let probe = observer
            .query(&format!(
                "SELECT COUNT(*) AS active_count FROM sys.dm_exec_requests r \
                 CROSS APPLY sys.dm_exec_sql_text(r.sql_handle) t \
                 WHERE r.session_id <> @@SPID AND t.text LIKE N'%{tag}%'"
            ))
            .await
            .expect("inspect active SQL Server session query");
        if probe.rows == vec![vec![Value::Int(1)]] {
            active = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(active, "session query must reach WAITFOR before server loss");
    container.stop().await.expect("stop SQL Server during session query");

    let (session, result) = tokio::time::timeout(std::time::Duration::from_secs(20), query)
        .await
        .expect("disconnected session query must finish promptly")
        .expect("session query task");
    let error = result.expect_err("server loss must fail the session query");
    assert!(matches!(error, DriverError::Disconnected), "{error:?}");
    assert!(!session.is_usable(), "a disconnected session must be retired");
}
