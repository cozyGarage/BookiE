#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::parse_input_for_grid_cell;
use tablepro_core::Value;

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_clickhouse_enum16_grid_edit_preserves_labels_and_refuses_invalid_values() {
    use tablepro_core::{ConnectOptions, DatabaseDriver, TlsConfig};
    use testcontainers::core::wait::HttpWaitStrategy;
    use testcontainers::core::{IntoContainerPort, WaitFor};
    use testcontainers::runners::AsyncRunner;
    use testcontainers::{GenericImage, ImageExt};

    let container = GenericImage::new("clickhouse/clickhouse-server", "24.8")
        .with_exposed_port(8123.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/ping")
                .with_port(8123.tcp())
                .with_expected_status_code(200u16),
        ))
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "tablepro")
        .with_env_var("CLICKHOUSE_DB", "default")
        .with_env_var("CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT", "1")
        .start()
        .await
        .expect("start ClickHouse fixture");
    let connection = drivers_clickhouse::ClickhouseDriver
        .connect(ConnectOptions {
            host: container.get_host().await.expect("container host").to_string(),
            port: container.get_host_port_ipv4(8123).await.expect("HTTP port"),
            database: "default".into(),
            username: "default".into(),
            password: secrecy::SecretString::new("tablepro".to_string().into()),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .expect("connect to ClickHouse fixture");
    connection
        .execute(
            "CREATE TABLE enum16_grid (
                id UInt8,
                state Nullable(Enum16('low' = -32768, 'NULL' = 0, 'high' = 32767, '' = 1, 'O''Brien' = 2))
            ) ENGINE = MergeTree ORDER BY id",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO enum16_grid VALUES (1, 'low'), (2, 'NULL'), (3, NULL), (4, 'high'), (5, 'low'), (6, 'low')",
        )
        .await
        .unwrap();

    let columns = connection.fetch_columns(None, "enum16_grid").await.unwrap();
    let id_index = columns.iter().position(|column| column.name == "id").unwrap();
    let state_index = columns.iter().position(|column| column.name == "state").unwrap();
    assert!(columns[id_index].primary_key, "id is the MergeTree ordering key");
    assert!(columns[state_index].nullable);
    assert!(columns[state_index].data_type.starts_with("Nullable(Enum16("));

    let high = parse_input_for_grid_cell("high", Some(&columns[state_index]), "clickhouse", None).unwrap();
    assert_eq!(high, Value::Text("high".into()));
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "clickhouse",
        None,
        "enum16_grid",
        &columns,
        &[(state_index, high)],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_in_transaction(&[update]).await.unwrap();

    let clear = parse_input_for_grid_cell("", Some(&columns[state_index]), "clickhouse", None).unwrap();
    assert_eq!(clear, Value::Null);
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "clickhouse",
        None,
        "enum16_grid",
        &columns,
        &[(state_index, clear)],
        &[Value::Int(2)],
    )
    .unwrap();
    connection.execute_in_transaction(&[update]).await.unwrap();

    let empty_label = parse_input_for_grid_cell("''", Some(&columns[state_index]), "clickhouse", None).unwrap();
    assert_eq!(empty_label, Value::Text(String::new()));
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "clickhouse",
        None,
        "enum16_grid",
        &columns,
        &[(state_index, empty_label)],
        &[Value::Int(5)],
    )
    .unwrap();
    connection.execute_in_transaction(&[update]).await.unwrap();

    let apostrophe_label =
        parse_input_for_grid_cell("'O''Brien'", Some(&columns[state_index]), "clickhouse", None).unwrap();
    assert_eq!(apostrophe_label, Value::Text("O'Brien".into()));
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "clickhouse",
        None,
        "enum16_grid",
        &columns,
        &[(state_index, apostrophe_label)],
        &[Value::Int(6)],
    )
    .unwrap();
    connection.execute_in_transaction(&[update]).await.unwrap();

    let before_refusal = connection
        .query("SELECT id, state, CAST(assumeNotNull(state) AS Int16), isNull(state) FROM enum16_grid ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        before_refusal.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("high".into()),
                Value::Int(32767),
                Value::Int(0)
            ],
            vec![Value::Int(2), Value::Null, Value::Int(0), Value::Int(1)],
            vec![Value::Int(3), Value::Null, Value::Int(0), Value::Int(1)],
            vec![
                Value::Int(4),
                Value::Text("high".into()),
                Value::Int(32767),
                Value::Int(0)
            ],
            vec![Value::Int(5), Value::Text(String::new()), Value::Int(1), Value::Int(0)],
            vec![
                Value::Int(6),
                Value::Text("O'Brien".into()),
                Value::Int(2),
                Value::Int(0)
            ],
        ]
    );

    let invalid = parse_input_for_grid_cell("not-a-label", Some(&columns[state_index]), "clickhouse", None).unwrap();
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "clickhouse",
        None,
        "enum16_grid",
        &columns,
        &[(state_index, invalid)],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(connection.execute_in_transaction(&[update]).await.is_err());
    let after_refusal = connection
        .query("SELECT id, state, CAST(assumeNotNull(state) AS Int16), isNull(state) FROM enum16_grid ORDER BY id")
        .await
        .unwrap();
    assert_eq!(after_refusal.rows, before_refusal.rows);
}
