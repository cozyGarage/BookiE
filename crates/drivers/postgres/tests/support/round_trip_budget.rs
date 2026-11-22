use bookie_core::{ConnectOptions, Connection, DatabaseDriver, TlsConfig, Value};
use drivers_postgres::PgDriver;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const APP_NAME: &str = "bookie_round_trip_budget";
const SIMPLE_SELECT_STATEMENT_BUDGET: i64 = 3;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_simple_select_stays_within_its_server_statement_budget() {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .with_cmd([
            "postgres",
            "-c",
            "shared_preload_libraries=pg_stat_statements",
            "-c",
            "pg_stat_statements.track=all",
        ])
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .expect("start PostgreSQL with statement statistics");
    let host = container.get_host().await.expect("container host").to_string();
    let port = container.get_host_port_ipv4(5432).await.expect("PostgreSQL port");
    let admin_options = ConnectOptions {
        host,
        port,
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let admin = PgDriver
        .connect(admin_options.clone())
        .await
        .expect("connect as fixture admin");
    admin
        .execute("CREATE EXTENSION pg_stat_statements")
        .await
        .expect("enable statement statistics");
    admin
        .execute("CREATE ROLE bookie_budget LOGIN PASSWORD 'bookie_budget'")
        .await
        .expect("create isolated app role");
    let app: Box<dyn Connection> = PgDriver
        .connect(ConnectOptions {
            username: "bookie_budget".into(),
            password: secrecy::SecretString::new("bookie_budget".to_string().into()),
            application_name: Some(APP_NAME.into()),
            ..admin_options
        })
        .await
        .expect("connect as measured app role");
    app.query("SELECT 0::bigint AS warmup")
        .await
        .expect("warm the app pool");
    admin
        .query("SELECT pg_stat_statements_reset()")
        .await
        .expect("clear setup and warmup statements");
    assert_eq!(
        app.query("SELECT 42::bigint AS value")
            .await
            .expect("simple SELECT")
            .rows,
        vec![vec![Value::Int(42)]]
    );

    let observed = admin
        .query(
            "SELECT query, calls::bigint FROM pg_stat_statements \
             WHERE userid = (SELECT oid FROM pg_roles WHERE rolname = 'bookie_budget') \
             ORDER BY query",
        )
        .await
        .expect("read observed app statements");
    let statements = observed
        .rows
        .iter()
        .map(|row| {
            let query = match row.first() {
                Some(Value::Text(query)) => query.as_str(),
                _ => "<unreadable SQL>",
            };
            let calls = match row.get(1) {
                Some(Value::Int(calls)) => *calls,
                _ => -1,
            };
            format!("{calls} × {query}")
        })
        .collect::<Vec<_>>();
    let total_calls = observed
        .rows
        .iter()
        .filter_map(|row| match row.get(1) {
            Some(Value::Int(calls)) => Some(*calls),
            _ => None,
        })
        .sum::<i64>();
    assert!(
        total_calls <= SIMPLE_SELECT_STATEMENT_BUDGET,
        "simple SELECT used {total_calls} server statements (budget {SIMPLE_SELECT_STATEMENT_BUDGET}):\n{}",
        statements.join("\n")
    );
    assert!(
        statements
            .iter()
            .any(|statement| statement.contains("SELECT") && statement.contains("AS value")),
        "the server trace omitted the requested query: {statements:#?}"
    );
}
