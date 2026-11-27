use std::future::Future;
use std::pin::Pin;

use bookie_core::ConnectOptions;
use testcontainers::core::ExecCommand;
use testcontainers::{ContainerAsync, ContainerRequest, Image, ImageExt};

pub struct Dialect {
    pub driver_id: &'static str,
    pub create_table: &'static str,
    pub quote_text: fn(&str) -> String,
    pub text_expr: &'static str,
    pub hex_expr: &'static str,
    pub epoch_expr: &'static str,
    pub type_expr: &'static str,
    pub wrap_hex: fn(&str) -> String,
    pub join: fn(&[String]) -> String,
    pub param_select: &'static str,
    pub param_where: &'static str,
    pub drop_table: &'static str,
    pub schema: Option<&'static str>,
    pub epoch_in_nanos: bool,
    pub session_variant: fn(&str, &[String]) -> SessionVariant,
    pub cast_query: fn(&[String], &str, &str) -> String,
}

pub struct SessionVariant {
    pub setup: Vec<String>,
    pub teardown: Vec<String>,
    pub login: Option<(String, String)>,
    pub bookie_setup: Vec<String>,
    pub checks: Vec<SettingCheck>,
}

pub struct SettingCheck {
    pub sql: String,
    pub expected: String,
    pub through_bookie: bool,
}

pub fn setting_parts(setting: &str) -> (&str, String) {
    let (name, value) = setting.split_once('=').unwrap_or((setting, ""));
    (name.trim(), value.trim().trim_matches('\'').to_string())
}

impl Dialect {
    pub fn fill(template: &str, table: &str, column_type: &str) -> String {
        template.replace("{table}", table).replace("{type}", column_type)
    }

    pub fn on(template: &str, column: &str) -> String {
        template.replace("{v}", column)
    }
}

pub type SqlFuture<'a> = Pin<Box<dyn Future<Output = Result<String, String>> + 'a>>;

pub trait Referee: Send + Sync {
    fn run<'a>(&'a self, sql: &'a str) -> SqlFuture<'a>;
}

pub trait Cli: Send + Sync {
    fn command(&self, sql: &str) -> Vec<String>;
}

pub struct ContainerCli<I: Image> {
    pub container: ContainerAsync<I>,
    pub cli: Box<dyn Cli>,
}

impl<I: Image> Referee for ContainerCli<I> {
    fn run<'a>(&'a self, sql: &'a str) -> SqlFuture<'a> {
        Box::pin(exec(&self.container, self.cli.command(sql)))
    }
}

pub struct Server {
    pub label: String,
    pub dialect: &'static Dialect,
    pub options: ConnectOptions,
    pub referee: Box<dyn Referee>,
}

impl Server {
    pub async fn run_sql(&self, sql: &str) -> Result<String, String> {
        self.referee.run(sql).await
    }
}

pub async fn exec<I: Image>(container: &ContainerAsync<I>, argv: Vec<String>) -> Result<String, String> {
    let mut result = container
        .exec(ExecCommand::new(argv))
        .await
        .map_err(|error| format!("exec failed: {error}"))?;
    let stdout = result.stdout_to_vec().await.map_err(|error| error.to_string())?;
    let stderr = result.stderr_to_vec().await.map_err(|error| error.to_string())?;
    let code = result.exit_code().await.map_err(|error| error.to_string())?;
    if code != Some(0) {
        return Err(format!("exit {code:?}: {}", String::from_utf8_lossy(&stderr).trim()));
    }
    String::from_utf8(stdout).map_err(|error| format!("client output is not UTF-8: {error}"))
}

pub fn labelled<I: Image>(image: ContainerRequest<I>) -> ContainerRequest<I> {
    match std::env::var("BOOKIE_TEST_RUN_ID") {
        Ok(run_id) => image.with_label("com.bookie.test-run", run_id),
        Err(_) => image,
    }
}
