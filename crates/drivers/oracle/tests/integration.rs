#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../shared/connect_refusal.rs"]
mod connect_refusal;

use bookie_core::{ConnectOptions, Connection, DatabaseDriver, DriverError, TlsConfig, Value};
use drivers_oracle::OracleDriver;
use rust_decimal::Decimal;
use secrecy::SecretString;
use std::str::FromStr;
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::IntoContainerPort, core::WaitFor, runners::AsyncRunner,
};

const PASSWORD: &str = "bookie_it_admin";

#[tokio::test]
async fn an_unavailable_oracle_listener_is_classified_as_connection_refused() {
    connect_refusal::assert_connection_refused(&OracleDriver)
        .await
        .expect("Oracle setup refusal remains distinct from an established disconnect");
}

async fn start_oracle() -> (ContainerAsync<GenericImage>, u16) {
    let container = GenericImage::new("gvenzl/oracle-free", "slim-faststart")
        .with_exposed_port(1521.tcp())
        .with_wait_for(WaitFor::message_on_stdout("DATABASE IS READY TO USE!"))
        .with_env_var("ORACLE_PASSWORD", PASSWORD)
        .start()
        .await
        .expect("start Oracle Free");
    let port = container.get_host_port_ipv4(1521).await.expect("port");
    (container, port)
}

fn options(port: u16, user: &str, password: &str) -> ConnectOptions {
    ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        database: "FREEPDB1".into(),
        username: user.into(),
        password: SecretString::new(password.to_string().into()),
        tls: TlsConfig::disabled(),
        ..ConnectOptions::default()
    }
}

async fn seed(port: u16) {
    tokio::task::spawn_blocking(move || {
        let config = oracledb::Config::default()
            .set_credentials("system", PASSWORD)
            .set_connect_string(&format!("127.0.0.1:{port}/FREEPDB1"))
            .unwrap();
        let admin = oracledb::connect(config).unwrap();
        for statement in [
            "create user bookie_it identified by bookie_it_pass quota unlimited on users",
            "grant create session to bookie_it",
            "create table bookie_it.items (id number(10) primary key, label varchar2(40) not null, \
             amount number(38,10), stamp timestamp(6), note clob)",
            "comment on column bookie_it.items.label is 'display label'",
            "insert into bookie_it.items values (1, 'one', 12345678901234567890.12345678, \
             timestamp '2026-11-04 23:59:59.123456', 'first note')",
            "insert into bookie_it.items values (2, 'two', null, null, null)",
            "insert into bookie_it.items values (3, 'three', 0.10, timestamp '2000-01-01 00:00:00', 'third')",
            "insert into bookie_it.items values (4, 'four', 4, null, null)",
            "insert into bookie_it.items values (5, 'five', -5.5, null, null)",
            "commit",
            "grant select on bookie_it.items to bookie_it",
        ] {
            admin.execute(statement, &[]).unwrap();
        }
    })
    .await
    .unwrap();
}

async fn connected(port: u16) -> Box<dyn Connection> {
    OracleDriver
        .connect(options(port, "system", PASSWORD))
        .await
        .expect("connect")
}

#[tokio::test]
#[ignore = "requires docker"]
async fn the_driver_reads_typed_values_from_a_real_oracle_without_losing_digits() {
    let (_container, port) = start_oracle().await;
    seed(port).await;
    let connection = connected(port).await;
    let result = connection
        .query("select id, label, amount, stamp, note from bookie_it.items order by id")
        .await
        .unwrap();
    let rows: Vec<Vec<Value>> = result.rows.iter().map(|row| row.to_vec()).collect();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0][1], Value::Text("one".into()));
    assert_eq!(
        rows[0][2],
        Value::Decimal(Decimal::from_str("12345678901234567890.12345678").unwrap())
    );
    assert_eq!(rows[1][2], Value::Null);
    assert_eq!(rows[2][2], Value::Decimal(Decimal::from_str("0.10").unwrap()));
    assert_eq!(rows[0][4], Value::Text("first note".into()));
    assert_eq!(rows[1][4], Value::Null);
    let expected_stamp = chrono::NaiveDate::from_ymd_opt(2026, 11, 4)
        .and_then(|date| date.and_hms_micro_opt(23, 59, 59, 123_456))
        .unwrap();
    assert_eq!(rows[0][3], Value::DateTime(expected_stamp));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn the_driver_refuses_every_write_and_leaves_the_data_unchanged() {
    let (_container, port) = start_oracle().await;
    seed(port).await;
    let connection = connected(port).await;
    for sql in [
        "delete from bookie_it.items",
        "update bookie_it.items set label = 'x'",
        "drop table bookie_it.items",
        "select * from bookie_it.items for update",
    ] {
        let error = connection.query(sql).await.unwrap_err();
        assert!(matches!(error, DriverError::ReadOnly), "{sql}: {error:?}");
    }
    assert!(matches!(
        connection.execute("delete from bookie_it.items").await,
        Err(DriverError::ReadOnly)
    ));
    assert!(matches!(
        connection
            .execute_in_transaction(&[("delete from bookie_it.items".into(), vec![])])
            .await,
        Err(DriverError::ReadOnly)
    ));
    let count = connection.query("select count(*) from bookie_it.items").await.unwrap();
    assert_eq!(count.rows.iter().next().unwrap()[0], Value::Decimal(Decimal::from(5)));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn the_catalog_lists_user_tables_and_describes_their_columns() {
    let (_container, port) = start_oracle().await;
    seed(port).await;
    let connection = connected(port).await;
    let tables = connection.list_tables().await.unwrap();
    assert!(
        tables
            .iter()
            .any(|table| table.schema.as_deref() == Some("BOOKIE_IT") && table.name == "ITEMS")
    );
    assert!(
        tables.iter().all(|table| table.schema.as_deref() != Some("SYS")),
        "{tables:?}"
    );
    let columns = connection.fetch_columns(Some("BOOKIE_IT"), "ITEMS").await.unwrap();
    let names: Vec<&str> = columns.iter().map(|column| column.name.as_str()).collect();
    assert_eq!(names, ["ID", "LABEL", "AMOUNT", "STAMP", "NOTE"]);
    assert!(columns[0].primary_key && !columns[0].nullable);
    assert!(!columns[1].primary_key && !columns[1].nullable);
    assert_eq!(columns[1].comment.as_deref(), Some("display label"));
    assert!(columns[2].nullable);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn browsing_pages_by_offset_and_limit_in_a_stable_order() {
    let (_container, port) = start_oracle().await;
    seed(port).await;
    let connection = connected(port).await;
    let page = connection.fetch_rows(Some("BOOKIE_IT"), "ITEMS", 2, 2).await.unwrap();
    assert_eq!(page.rows.iter().count(), 2);
    assert!(!page.truncated);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_wrong_password_is_reported_as_an_authentication_failure() {
    let (_container, port) = start_oracle().await;
    let error = OracleDriver
        .connect(options(port, "system", "not-the-password"))
        .await
        .err()
        .unwrap();
    assert!(matches!(error, DriverError::AuthFailed), "{error:?}");
}
