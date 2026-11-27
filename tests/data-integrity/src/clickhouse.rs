use std::time::Duration;

use bookie_core::{ConnectOptions, TlsConfig};
use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

use crate::server::{Cli, ContainerCli, Dialect, Server, labelled};

const PASSWORD: &str = "bookie";

pub static DIALECT: Dialect = Dialect {
    driver_id: "clickhouse",
    create_table: "CREATE TABLE {table} (id Int32, v {type}) ENGINE = MergeTree ORDER BY id",
    quote_text,
    text_expr: "toString({v})",
    hex_expr: "hex({v})",
    epoch_expr: "toUnixTimestamp64Nano(toDateTime64({v}, 9))",
    type_expr: "toTypeName(v)",
    wrap_hex,
    join,
    param_select: "SELECT CAST(? AS {type})",
    param_where: "SELECT v FROM {table} WHERE id = ?",
    drop_table: "DROP TABLE IF EXISTS {table}",
    schema: Some("default"),
    epoch_in_nanos: true,
};

pub fn quote_text(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('\'', "\\'").replace('\0', "\\0");
    format!("'{escaped}'")
}

fn wrap_hex(expression: &str) -> String {
    format!("if(isNull({expression}), 'N', hex(assumeNotNull(toString({expression}))))")
}

fn join(fields: &[String]) -> String {
    format!("concatWithSeparator(':', {})", fields.join(", "))
}

struct ClickhouseClient;

impl Cli for ClickhouseClient {
    fn command(&self, sql: &str) -> Vec<String> {
        [
            "clickhouse-client",
            "--password",
            PASSWORD,
            "--multiquery",
            "--format",
            "TSVRaw",
            "-q",
            sql,
        ]
        .map(String::from)
        .to_vec()
    }
}

pub async fn start(label: &str) -> Server {
    let image = GenericImage::new("clickhouse/clickhouse-server", "24.8")
        .with_exposed_port(8123.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/ping")
                .with_port(8123.tcp())
                .with_expected_status_code(200u16),
        ))
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", PASSWORD)
        .with_env_var("CLICKHOUSE_DB", "default")
        .with_env_var("CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT", "1");
    let container = labelled(image)
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("start clickhouse container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(8123).await.expect("port"),
        database: "default".into(),
        username: "default".into(),
        password: secrecy::SecretString::new(PASSWORD.to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let server = Server {
        label: label.to_string(),
        dialect: &DIALECT,
        options,
        referee: Box::new(ContainerCli {
            container,
            cli: Box::new(ClickhouseClient),
        }),
    };
    wait_until_queryable(&server).await;
    server
}

async fn wait_until_queryable(server: &Server) {
    let mut last_error = String::new();
    for _ in 0..120 {
        match server.run_sql("SELECT 1").await {
            Ok(_) => return,
            Err(error) => last_error = error,
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("clickhouse never answered SELECT 1: {last_error}");
}
