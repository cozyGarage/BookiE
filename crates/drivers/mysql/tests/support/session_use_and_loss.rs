#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{Connection, DriverError, OperationControl, Value};

use crate::{connect, start_mariadb, start_mysql};

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(5))
}

async fn count(connection: &dyn Connection, table: &str) -> Value {
    connection
        .query(&format!("SELECT COUNT(*) FROM {table}"))
        .await
        .unwrap()
        .rows[0][0]
        .clone()
}

async fn session_with_tables(
    mariadb: bool,
) -> (
    Box<dyn std::any::Any>,
    Box<dyn Connection>,
    Box<dyn bookie_core::Session>,
) {
    let (container, opts): (Box<dyn std::any::Any>, _) = if mariadb {
        let (container, opts) = start_mariadb().await;
        (Box::new(container), opts)
    } else {
        let (container, opts) = start_mysql().await;
        (Box::new(container), opts)
    };
    let connection = connect(opts).await;
    connection.execute("CREATE TABLE ledger (id INT)").await.unwrap();
    connection.execute("CREATE DATABASE other_db").await.unwrap();
    let session = connection.open_session().await.unwrap();
    (container, connection, session)
}

#[tokio::test]
#[ignore = "requires docker"]
async fn autocommit_off_survives_use_and_a_rollback_still_undoes_the_insert() {
    for mariadb in [false, true] {
        let (_container, connection, mut session) = session_with_tables(mariadb).await;
        let control = control();

        session
            .query_params_controlled("SET autocommit = 0", &[], &control)
            .await
            .unwrap();
        session
            .query_params_controlled("USE other_db", &[], &control)
            .await
            .unwrap();
        let autocommit = session
            .query_params_controlled("SELECT @@autocommit, DATABASE()", &[], &control)
            .await
            .unwrap();
        session
            .query_params_controlled("INSERT INTO test.ledger VALUES (1)", &[], &control)
            .await
            .unwrap();
        session
            .query_params_controlled("ROLLBACK", &[], &control)
            .await
            .unwrap();

        assert_eq!(autocommit.rows[0][0], Value::Int(0), "autocommit stays off after USE");
        assert_eq!(autocommit.rows[0][1], Value::Text("other_db".into()));
        assert_eq!(count(connection.as_ref(), "ledger").await, Value::Int(0));
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_use_inside_an_open_transaction_does_not_end_the_transaction() {
    for mariadb in [false, true] {
        let (_container, connection, mut session) = session_with_tables(mariadb).await;
        let control = control();

        for sql in [
            "START TRANSACTION",
            "INSERT INTO test.ledger VALUES (7)",
            "USE other_db",
            "ROLLBACK",
        ] {
            session.query_params_controlled(sql, &[], &control).await.unwrap();
        }

        assert_eq!(count(connection.as_ref(), "ledger").await, Value::Int(0));
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn use_of_a_missing_database_fails_and_the_session_stays_where_it_was() {
    for mariadb in [false, true] {
        let (_container, _connection, mut session) = session_with_tables(mariadb).await;
        let control = control();
        session
            .query_params_controlled("USE other_db", &[], &control)
            .await
            .unwrap();

        let failed = session
            .query_params_controlled("USE no_such_database", &[], &control)
            .await;
        let current = session
            .query_params_controlled("SELECT DATABASE()", &[], &control)
            .await
            .unwrap();

        assert!(matches!(failed, Err(DriverError::Query { .. })), "{failed:?}");
        assert_eq!(current.rows[0][0], Value::Text("other_db".into()));
    }
}

async fn kill_backend_of(connection: &dyn Connection, session: &mut Box<dyn bookie_core::Session>) {
    let id = session
        .query_params_controlled("SELECT CONNECTION_ID()", &[], &control())
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let Value::Int(id) = id else {
        panic!("connection id is an integer, got {id:?}");
    };
    connection.execute(&format!("KILL {id}")).await.unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_session_whose_backend_was_killed_reports_the_loss_instead_of_continuing_on_a_new_one() {
    for mariadb in [false, true] {
        let (_container, connection, mut session) = session_with_tables(mariadb).await;
        let control = control();
        session
            .query_params_controlled("SET @marker = 'lost-with-the-backend'", &[], &control)
            .await
            .unwrap();

        kill_backend_of(connection.as_ref(), &mut session).await;
        let next = session.query_params_controlled("SELECT @marker", &[], &control).await;

        assert!(
            next.is_err(),
            "a silent reconnect would answer with NULL and hide that session state was lost: {next:?}"
        );
        assert_eq!(
            connection.query("SELECT 1").await.unwrap().rows[0][0],
            Value::Int(1),
            "the pooled connection still works after a session's backend was killed"
        );
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_transaction_on_a_killed_backend_is_rolled_back_and_never_replayed() {
    for mariadb in [false, true] {
        let (_container, connection, mut session) = session_with_tables(mariadb).await;
        let control = control();
        for sql in ["START TRANSACTION", "INSERT INTO ledger VALUES (5)"] {
            session.query_params_controlled(sql, &[], &control).await.unwrap();
        }

        kill_backend_of(connection.as_ref(), &mut session).await;
        let commit = session.query_params_controlled("COMMIT", &[], &control).await;
        let retry = session.query_params_controlled("COMMIT", &[], &control).await;

        assert!(
            commit.is_err(),
            "the commit must not report success after the kill: {commit:?}"
        );
        assert!(
            retry.is_err(),
            "a second attempt must not reconnect and commit: {retry:?}"
        );
        assert_eq!(count(connection.as_ref(), "ledger").await, Value::Int(0));
    }
}
