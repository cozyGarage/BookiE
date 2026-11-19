#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{connect, start_mariadb, start_mysql};
use bookie_core::Connection;

#[tokio::test]
#[ignore = "requires docker"]
async fn use_on_a_pooled_connection_does_not_change_what_later_catalog_reads_see() {
    let (_c, opts) = start_mysql().await;
    let connection = connect(opts.clone()).await;
    connection.execute("CREATE TABLE home_table (id INT)").await.unwrap();
    connection.execute("CREATE DATABASE other_db").await.unwrap();
    connection
        .execute("CREATE TABLE other_db.away_table (id INT)")
        .await
        .unwrap();

    connection
        .query_controlled(
            "USE other_db",
            &bookie_core::OperationControl::with_timeout(std::time::Duration::from_secs(5)),
        )
        .await
        .unwrap();

    for _ in 0..8 {
        let names: Vec<String> = connection
            .list_tables()
            .await
            .unwrap()
            .into_iter()
            .map(|table| table.name)
            .collect();
        assert_eq!(names, vec!["home_table".to_string()]);
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_session_keeps_user_variables_on_its_backend_across_use() {
    let (_container, opts) = start_mysql().await;
    let connection = connect(opts).await;
    connection.execute("CREATE DATABASE other_db").await.unwrap();
    let mut session = connection.open_session().await.unwrap();
    let control = bookie_core::OperationControl::with_timeout(std::time::Duration::from_secs(5));

    session
        .query_params_controlled("SET @bookie_session_marker = 'session-owned'", &[], &control)
        .await
        .unwrap();
    let before = session
        .query_params_controlled("SELECT CONNECTION_ID(), @bookie_session_marker", &[], &control)
        .await
        .unwrap();
    session
        .query_params_controlled("USE other_db", &[], &control)
        .await
        .unwrap();
    let after = session
        .query_params_controlled("SELECT CONNECTION_ID(), @bookie_session_marker", &[], &control)
        .await
        .unwrap();

    assert_eq!(before.rows[0][0], after.rows[0][0]);
    assert!(matches!(&after.rows[0][1], bookie_core::Value::Text(value) if value == "session-owned"));
}

async fn assert_session_variables_do_not_leak(connection: Box<dyn Connection>) {
    connection.execute("CREATE DATABASE other_db").await.unwrap();
    let control = bookie_core::OperationControl::with_timeout(std::time::Duration::from_secs(5));
    let probe = "SELECT @@session.sql_mode, @@session.time_zone, @bookie_leak_marker, DATABASE()";

    let mut first = connection.open_session().await.unwrap();
    let default_state = first.query_params_controlled(probe, &[], &control).await.unwrap();
    for statement in [
        "SET SESSION sql_mode = 'ANSI_QUOTES,NO_BACKSLASH_ESCAPES'",
        "SET SESSION time_zone = '+05:00'",
        "SET @bookie_leak_marker = 'first-session'",
        "USE other_db",
    ] {
        first.query_params_controlled(statement, &[], &control).await.unwrap();
    }
    let after_use = first.query_params_controlled(probe, &[], &control).await.unwrap();
    assert_eq!(
        after_use.rows[0],
        vec![
            bookie_core::Value::Text("ANSI_QUOTES,NO_BACKSLASH_ESCAPES".into()),
            bookie_core::Value::Text("+05:00".into()),
            bookie_core::Value::Text("first-session".into()),
            bookie_core::Value::Text("other_db".into()),
        ]
    );
    first.close().await.unwrap();

    let mut second = connection.open_session().await.unwrap();
    let fresh = second.query_params_controlled(probe, &[], &control).await.unwrap();
    assert_eq!(fresh.rows[0][0..3], default_state.rows[0][0..3]);
    assert_eq!(fresh.rows[0][3], default_state.rows[0][3]);
    assert!(matches!(fresh.rows[0][2], bookie_core::Value::Null));
    second.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_session_keeps_system_variables_across_use_and_does_not_leak_them_to_the_next_session() {
    let (_container, opts) = start_mysql().await;
    assert_session_variables_do_not_leak(connect(opts).await).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_session_keeps_system_variables_across_use_without_leaking() {
    let (_container, opts) = start_mariadb().await;
    assert_session_variables_do_not_leak(connect(opts).await).await;
}

async fn first_value(
    session: &mut Box<dyn bookie_core::Session>,
    sql: &str,
    control: &bookie_core::OperationControl,
) -> bookie_core::Value {
    let result = session.query_params_controlled(sql, &[], control).await.unwrap();
    result
        .rows
        .first()
        .map_or(bookie_core::Value::Null, |row| row[0].clone())
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_session_reads_the_new_database_after_use_even_for_identical_sql() {
    let (_container, opts) = start_mysql().await;
    let connection = connect(opts).await;
    connection.execute("CREATE DATABASE other_db").await.unwrap();
    connection.execute("CREATE TABLE t (v VARCHAR(5))").await.unwrap();
    connection.execute("INSERT INTO t VALUES ('home')").await.unwrap();
    connection
        .execute("CREATE TABLE other_db.t (v VARCHAR(5))")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO other_db.t VALUES ('away')")
        .await
        .unwrap();
    let mut session = connection.open_session().await.unwrap();
    let control = bookie_core::OperationControl::with_timeout(std::time::Duration::from_secs(5));
    let text = |value: &str| bookie_core::Value::Text(value.into());

    assert_eq!(
        first_value(&mut session, "SELECT v FROM t", &control).await,
        text("home")
    );
    assert_eq!(
        first_value(&mut session, "SELECT DATABASE()", &control).await,
        text("test")
    );
    first_value(&mut session, "USE other_db", &control).await;
    assert_eq!(
        first_value(&mut session, "SELECT v FROM t", &control).await,
        text("away")
    );
    assert_eq!(
        first_value(&mut session, "SELECT DATABASE()", &control).await,
        text("other_db")
    );
    first_value(&mut session, "USE test", &control).await;
    assert_eq!(
        first_value(&mut session, "SELECT v FROM t", &control).await,
        text("home")
    );
}

#[path = "../../../shared/pool_saturation.rs"]
mod contract;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_health_ping_does_not_report_a_disconnect_while_every_pooled_connection_is_busy() {
    contract::a_ping_does_not_report_a_disconnect_while_every_pooled_connection_is_busy(
        start_mysql,
        connect,
        "SELECT SLEEP(12)",
    )
    .await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_health_ping_still_reports_a_pool_that_cannot_reach_its_server() {
    contract::a_ping_still_reports_a_server_that_cannot_be_reached(
        super::shared_container::start_mysql_durable,
        connect,
        |server| server.stop(),
    )
    .await;
}
