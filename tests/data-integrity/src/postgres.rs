use std::time::Duration;

use bookie_core::{ConnectOptions, TlsConfig};
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

use crate::server::{Cli, ContainerCli, Dialect, Server, SessionVariant, SettingCheck, exec, labelled, setting_parts};

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
    session_variant,
    cast_query,
};

const ROLE_PASSWORD: &str = "integrity";

fn session_variant(role: &str, settings: &[String]) -> SessionVariant {
    let mut setup = vec![
        format!("CREATE ROLE {role} LOGIN PASSWORD '{ROLE_PASSWORD}'"),
        format!("GRANT pg_read_all_data TO {role}"),
    ];
    setup.extend(
        settings
            .iter()
            .map(|setting| format!("ALTER ROLE {role} SET {setting}")),
    );
    let checks = settings
        .iter()
        .map(|setting| {
            let (name, expected) = setting_parts(setting);
            SettingCheck {
                sql: format!("SELECT current_setting('{name}')"),
                expected,
                through_bookie: true,
            }
        })
        .collect();
    SessionVariant {
        setup,
        teardown: Vec::new(),
        login: Some((role.to_string(), ROLE_PASSWORD.to_string())),
        bookie_setup: Vec::new(),
        checks,
    }
}

fn cast_query(settings: &[String], cast: &str, render: &str) -> String {
    if settings.is_empty() {
        return format!("SELECT {render} FROM (SELECT {cast} AS v) AS cast_back");
    }
    let set: String = settings.iter().map(|setting| format!("SET {setting}; ")).collect();
    format!("{set}CREATE TEMP TABLE cast_back AS SELECT {cast} AS v; RESET ALL; SELECT {render} FROM cast_back")
}

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
