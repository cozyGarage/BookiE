#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::time::Duration;

use bookie_core::{ConnectOptions, DatabaseDriver, TlsConfig};
use bookie_pg_sessions::{blockers, list_sessions};
use drivers_postgres::PgDriver;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres as PostgresImage;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_session_waiting_on_a_row_lock_names_its_blocker() {
    let container = PostgresImage::default().with_tag("16-alpine").start().await.unwrap();
    let options = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(5432).await.unwrap(),
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let setup = PgDriver.connect(options.clone()).await.unwrap();
    setup.execute("CREATE TABLE held (id int PRIMARY KEY)").await.unwrap();
    setup.execute("INSERT INTO held VALUES (1)").await.unwrap();

    let holder_connection = PgDriver.connect(options.clone()).await.unwrap();
    let mut holder = holder_connection.begin().await.unwrap();
    holder.execute("UPDATE held SET id = 1 WHERE id = 1").await.unwrap();
    let waiter = PgDriver.connect(options.clone()).await.unwrap();
    let waiting = tokio::spawn(async move { waiter.execute("UPDATE held SET id = 1 WHERE id = 1").await });
    tokio::time::sleep(Duration::from_secs(2)).await;

    let sessions = list_sessions(&*setup).await.unwrap();
    let ranked = blockers(&sessions);
    assert_eq!(ranked.len(), 1, "{sessions:#?}");
    let blocker = sessions
        .iter()
        .find(|s| s.pid == ranked[0].0)
        .expect("the blocker is listed");
    assert!(blocker.query.contains("UPDATE held"), "{blocker:?}");
    assert!(
        sessions
            .iter()
            .any(|s| s.blocked_by == [blocker.pid] && s.wait.as_deref().is_some_and(|w| w.starts_with("Lock")))
    );

    holder.commit().await.unwrap();
    waiting.await.unwrap().unwrap();
}
