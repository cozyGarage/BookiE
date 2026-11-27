use std::path::{Path, PathBuf};

use bookie_core::ConnectOptions;

use crate::server::{Dialect, Referee, Server, SqlFuture};

pub static DIALECT: Dialect = Dialect {
    driver_id: "duckdb",
    create_table: "CREATE TABLE {table} (id INTEGER PRIMARY KEY, v {type})",
    quote_text,
    text_expr: "CAST({v} AS VARCHAR)",
    hex_expr: "hex({v})",
    epoch_expr: "epoch_ns({v})",
    type_expr: "typeof(v)",
    wrap_hex,
    join,
    param_select: "SELECT CAST(? AS {type})",
    param_where: "SELECT v FROM {table} WHERE id = ?",
    drop_table: "DROP TABLE IF EXISTS {table}",
    schema: None,
    epoch_in_nanos: true,
};

pub fn quote_text(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn wrap_hex(expression: &str) -> String {
    format!("coalesce(hex(CAST(({expression}) AS VARCHAR)), 'N')")
}

fn join(fields: &[String]) -> String {
    format!("concat_ws(':', {})", fields.join(", "))
}

struct DuckdbReferee {
    path: PathBuf,
    _directory: tempfile::TempDir,
}

impl Referee for DuckdbReferee {
    fn run<'a>(&'a self, sql: &'a str) -> SqlFuture<'a> {
        let result = run(&self.path, sql);
        Box::pin(async move { result })
    }
}

fn run(path: &Path, sql: &str) -> Result<String, String> {
    let connection = duckdb::Connection::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    if !sql.trim_start().to_ascii_uppercase().starts_with("SELECT") {
        connection.execute_batch(sql).map_err(|error| error.to_string())?;
        return Ok(String::new());
    }
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let lines = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(lines.join("\n"))
}

pub fn start(label: &str) -> Server {
    let directory = tempfile::TempDir::new().expect("duckdb directory");
    let path = directory.path().join("integrity.duckdb");
    let options = ConnectOptions {
        database: path.to_string_lossy().into_owned(),
        ..Default::default()
    };
    Server {
        label: label.to_string(),
        dialect: &DIALECT,
        options,
        referee: Box::new(DuckdbReferee {
            path,
            _directory: directory,
        }),
    }
}
