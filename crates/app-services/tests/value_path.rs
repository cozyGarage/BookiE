#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "value_path/accounting.rs"]
mod accounting;
#[path = "value_path/runner.rs"]
mod runner;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Environment, TlsConfig};
use bookie_policy::{
    AuditError, AuditEvent, AuditSink, AuditState, AutoApproveSink, GuardContext, PolicyConfig, PolicyGuard, Principal,
};
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use uuid::Uuid;

use accounting::{Case, Key, TypeCases, account};
use runner::{Engine, load_corpus, load_known_issues, matrix, run_engine};

#[derive(Default)]
struct MemoryAudit(Mutex<Vec<AuditEvent>>);

#[async_trait]
impl AuditSink for MemoryAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.0.lock().unwrap().push(event);
        Ok(())
    }
}

fn guarded(driver_id: &str, connection: Box<dyn Connection>) -> Arc<dyn Connection> {
    Arc::new(PolicyGuard::new(
        Arc::from(connection),
        GuardContext {
            connection_id: Uuid::new_v4(),
            connection_name: "type corpus".into(),
            driver_id: driver_id.into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit: Arc::new(MemoryAudit::default()),
            audit_state: Arc::new(AuditState::new()),
        },
    ))
}

async fn sqlite(directory: &tempfile::TempDir) -> Box<dyn Connection> {
    let path = directory.path().join("corpus.db");
    std::fs::File::create(&path).unwrap();
    drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: path.to_string_lossy().into_owned(),
            ..Default::default()
        })
        .await
        .unwrap()
}

async fn assert_engine(id: &str, driver: &dyn DatabaseDriver, connection: Box<dyn Connection>) -> String {
    let corpus = load_corpus(id);
    let engine = Engine {
        id,
        driver,
        connection: guarded(id, connection),
    };
    let report = run_engine(&engine, &corpus).await;
    let known: BTreeMap<Key, u64> = load_known_issues()
        .into_iter()
        .filter(|(key, _)| key.engine == id)
        .collect();
    let mut log = matrix(id, &corpus, &report, &known);
    match account(&known, &report.failures) {
        Ok(lines) => lines.iter().for_each(|line| log.push_str(&format!("known: {line}\n"))),
        Err(error) => panic!("{id} type corpus: {error}\n{log}{}", report.details.join("\n")),
    }
    log
}

#[tokio::test(flavor = "multi_thread")]
async fn sqlite_type_corpus_survives_the_row_path() {
    let directory = tempfile::tempdir().unwrap();
    eprintln!(
        "{}",
        assert_engine("sqlite", &drivers_sqlite::SqliteDriver, sqlite(&directory).await).await
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires docker"]
async fn postgres_type_corpus_survives_the_row_path() {
    let container = testcontainers_modules::postgres::Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .unwrap();
    let connection = drivers_postgres::PgDriver
        .connect(ConnectOptions {
            host: container.get_host().await.unwrap().to_string(),
            port: container.get_host_port_ipv4(5432).await.unwrap(),
            database: "postgres".into(),
            username: "postgres".into(),
            password: SecretString::from("postgres"),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();
    eprintln!(
        "{}",
        assert_engine("postgres", &drivers_postgres::PgDriver, connection).await
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires docker"]
async fn mysql_type_corpus_survives_the_row_path() {
    let container = testcontainers_modules::mysql::Mysql::default()
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
    let connection = drivers_mysql::MysqlDriver
        .connect(ConnectOptions {
            host: container.get_host().await.unwrap().to_string(),
            port: container.get_host_port_ipv4(3306).await.unwrap(),
            database: "test".into(),
            username: "root".into(),
            password: SecretString::from("bookie_test"),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();
    eprintln!(
        "{}",
        assert_engine("mysql", &drivers_mysql::MysqlDriver, connection).await
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_runner_reports_a_wrong_cell_and_passes_a_right_one() {
    let directory = tempfile::tempdir().unwrap();
    let engine = Engine {
        id: "sqlite",
        driver: &drivers_sqlite::SqliteDriver,
        connection: guarded("sqlite", sqlite(&directory).await),
    };
    let case = |literal: &str, cell: &str| Case {
        literal: literal.into(),
        cell: cell.into(),
    };
    let corpus = [
        TypeCases {
            data_type: "TEXT".into(),
            contract: None,
            cases: vec![case("'right'", "right")],
        },
        TypeCases {
            data_type: "INTEGER".into(),
            contract: None,
            cases: vec![case("7", "8")],
        },
    ];

    let failures = run_engine(&engine, &corpus).await.failures;

    let wrong = Key {
        engine: "sqlite".into(),
        data_type: "INTEGER".into(),
        step: "browse".into(),
        signature: "cell-mismatch".into(),
    };
    assert_eq!(failures.into_iter().collect::<Vec<_>>(), [wrong]);
}

#[test]
fn every_corpus_file_loads_with_its_known_issues() {
    for engine in ["sqlite", "postgres", "mysql"] {
        assert!(!load_corpus(engine).is_empty(), "{engine}");
    }
    load_known_issues();
}
