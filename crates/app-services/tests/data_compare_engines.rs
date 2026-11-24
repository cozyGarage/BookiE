#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use bookie_app_services::data_compare::{DataCompare, TableSide, compare_tables};
use bookie_core::sql_literal::render_sql_literal;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, OperationControl, TlsConfig, Value};
use bookie_policy::{GuardContext, NullAuditSink, PolicyConfig, PolicyGuard, Principal};
use secrecy::SecretString;
use testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;

const ROWS: i64 = 2_500;
const PREFIXES: [&str; 5] = ["a", "B", "\u{e9}", "Z", "_"];

struct Engine {
    driver_id: &'static str,
    schema: Option<&'static str>,
    create: &'static str,
}

fn guarded(driver_id: &str, connection: Arc<dyn Connection>) -> PolicyGuard {
    PolicyGuard::new(
        connection,
        GuardContext {
            connection_id: uuid::Uuid::new_v4(),
            connection_name: "data compare".into(),
            driver_id: driver_id.into(),
            environment: bookie_core::Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(bookie_policy::AutoApproveSink),
            audit: Arc::new(NullAuditSink),
            audit_state: Arc::new(bookie_policy::AuditState::new()),
        },
    )
}

fn source_row(i: i64) -> Vec<Value> {
    let note = if i % 7 == 0 {
        Value::Null
    } else {
        Value::Text(format!("it's \\ {i}"))
    };
    vec![
        Value::Text(format!("{}{}", PREFIXES[(i % 5) as usize], i % 13)),
        Value::Int(i),
        note,
        Value::Float(i as f64 / 2.0),
    ]
}

fn target_rows() -> Vec<Vec<Value>> {
    let mut rows: Vec<Vec<Value>> = (0..ROWS)
        .filter(|i| i % 50 != 0)
        .map(|i| {
            let mut row = source_row(i);
            if i % 40 == 1 {
                row[2] = Value::Text("changed".into());
            }
            row
        })
        .collect();
    rows.extend((0..20).map(|n| vec![Value::Text("zz".into()), Value::Int(n), Value::Null, Value::Float(0.0)]));
    rows
}

async fn insert(driver_id: &str, connection: &dyn Connection, table: &str, rows: &[Vec<Value>]) {
    for chunk in rows.chunks(500) {
        let values = chunk
            .iter()
            .map(|row| {
                let cells: Vec<String> = row
                    .iter()
                    .map(|value| render_sql_literal(driver_id, value).unwrap())
                    .collect();
                format!("({})", cells.join(", "))
            })
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("INSERT INTO {table} (code, n, note, price) VALUES {values}");
        connection.execute(&sql).await.unwrap();
    }
}

async fn compare(engine: &Engine, connection: &dyn Connection) -> DataCompare {
    let control = OperationControl::new(CancellationToken::new(), None);
    let side = |table| TableSide {
        connection,
        schema: engine.schema,
        table,
    };
    compare_tables(engine.driver_id, side("src"), side("dst"), None, &control)
        .await
        .unwrap()
}

async fn run_script(engine: &Engine, guard: &PolicyGuard, script: &str) {
    for statement in bookie_core::sql_lex::split_statements(script, engine.driver_id) {
        if let Err(error) = guard.execute(&statement).await {
            panic!("running {statement:?} failed: {error:?}");
        }
    }
}

async fn round_trip(engine: Engine, connection: Box<dyn Connection>) {
    let connection: Arc<dyn Connection> = Arc::from(connection);
    for table in ["src", "dst"] {
        connection
            .execute(&engine.create.replace("{table}", table))
            .await
            .unwrap();
    }
    let source: Vec<_> = (0..ROWS).map(source_row).collect();
    insert(engine.driver_id, connection.as_ref(), "src", &source).await;
    insert(engine.driver_id, connection.as_ref(), "dst", &target_rows()).await;

    let before = compare(&engine, connection.as_ref()).await;
    assert_eq!(before.layout.key, ["code", "n"]);
    assert_eq!(before.counts.insert, 50);
    assert_eq!(before.counts.update, 63);
    assert_eq!(before.counts.delete, 20);
    assert_eq!(before.counts.same, (ROWS - 50 - 63) as u64);

    let guard = guarded(engine.driver_id, connection.clone());
    let without_deletes = before.script(engine.driver_id, engine.schema, "dst", false).unwrap();
    run_script(&engine, &guard, &without_deletes).await;
    let middle = compare(&engine, connection.as_ref()).await;
    assert_eq!(
        (middle.counts.insert, middle.counts.update, middle.counts.delete),
        (0, 0, 20)
    );

    let with_deletes = middle.script(engine.driver_id, engine.schema, "dst", true).unwrap();
    run_script(&engine, &guard, &with_deletes).await;
    let after = compare(&engine, connection.as_ref()).await;
    assert!(after.is_identical(), "{:?}", after.counts);
    assert_eq!(after.counts.same, ROWS as u64);
}

#[tokio::test]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test data_compare_engines -- --include-ignored"]
async fn postgres_round_trip_makes_the_tables_identical() {
    let container = testcontainers_modules::postgres::Postgres::default()
        .with_tag("18-alpine")
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
    let engine = Engine {
        driver_id: "postgres",
        schema: Some("public"),
        create: "CREATE TABLE {table} (code varchar(20), n int, note text, price double precision, PRIMARY KEY (code, n))",
    };
    round_trip(engine, connection).await;
}

#[tokio::test]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test data_compare_engines -- --include-ignored"]
async fn mysql_round_trip_makes_the_tables_identical() {
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
    let engine = Engine {
        driver_id: "mysql",
        schema: Some("test"),
        create: "CREATE TABLE {table} (code VARCHAR(20), n INT, note TEXT, price DOUBLE, PRIMARY KEY (code, n))",
    };
    round_trip(engine, connection).await;
}

#[tokio::test]
async fn sqlite_round_trip_makes_the_tables_identical() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("compare.db");
    std::fs::File::create(&path).unwrap();
    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: path.to_string_lossy().into_owned(),
            ..ConnectOptions::default()
        })
        .await
        .unwrap();
    let engine = Engine {
        driver_id: "sqlite",
        schema: None,
        create: "CREATE TABLE {table} (code TEXT NOT NULL, n INTEGER NOT NULL, note TEXT, price REAL, PRIMARY KEY (code, n))",
    };
    round_trip(engine, connection).await;
}

#[tokio::test]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test data_compare_engines -- --include-ignored"]
async fn sql_server_round_trip_makes_the_tables_identical() {
    let container = testcontainers_modules::mssql_server::MssqlServer::default()
        .with_accept_eula()
        .with_tag("2022-CU23-ubuntu-22.04")
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .unwrap();
    let options = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(1433).await.unwrap(),
        database: "master".into(),
        username: "sa".into(),
        password: SecretString::new(
            testcontainers_modules::mssql_server::MssqlServer::DEFAULT_SA_PASSWORD
                .to_string()
                .into(),
        ),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let mut connection = None;
    for _ in 0..60 {
        if let Ok(opened) = drivers_mssql::MssqlDriver.connect(options.clone()).await
            && opened.query("SELECT 1").await.is_ok()
        {
            connection = Some(opened);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    let engine = Engine {
        driver_id: "mssql",
        schema: Some("dbo"),
        create: "CREATE TABLE {table} (code NVARCHAR(20) NOT NULL, n INT NOT NULL, note NVARCHAR(200), price FLOAT, PRIMARY KEY (code, n))",
    };
    round_trip(engine, connection.expect("SQL Server accepts a login")).await;
}
