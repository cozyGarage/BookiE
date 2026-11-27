use std::time::Duration;

use bookie_core::{ConnectOptions, TlsConfig};
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

use crate::server::{Cli, ContainerCli, Dialect, Server, exec, labelled};

pub const DATABASE: &str = "integrity";

pub static DIALECT: Dialect = Dialect {
    driver_id: "postgres",
    create_table: "CREATE TABLE {table} (id integer PRIMARY KEY, v {type})",
    quote_text,
    text_expr: "CASE WHEN {v} IS NULL THEN NULL ELSE format('%s', {v}) END",
    hex_expr: "encode({v}, 'hex')",
    epoch_expr: "extract(epoch from {v})::text",
    type_expr: "pg_typeof(v)::text",
    wrap_hex,
    join,
    param_select: "SELECT $1::{type}",
    param_where: "SELECT v FROM {table} WHERE id = $1",
    drop_table: "DROP TABLE IF EXISTS {table} CASCADE",
    schema: Some("public"),
    epoch_in_nanos: false,
};

pub fn quote_text(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn wrap_hex(expression: &str) -> String {
    format!("coalesce(encode(convert_to(({expression})::text, 'UTF8'), 'hex'), 'N')")
}

fn join(fields: &[String]) -> String {
    format!("concat_ws(':', {})", fields.join(", "))
}

struct Psql {
    database: &'static str,
}

impl Cli for Psql {
    fn command(&self, sql: &str) -> Vec<String> {
        [
            "psql",
            "-X",
            "-q",
            "-v",
            "ON_ERROR_STOP=1",
            "-A",
            "-t",
            "-U",
            "postgres",
            "-d",
            self.database,
            "-c",
            sql,
        ]
        .map(String::from)
        .to_vec()
    }
}

pub async fn start(tag: &str, label: &str) -> Server {
    let image = Postgres::default().with_tag(tag).with_cmd([
        "postgres",
        "-c",
        "fsync=off",
        "-c",
        "synchronous_commit=off",
        "-c",
        "full_page_writes=off",
    ]);
    let container = labelled(image)
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("start postgres container");
    let create = Psql { database: "postgres" }.command(&format!(
        "CREATE DATABASE {DATABASE} TEMPLATE template0 ENCODING 'UTF8' LOCALE 'C'"
    ));
    exec(&container, create).await.expect("create database");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: DATABASE.into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    Server {
        label: label.to_string(),
        dialect: &DIALECT,
        options,
        referee: Box::new(ContainerCli {
            container,
            cli: Box::new(Psql { database: DATABASE }),
        }),
    }
}
