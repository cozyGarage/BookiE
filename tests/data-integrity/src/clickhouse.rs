use std::time::Duration;

use bookie_core::{ConnectOptions, TlsConfig};
use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

use crate::server::{Cli, ContainerCli, Dialect, Server, SessionVariant, SettingCheck, labelled, setting_parts};

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
    session_variant,
    cast_query,
};

const ROLE_PASSWORD: &str = "integrity";

fn session_variant(role: &str, settings: &[String]) -> SessionVariant {
    let profile = settings.join(", ");
    SessionVariant {
        setup: vec![
            format!("DROP USER IF EXISTS {role}"),
            format!("CREATE USER {role} IDENTIFIED WITH plaintext_password BY '{ROLE_PASSWORD}' SETTINGS {profile}"),
            format!("GRANT SELECT ON default.* TO {role}"),
        ],
        teardown: vec![format!("DROP USER IF EXISTS {role}")],
        login: Some((role.to_string(), ROLE_PASSWORD.to_string())),
        bookie_setup: Vec::new(),
        checks: settings
            .iter()
            .map(|setting| {
                let (name, expected) = setting_parts(setting);
                SettingCheck {
                    sql: format!("SELECT getSetting('{name}')"),
                    expected,
                    through_bookie: true,
                }
            })
            .collect(),
    }
}

fn cast_query(settings: &[String], cast: &str, render: &str) -> String {
    let query = format!("SELECT {render} FROM (SELECT {cast} AS v)");
    if settings.is_empty() {
        return query;
    }
    format!("{query} SETTINGS {}", settings.join(", "))
}

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
            "--output_format_json_quote_64bit_integers=0",
            "--format",
            "TSVRaw",
            "-q",
            sql,
        ]
        .map(String::from)
        .to_vec()
    }
}

pub async fn start(label: &str, time_zone: Option<&str>) -> Server {
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
        .with_env_var("CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT", "1")
        .with_env_var("TZ", time_zone.unwrap_or("UTC"));
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
    let zone = server.run_sql("SELECT timezone()").await.expect("server time zone");
    assert_eq!(zone.trim(), time_zone.unwrap_or("UTC"), "clickhouse server time zone");
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
