use std::time::Duration;

use bookie_core::{ConnectOptions, TlsConfig};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, Image, ImageExt};
use testcontainers_modules::mysql::Mysql;

use crate::server::{Cli, Dialect, Server, SessionVariant, SettingCheck, labelled, setting_parts};

pub const DATABASE: &str = "integrity";
pub const MYSQL_TAG: &str = "8.4.13";
const PASSWORD: &str = "bookie_test";

pub static DIALECT: Dialect = Dialect {
    driver_id: "mysql",
    id_type: "INT",
    quote_text,
    text_expr: "CAST({v} AS CHAR)",
    hex_expr: "HEX({v})",
    epoch_expr: "UNIX_TIMESTAMP({v})",
    type_expr: "(SELECT COLUMN_TYPE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '{table}' AND COLUMN_NAME = 'v')",
    wrap_hex,
    join,
    param_select: "SELECT ?",
    param_where: "SELECT v FROM {table} WHERE id = ?",
    drop_table: "DROP TABLE IF EXISTS {table}",
    schema: None,
    session_variant,
    cast_query,
};

fn setting_name(setting: &str) -> &str {
    setting_parts(setting).0
}

fn session_variant(_table: &str, settings: &[String]) -> SessionVariant {
    SessionVariant {
        setup: settings.iter().map(|setting| format!("SET GLOBAL {setting}")).collect(),
        teardown: settings
            .iter()
            .map(|setting| format!("SET GLOBAL {} = DEFAULT", setting_name(setting)))
            .collect(),
        login: None,
        checks: settings
            .iter()
            .map(|setting| {
                let (name, expected) = setting_parts(setting);
                SettingCheck {
                    sql: format!("SELECT @@GLOBAL.{name}"),
                    expected,
                    through_bookie: false,
                }
            })
            .collect(),
    }
}

fn cast_query(settings: &[String], cast: &str, render: &str) -> String {
    if settings.is_empty() {
        return format!("SELECT {render} FROM (SELECT {cast} AS v) AS cast_back");
    }
    let set: String = settings
        .iter()
        .map(|setting| format!("SET SESSION {setting}; "))
        .collect();
    let reset: String = settings
        .iter()
        .map(|setting| format!("SET SESSION {} = DEFAULT; ", setting_name(setting)))
        .collect();
    format!("{set}CREATE TEMPORARY TABLE cast_back AS SELECT {cast} AS v; {reset}SELECT {render} FROM cast_back")
}

pub fn quote_text(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('\'', "''").replace('\0', "\\0");
    format!("'{escaped}'")
}

fn wrap_hex(expression: &str) -> String {
    format!("COALESCE(HEX(CONVERT(({expression}) USING utf8mb4)), 'N')")
}

fn join(fields: &[String]) -> String {
    format!("CONCAT_WS(':', {})", fields.join(", "))
}

struct MysqlClient {
    binary: &'static str,
}

impl Cli for MysqlClient {
    fn command(&self, sql: &str) -> Vec<String> {
        [
            self.binary,
            "--default-character-set=utf8mb4",
            "-uroot",
            "-pbookie_test",
            "-N",
            "-B",
            "-D",
            DATABASE,
            "-e",
            sql,
        ]
        .map(String::from)
        .to_vec()
    }
}

fn options(host: String, port: u16) -> ConnectOptions {
    ConnectOptions {
        host,
        port,
        database: DATABASE.into(),
        username: "root".into(),
        password: secrecy::SecretString::new(PASSWORD.to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    }
}

pub async fn start_mysql(label: &str) -> Server<Mysql> {
    let image = Mysql::default()
        .with_tag(MYSQL_TAG)
        .with_env_var("MYSQL_ROOT_PASSWORD", PASSWORD)
        .with_env_var("MYSQL_DATABASE", DATABASE)
        .with_cmd([
            "--mysql-native-password=ON",
            "--authentication-policy=mysql_native_password",
            "--innodb-flush-log-at-trx-commit=0",
            "--innodb-doublewrite=0",
            "--sync-binlog=0",
        ]);
    let container = labelled(image)
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("start mysql container");
    server(label, container, "mysql").await
}

pub async fn start_mariadb(label: &str) -> Server<GenericImage> {
    let image = GenericImage::new("mariadb", "11")
        .with_exposed_port(3306.tcp())
        .with_wait_for(WaitFor::message_on_stderr("port: 3306"))
        .with_env_var("MARIADB_ROOT_PASSWORD", PASSWORD)
        .with_env_var("MARIADB_DATABASE", DATABASE)
        .with_cmd([
            "mariadbd",
            "--innodb-flush-log-at-trx-commit=0",
            "--innodb-doublewrite=0",
            "--sync-binlog=0",
        ]);
    let container = labelled(image)
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("start mariadb container");
    server(label, container, "mariadb").await
}

async fn server<I: Image>(
    label: &str,
    container: testcontainers::ContainerAsync<I>,
    binary: &'static str,
) -> Server<I> {
    let host = container.get_host().await.expect("host").to_string();
    let port = container.get_host_port_ipv4(3306).await.expect("port");
    Server {
        label: label.to_string(),
        dialect: &DIALECT,
        options: options(host, port),
        container,
        cli: Box::new(MysqlClient { binary }),
    }
}
