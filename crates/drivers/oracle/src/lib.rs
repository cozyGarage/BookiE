use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bookie_core::{
    ColumnInfo, ConnectOptions, Connection, DatabaseDriver, DriverError, DriverMaturity, ExecResult, MAX_QUERY_ROWS,
    QueryResult, QueryResultBudget, QueryRows, TableInfo, TlsMode, Transport, Value,
};
use oracledb::{Config, ErrorKind};
use secrecy::ExposeSecret;

mod catalog;
mod guard;
mod value;

pub struct OracleDriver;

#[async_trait]
impl DatabaseDriver for OracleDriver {
    fn id(&self) -> &'static str {
        "oracle"
    }

    fn display_name(&self) -> &'static str {
        "Oracle Database"
    }

    fn maturity(&self) -> DriverMaturity {
        DriverMaturity::Experimental
    }

    fn default_port(&self) -> u16 {
        1521
    }

    fn default_database(&self) -> &'static str {
        "FREEPDB1"
    }

    fn default_username(&self) -> &'static str {
        "system"
    }

    fn reports_rows_affected(&self) -> bool {
        false
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        let config = connect_config(&opts)?;
        probe_listener(&opts).await?;
        let connection = blocking(move || oracledb::connect(config).map_err(map_error)).await?;
        Ok(Box::new(OracleConnection {
            inner: Arc::new(connection),
        }))
    }
}

fn connect_config(opts: &ConnectOptions) -> Result<Config, DriverError> {
    if opts.tls.mode != TlsMode::Disabled {
        return Err(DriverError::Unsupported(
            "TLS for Oracle is not implemented yet; set TLS mode to Disabled on a trusted network".into(),
        ));
    }
    let Transport::Tcp { host, port } = opts.transport() else {
        return Err(DriverError::Unsupported("Oracle connects over TCP only".into()));
    };
    if opts.database.trim().is_empty() {
        return Err(DriverError::Unsupported(
            "Oracle needs a service name in the database field".into(),
        ));
    }
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let connect_string = format!("{host}:{port}/{}", opts.database.trim());
    Config::default()
        .set_credentials(&opts.username, opts.password.expose_secret())
        .set_connect_string(&connect_string)
        .map(|config| config.set_transport_connect_timeout(Some(connect_timeout(opts))))
        .map_err(map_error)
}

fn connect_timeout(opts: &ConnectOptions) -> Duration {
    opts.connect_timeout_secs
        .filter(|seconds| *seconds > 0)
        .map_or(Duration::from_secs(30), |seconds| {
            Duration::from_secs(u64::from(seconds))
        })
}

async fn probe_listener(opts: &ConnectOptions) -> Result<(), DriverError> {
    let Transport::Tcp { host, port } = opts.transport() else {
        return Ok(());
    };
    let wait = connect_timeout(opts);
    match tokio::time::timeout(wait, tokio::net::TcpStream::connect((host, port))).await {
        Err(_) => Err(DriverError::TimedOut),
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => Err(DriverError::ConnectionRefused),
        Ok(Err(error)) => Err(DriverError::Internal(format!("oracle listener probe: {error}"))),
    }
}

pub struct OracleConnection {
    inner: Arc<oracledb::Connection>,
}

async fn blocking<T, F>(work: F) -> Result<T, DriverError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, DriverError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| DriverError::Internal(format!("oracle worker: {error}")))?
}

fn map_error(error: oracledb::Error) -> DriverError {
    match error.kind() {
        ErrorKind::DbError(db) if matches!(db.code(), 1017 | 28000 | 1005) => DriverError::AuthFailed,
        ErrorKind::DbError(db) => DriverError::Query {
            message: db.message().to_string(),
            sqlstate: Some(format!("ORA-{:05}", db.code())),
            position: None,
        },
        ErrorKind::NoCredentials => DriverError::AuthFailed,
        ErrorKind::CallTimeoutExceeded => DriverError::TimedOut,
        ErrorKind::DeadConnection | ErrorKind::NotConnected | ErrorKind::UnableToRecover => DriverError::Disconnected,
        ErrorKind::ListenerRefusedConnection(..) => DriverError::ConnectionRefused,
        _ => DriverError::Internal(format!("oracle: {error}")),
    }
}

impl OracleConnection {
    async fn read_query(&self, sql: &str, binds: Vec<String>, limit: usize) -> Result<QueryResult, DriverError> {
        let connection = Arc::clone(&self.inner);
        let sql = sql.to_string();
        blocking(move || run_query(&connection, &sql, &binds, limit)).await
    }
}

fn run_query(
    connection: &oracledb::Connection,
    sql: &str,
    binds: &[String],
    limit: usize,
) -> Result<QueryResult, DriverError> {
    let params: Vec<&dyn oracledb::ToDbValue> = binds.iter().map(|bind| bind as &dyn oracledb::ToDbValue).collect();
    let cursor = connection.query(sql, &params).map_err(map_error)?;
    let metadata = cursor.columns().to_vec();
    let columns = metadata.iter().map(catalog::column_info).collect::<Vec<_>>();
    let kinds = metadata
        .iter()
        .map(|column| (column.db_type().name().to_string(), column.data_type()))
        .collect::<Vec<_>>();
    let mut budget = QueryResultBudget::default();
    let mut rows = Vec::new();
    let mut truncated = false;
    for row in cursor {
        let row = row.map_err(map_error)?;
        let cells = kinds
            .iter()
            .enumerate()
            .map(|(index, (db_type, type_name))| value::cell(&row, index, db_type, type_name))
            .collect::<Result<Vec<Value>, DriverError>>()?;
        if rows.len() >= limit || !budget.admit(&cells) {
            truncated = true;
            break;
        }
        rows.push(cells);
    }
    Ok(QueryResult {
        columns,
        rows: QueryRows::from_rows(rows),
        truncated,
    })
}

fn refuse_write() -> DriverError {
    DriverError::ReadOnly
}

#[async_trait]
impl Connection for OracleConnection {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        catalog::list(self, catalog::TABLES_SQL).await
    }

    async fn list_views(&self) -> Result<Vec<TableInfo>, DriverError> {
        catalog::list(self, catalog::VIEWS_SQL).await
    }

    async fn fetch_columns(&self, schema: Option<&str>, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        catalog::columns(self, schema, table).await
    }

    async fn fetch_rows(
        &self,
        schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<QueryResult, DriverError> {
        let sql = format!(
            "SELECT * FROM {} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY",
            guard::qualified(schema, table)
        );
        let bound = usize::try_from(limit).unwrap_or(MAX_QUERY_ROWS);
        self.read_query(&sql, Vec::new(), bound).await
    }

    async fn query(&self, sql: &str) -> Result<QueryResult, DriverError> {
        let statement = guard::read_only_statement(sql).ok_or_else(refuse_write)?;
        self.read_query(&statement, Vec::new(), MAX_QUERY_ROWS).await
    }

    async fn execute(&self, _sql: &str) -> Result<ExecResult, DriverError> {
        Err(refuse_write())
    }

    async fn execute_params(&self, _sql: &str, _params: &[Value]) -> Result<ExecResult, DriverError> {
        Err(refuse_write())
    }

    async fn execute_in_transaction(&self, _statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Err(refuse_write())
    }

    async fn ping(&self) -> Result<(), DriverError> {
        self.read_query("SELECT 1 FROM dual", Vec::new(), 1).await.map(|_| ())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        let connection = Arc::clone(&self.inner);
        drop(self);
        blocking(move || {
            drop(connection);
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests;
