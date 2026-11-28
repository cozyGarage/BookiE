use bookie_core::ConnectOptions;

use crate::server::{Dialect, Referee, Server, SessionVariant, SettingCheck, SqlFuture, setting_parts};

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
    session_variant,
    cast_query,
};

fn session_variant(_table: &str, settings: &[String]) -> SessionVariant {
    SessionVariant {
        setup: Vec::new(),
        teardown: Vec::new(),
        login: None,
        bookie_setup: settings.iter().map(|setting| format!("SET {setting}")).collect(),
        checks: settings
            .iter()
            .map(|setting| {
                let (name, expected) = setting_parts(setting);
                SettingCheck {
                    sql: format!("SELECT current_setting('{name}')"),
                    expected,
                    through_bookie: true,
                }
            })
            .collect(),
    }
}

fn cast_query(settings: &[String], cast: &str, render: &str) -> String {
    let set: String = settings.iter().map(|setting| format!("SET {setting}; ")).collect();
    format!("{set}SELECT {render} FROM (SELECT {cast} AS v) AS cast_back")
}

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
    path: std::path::PathBuf,
    connection: std::sync::Mutex<Option<duckdb::Connection>>,
    _directory: tempfile::TempDir,
}

impl Referee for DuckdbReferee {
    fn run<'a>(&'a self, sql: &'a str) -> SqlFuture<'a> {
        let result = self
            .connection
            .lock()
            .map_err(|error| error.to_string())
            .and_then(|mut connection| {
                if connection.is_none() {
                    *connection = Some(duckdb::Connection::open(&self.path).map_err(|error| error.to_string())?);
                }
                connection
                    .as_ref()
                    .expect("connection opened")
                    .try_clone()
                    .map_err(|error| error.to_string())
            })
            .and_then(|connection| run(&connection, sql));
        Box::pin(async move { result })
    }

    fn release(&self) {
        self.connection.lock().unwrap_or_else(|error| error.into_inner()).take();
    }
}

fn run(connection: &duckdb::Connection, sql: &str) -> Result<String, String> {
    let (prefix, last) = match sql.rfind("; SELECT ") {
        Some(at) => (&sql[..at], &sql[at + 2..]),
        None => ("", sql),
    };
    if !prefix.is_empty() {
        connection.execute_batch(prefix).map_err(|error| error.to_string())?;
    }
    if !last.trim_start().to_ascii_uppercase().starts_with("SELECT") {
        connection.execute_batch(last).map_err(|error| error.to_string())?;
        return Ok(String::new());
    }
    let mut statement = connection.prepare(last).map_err(|error| error.to_string())?;
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
            connection: std::sync::Mutex::new(None),
            _directory: directory,
        }),
    }
}
