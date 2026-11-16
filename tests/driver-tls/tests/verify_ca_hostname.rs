#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::path::PathBuf;

use bookie_core::{ConnectOptions, DatabaseDriver, TlsMode};
use bookie_driver_tls_tests::{DriverTlsFixture, assert_verify_ca_hostname_behavior};
use drivers_clickhouse::ClickhouseDriver;
use drivers_mongodb::MongodbDriver;
use drivers_mssql::MssqlDriver;
use drivers_mysql::MysqlDriver;
use drivers_postgres::PgDriver;
use drivers_redis::RedisDriver;

type OptionsFactory = fn(&DriverTlsFixture, TlsMode, Option<PathBuf>) -> ConnectOptions;

struct EngineCase {
    driver: &'static dyn DatabaseDriver,
    options: OptionsFactory,
    service_host: &'static str,
    service_port: u16,
    socket_name: Option<&'static str>,
    checks_hostname: bool,
}

const ENGINES: [EngineCase; 6] = [
    EngineCase {
        driver: &PgDriver,
        options: DriverTlsFixture::postgres,
        service_host: "postgres",
        service_port: 5432,
        socket_name: Some(".s.PGSQL.5432"),
        checks_hostname: false,
    },
    EngineCase {
        driver: &MysqlDriver,
        options: DriverTlsFixture::mysql,
        service_host: "mysql-ssh.bookie.test",
        service_port: 3306,
        socket_name: Some("mysqld.sock"),
        checks_hostname: false,
    },
    EngineCase {
        driver: &MongodbDriver,
        options: DriverTlsFixture::mongo,
        service_host: "mongo.bookie.test",
        service_port: 27017,
        socket_name: None,
        checks_hostname: true,
    },
    EngineCase {
        driver: &RedisDriver,
        options: DriverTlsFixture::redis,
        service_host: "redis.bookie.test",
        service_port: 6379,
        socket_name: None,
        checks_hostname: true,
    },
    EngineCase {
        driver: &ClickhouseDriver,
        options: DriverTlsFixture::clickhouse,
        service_host: "clickhouse.bookie.test",
        service_port: 8443,
        socket_name: None,
        checks_hostname: true,
    },
    EngineCase {
        driver: &MssqlDriver,
        options: DriverTlsFixture::mssql,
        service_host: "mssql-ssh.bookie.test",
        service_port: 1433,
        socket_name: None,
        checks_hostname: true,
    },
];

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn verify_ca_hostname_behavior_by_ip_matches_each_tls_engine() {
    let fixture = DriverTlsFixture::from_env();
    let wrong_host = std::env::var("BOOKIE_DRIVER_TLS_WRONG_IDENTITY_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    for engine in ENGINES {
        let accepted = (engine.options)(&fixture, TlsMode::VerifyCa, Some(fixture.ca_cert.clone()));
        let mut mismatched = accepted.clone();
        mismatched.host = wrong_host.clone();
        assert_verify_ca_hostname_behavior(engine.driver, accepted, mismatched, engine.checks_hostname).await;
    }
}

#[tokio::test]
#[ignore = "requires the driver tls fixture"]
async fn verify_ca_hostname_behavior_through_ssh_matches_each_tls_engine() {
    let fixture = DriverTlsFixture::from_env();
    let wrong_host = std::env::var("BOOKIE_DRIVER_TLS_WRONG_IDENTITY_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    for engine in ENGINES {
        let tunnel = fixture
            .open_service_tunnel(engine.service_host, engine.service_port, engine.socket_name)
            .await;
        let base = (engine.options)(&fixture, TlsMode::VerifyCa, Some(fixture.ca_cert.clone()));
        let accepted = DriverTlsFixture::tunnel_options(
            base,
            engine.service_host,
            engine.service_port,
            &tunnel,
            engine.socket_name,
        );
        let mut mismatched = accepted.clone();
        mismatched.service_endpoint = Some((wrong_host.clone(), engine.service_port));
        assert_verify_ca_hostname_behavior(engine.driver, accepted, mismatched, engine.checks_hostname).await;
    }
}
