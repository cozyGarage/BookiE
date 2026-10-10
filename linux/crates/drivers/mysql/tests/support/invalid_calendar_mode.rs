use tablepro_core::{ConnectOptions, OperationControl, Value};

use super::{connect, start_mariadb, start_mysql};

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_allow_invalid_dates_preserves_native_calendar_text() {
    let (_container, options) = start_mysql().await;
    assert_invalid_calendar_text(&options, "MySQL").await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_allow_invalid_dates_preserves_native_calendar_text() {
    let (_container, options) = start_mariadb().await;
    assert_invalid_calendar_text(&options, "MariaDB").await;
}

async fn assert_invalid_calendar_text(options: &ConnectOptions, engine: &str) {
    let connection = connect(options.clone()).await;
    connection
        .execute("CREATE TABLE allow_invalid_dates (d DATE, dt DATETIME)")
        .await
        .unwrap();
    let mut session = connection.open_session().await.unwrap();
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    session
        .query_params_controlled("SET SESSION sql_mode = 'ALLOW_INVALID_DATES'", &[], &control)
        .await
        .unwrap();
    session
        .query_params_controlled(
            "INSERT INTO allow_invalid_dates VALUES ('2024-02-31', '2024-02-31 12:34:56')",
            &[],
            &control,
        )
        .await
        .unwrap_or_else(|error| panic!("{engine} must accept the calendar values in ALLOW_INVALID_DATES: {error}"));

    let result = session
        .query_params_controlled("SELECT d, dt FROM allow_invalid_dates", &[], &control)
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Text("2024-02-31".into()),
            Value::Text("2024-02-31 12:34:56".into())
        ]],
        "{engine} driver values"
    );
    let native = session
        .query_params_controlled(
            "SELECT CAST(d AS CHAR), CAST(dt AS CHAR), CAST(d + 0 AS CHAR), CAST(dt + 0 AS CHAR) \
             FROM allow_invalid_dates",
            &[],
            &control,
        )
        .await
        .unwrap();
    assert_eq!(
        native.rows,
        vec![vec![
            Value::Text("2024-02-31".into()),
            Value::Text("2024-02-31 12:34:56".into()),
            Value::Text("20240231".into()),
            Value::Text("20240231123456".into()),
        ]],
        "{engine} native storage oracle"
    );
}
