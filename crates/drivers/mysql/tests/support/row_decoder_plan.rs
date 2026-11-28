use std::str::FromStr;

use bookie_core::{ConnectOptions, OperationControl, Value};
use rust_decimal::Decimal;
use secrecy::ExposeSecret;
use sqlx::Connection as _;

use crate::{connect, start_mariadb, start_mysql};

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(60))
}

async fn run_raw(opts: &ConnectOptions, sql: &str) {
    let raw_options = sqlx::mysql::MySqlConnectOptions::new()
        .host(&opts.host)
        .port(opts.port)
        .database(&opts.database)
        .username(&opts.username)
        .password(opts.password.expose_secret())
        .ssl_mode(sqlx::mysql::MySqlSslMode::Disabled);
    let mut raw = sqlx::MySqlConnection::connect_with(&raw_options).await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(&mut raw)
        .await
        .unwrap();
    raw.close().await.unwrap();
}

async fn assert_bound_parameter_columns_decode_from_row_metadata(opts: ConnectOptions) {
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE coalesce_source (v INT NULL)").await.unwrap();
    conn.execute("INSERT INTO coalesce_source VALUES (NULL)").await.unwrap();

    let selected = conn.query_params("SELECT ?", &[Value::Int(1)]).await.unwrap();
    assert!(!selected.truncated);
    assert_eq!(selected.rows, vec![vec![Value::Int(1)]]);

    let coalesced = conn
        .query_params("SELECT COALESCE(v, ?) FROM coalesce_source", &[Value::Int(1)])
        .await
        .unwrap();
    assert!(!coalesced.truncated);
    assert_eq!(coalesced.rows, vec![vec![Value::Int(1)]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_bound_parameter_columns_decode_as_their_executed_type() {
    let (_db, opts) = start_mysql().await;
    assert_bound_parameter_columns_decode_from_row_metadata(opts).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_bound_parameter_columns_decode_as_their_executed_type() {
    let (_db, opts) = start_mariadb().await;
    assert_bound_parameter_columns_decode_from_row_metadata(opts).await;
}

async fn assert_parameterised_call_keeps_each_result_set(opts: ConnectOptions) {
    run_raw(
        &opts,
        "CREATE PROCEDURE two_shapes(IN n INT) BEGIN SELECT n AS first_n; SELECT 'b' AS label, n * 2.5 AS scaled; END",
    )
    .await;
    let conn = connect(opts).await;
    let expected_second = vec![vec![
        Value::Text("b".into()),
        Value::Decimal(Decimal::from_str("5.0").unwrap()),
    ]];

    let batch = conn
        .query_result_sets_controlled("CALL two_shapes(?)", &[Value::Int(2)], &control())
        .await
        .unwrap();
    assert!(!batch.truncated);
    assert_eq!(batch.result_sets.len(), 2);
    assert!(batch.result_sets.iter().all(|set| !set.truncated));
    assert_eq!(batch.result_sets[0].columns.len(), 1);
    assert_eq!(batch.result_sets[0].rows, vec![vec![Value::Int(2)]]);
    assert_eq!(batch.result_sets[1].columns.len(), 2);
    assert_eq!(batch.result_sets[1].rows, expected_second);

    let bounded = conn
        .query_result_sets_bounded_controlled("CALL two_shapes(?)", &[Value::Int(2)], 10, 4096, &control())
        .await
        .unwrap();
    assert!(!bounded.truncated);
    assert_eq!(bounded.result_sets.len(), 2);
    assert_eq!(bounded.result_sets[1].rows, expected_second);
}

async fn assert_call_keeps_a_prefix_after_byte_overflow(opts: ConnectOptions) {
    run_raw(
        &opts,
        "CREATE PROCEDURE call_byte_prefix_4274() BEGIN \
         SELECT ordinal, REPEAT( \
             CASE WHEN ordinal = 8 THEN 'b' WHEN ordinal = 9 THEN 'c' ELSE 'a' END, \
             CASE WHEN ordinal = 8 THEN 10485760 WHEN ordinal = 9 THEN 1048576 ELSE 8388608 END \
         ) AS payload FROM ( \
             SELECT 1 AS ordinal UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL \
             SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL \
             SELECT 8 UNION ALL SELECT 9 \
         ) AS numbered_rows ORDER BY ordinal; \
         SELECT 99 AS sentinel; \
         END",
    )
    .await;
    let conn = connect(opts).await;
    let batch = conn
        .query_result_sets_controlled("CALL call_byte_prefix_4274()", &[], &control())
        .await
        .unwrap();

    assert!(batch.truncated);
    assert_eq!(batch.result_sets.len(), 2);
    let result = &batch.result_sets[0];
    assert!(result.truncated);
    assert_eq!(
        result.rows.iter().map(|row| row[0].clone()).collect::<Vec<_>>(),
        (1..=7).map(Value::Int).collect::<Vec<_>>()
    );
    assert!(result.rows.iter().all(|row| {
        matches!(
            row.get(1),
            Some(Value::Text(value)) if value.len() == 8_388_608 && value.starts_with('a')
        )
    }));
    let next_result = &batch.result_sets[1];
    assert!(!next_result.truncated);
    assert_eq!(next_result.rows, vec![vec![Value::Int(99)]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_parameterised_call_returns_each_result_set_untruncated() {
    let (_db, opts) = start_mysql().await;
    assert_parameterised_call_keeps_each_result_set(opts).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_parameterised_call_returns_each_result_set_untruncated() {
    let (_db, opts) = start_mariadb().await;
    assert_parameterised_call_keeps_each_result_set(opts).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_call_byte_overflow_keeps_a_result_prefix() {
    let (_db, opts) = start_mysql().await;
    assert_call_keeps_a_prefix_after_byte_overflow(opts).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_call_byte_overflow_keeps_a_result_prefix() {
    let (_db, opts) = start_mariadb().await;
    assert_call_keeps_a_prefix_after_byte_overflow(opts).await;
}
