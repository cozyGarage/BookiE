#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use secrecy::SecretString;
use tablepro_core::{ConnectOptions, TlsConfig};
use tablepro_dump::{ClientTool, DumpError, DumpFormat, PgDumpRequest, pg_dump};
use testcontainers::core::{CmdWaitFor, ExecCommand};
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;

async fn seeded_postgres() -> (ContainerAsync<Postgres>, ConnectOptions) {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("start postgres container");
    let seed = "CREATE TABLE items (id int PRIMARY KEY, name text); \
                INSERT INTO items VALUES (1, 'plain'), (2, 'colon: and ''quote''');";
    container
        .exec(
            ExecCommand::new(["psql", "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-c", seed])
                .with_cmd_ready_condition(CmdWaitFor::exit_code(0)),
        )
        .await
        .expect("seed");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: SecretString::from("postgres"),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    (container, options)
}

async fn pg_dump_tool() -> ClientTool {
    let configured = std::env::var_os("TABLEPRO_TEST_PG_DUMP").map(std::path::PathBuf::from);
    ClientTool::locate("pg_dump", configured.as_deref())
        .await
        .expect("pg_dump from TABLEPRO_TEST_PG_DUMP or the host")
}

fn request<'a>(options: &'a ConnectOptions, output: &'a std::path::Path) -> PgDumpRequest<'a> {
    PgDumpRequest {
        options,
        output,
        format: DumpFormat::Plain,
        server_major: 16,
    }
}

#[tokio::test]
#[ignore = "requires Docker; run scripts/test-pg-dump.sh"]
async fn pg_dump_writes_the_schema_and_rows_of_a_real_database() {
    let (_container, options) = seeded_postgres().await;
    let tool = pg_dump_tool().await;
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("postgres.sql");

    pg_dump(&tool, request(&options, &output), dir.path(), CancellationToken::new())
        .await
        .expect("dump");

    let dump = std::fs::read_to_string(&output).unwrap();
    assert!(dump.contains("CREATE TABLE public.items"), "{dump}");
    assert!(dump.contains("colon: and 'quote'"), "{dump}");
}

#[tokio::test]
#[ignore = "requires Docker; run scripts/test-pg-dump.sh"]
async fn a_wrong_password_fails_without_leaving_a_file() {
    let (_container, mut options) = seeded_postgres().await;
    options.password = SecretString::from("wrong");
    let tool = pg_dump_tool().await;
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("postgres.sql");

    let error = pg_dump(&tool, request(&options, &output), dir.path(), CancellationToken::new())
        .await
        .unwrap_err();

    let DumpError::Failed { stderr, .. } = error else {
        panic!("unexpected error: {error:?}");
    };
    assert!(!stderr.contains("wrong"), "{stderr}");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
