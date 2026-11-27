use std::path::PathBuf;

use bookie_core::ConnectOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sqlx::{Connection as _, Row};

use crate::server::{Dialect, Referee, Server, SqlFuture};

pub static DIALECT: Dialect = Dialect {
    driver_id: "sqlite",
    create_table: "CREATE TABLE {table} (id INTEGER PRIMARY KEY, v {type})",
    quote_text,
    text_expr: "CASE typeof({v}) WHEN 'real' THEN printf('%!.17g', {v}) WHEN 'blob' THEN hex({v}) ELSE CAST({v} AS TEXT) END",
    hex_expr: "CASE WHEN {v} IS NULL THEN NULL ELSE hex({v}) END",
    epoch_expr: "unixepoch({v}, 'subsec')",
    type_expr: "typeof(v)",
    wrap_hex,
    join,
    param_select: "SELECT ?",
    param_where: "SELECT v FROM {table} WHERE id = ?",
    drop_table: "DROP TABLE IF EXISTS {table}",
    schema: None,
    epoch_in_nanos: false,
};

pub fn quote_text(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn wrap_hex(expression: &str) -> String {
    format!("CASE WHEN ({expression}) IS NULL THEN 'N' ELSE hex(CAST(({expression}) AS TEXT)) END")
}

fn join(fields: &[String]) -> String {
    fields.join(" || ':' || ")
}

struct SqliteReferee {
    path: PathBuf,
    _directory: tempfile::TempDir,
}

impl Referee for SqliteReferee {
    fn run<'a>(&'a self, sql: &'a str) -> SqlFuture<'a> {
        Box::pin(run(self.path.clone(), sql))
    }
}

async fn run(path: PathBuf, sql: &str) -> Result<String, String> {
    let options = SqliteConnectOptions::new().filename(&path).create_if_missing(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .map_err(|error| format!("open {}: {error}", path.display()))?;
    let rows = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_all(&mut connection)
        .await
        .map_err(|error| error.to_string());
    connection.close().await.map_err(|error| error.to_string())?;
    let lines = rows?
        .iter()
        .map(|row| row.try_get::<String, _>(0).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(lines.join("\n"))
}

pub fn start(label: &str) -> Server {
    let directory = tempfile::TempDir::new().expect("sqlite directory");
    let path = directory.path().join("integrity.sqlite");
    let options = ConnectOptions {
        database: path.to_string_lossy().into_owned(),
        ..Default::default()
    };
    Server {
        label: label.to_string(),
        dialect: &DIALECT,
        options,
        referee: Box::new(SqliteReferee {
            path,
            _directory: directory,
        }),
    }
}
