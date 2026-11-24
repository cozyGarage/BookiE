#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use bookie_core::{ConnectOptions, DatabaseDriver, OperationControl, Value};
use drivers_sqlite::SqliteDriver;

#[tokio::test]
async fn missing_bind_values_never_modify_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    std::fs::File::create(&path).unwrap();
    let conn = SqliteDriver
        .connect(ConnectOptions {
            database: path.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b TEXT)")
        .await
        .unwrap();
    conn.execute("INSERT INTO t VALUES (1, 'original', 'kept')")
        .await
        .unwrap();
    let err = conn.execute("UPDATE t SET a = ? WHERE id = 1").await.unwrap_err();
    assert!(err.to_string().contains("expects 1 parameters, but 0"), "{err}");
    let err = conn
        .query_params(
            "UPDATE t SET a = ?, b = ? WHERE id = 1 RETURNING a",
            &[Value::Text("changed".into())],
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("expects 2 parameters, but 1"), "{err}");
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(10));
    let mut session = conn.open_session().await.unwrap();
    assert!(
        session
            .query_params_controlled("UPDATE t SET a = ? WHERE id = 1", &[], &control)
            .await
            .is_err()
    );
    session.close().await.unwrap();
    assert!(
        conn.execute_in_transaction(&[("UPDATE t SET a = ? WHERE id = 1".into(), vec![])])
            .await
            .is_err()
    );
    let mut tx = conn.begin().await.unwrap();
    assert!(tx.execute("UPDATE t SET a = ? WHERE id = 1").await.is_err());
    tx.rollback().await.unwrap();
    let result = conn.query("SELECT a, b FROM t").await.unwrap();
    assert_eq!(
        result.rows[0],
        [Value::Text("original".into()), Value::Text("kept".into())]
    );
}
