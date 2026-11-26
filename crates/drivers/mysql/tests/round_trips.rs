#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Counts the network round trips a statement costs on MySQL, for #3739. A TCP proxy sits between the driver
//! and a MySQL container and counts bursts. Run with `--ignored --nocapture`; it needs Docker and prints the
//! counts. `ROUND_TRIP_TRACE=1` prints each burst.

#[path = "../../shared/tests/counting_proxy.rs"]
mod counting_proxy;

use bookie_core::{ConnectOptions, DatabaseDriver, OperationControl, TlsConfig};
use counting_proxy::CountingProxy;
use drivers_mysql::MysqlDriver;
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires docker; prints the round trips per statement measured for #3739"]
async fn the_round_trips_each_statement_costs_are_counted() {
    const RUNS: u64 = 20;
    let container = Mysql::default()
        .with_tag("8.4.13")
        .with_env_var("MYSQL_ROOT_PASSWORD", "bookie_test")
        .with_cmd([
            "--mysql-native-password=ON",
            "--authentication-policy=mysql_native_password",
        ])
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .unwrap();
    let proxy = CountingProxy::start(container.get_host_port_ipv4(3306).await.unwrap()).await;
    let connection = MysqlDriver
        .connect(ConnectOptions {
            host: "127.0.0.1".into(),
            port: proxy.port,
            database: "test".into(),
            username: "root".into(),
            password: SecretString::from("bookie_test"),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE trips (id INT PRIMARY KEY, name VARCHAR(20))")
        .await
        .unwrap();
    for _ in 0..3 {
        connection.query("SELECT 1").await.unwrap();
        connection.query("SELECT id, name FROM trips").await.unwrap();
    }
    let control = || OperationControl::with_timeout(std::time::Duration::from_secs(30));
    let per_statement = |label: &str, bursts: u64| {
        let trips = bursts as f64 / RUNS as f64;
        println!("round trips per {label}: {trips:.2}");
        trips
    };

    if std::env::var_os("ROUND_TRIP_TRACE").is_some() {
        proxy.reset();
        proxy.trace(true);
        for round in 1..=3 {
            println!("SELECT 1 (query_controlled), run {round}:");
            connection.query_controlled("SELECT 1", &control()).await.unwrap();
        }
        proxy.trace(false);
    }

    proxy.reset();
    for _ in 0..RUNS {
        connection.query("SELECT 1").await.unwrap();
    }
    let plain = per_statement("SELECT 1 (query)", proxy.bursts());

    proxy.reset();
    for _ in 0..RUNS {
        connection.query_controlled("SELECT 1", &control()).await.unwrap();
    }
    let controlled = per_statement("SELECT 1 (query_controlled)", proxy.bursts());

    proxy.reset();
    for _ in 0..RUNS {
        connection.query("SELECT id, name FROM trips").await.unwrap();
    }
    let table = per_statement("table SELECT (query)", proxy.bursts());
    assert!(
        plain < 5.5,
        "a plain statement was 7.1 to 7.25 round trips before the setup was merged: {plain}"
    );
    assert!(
        controlled < 6.5,
        "a controlled statement was 9.2 to 9.3 round trips before: {controlled}"
    );
    assert!(table < 5.5, "a table statement was 7.2 round trips before: {table}");
}
