use std::time::Duration;

use async_trait::async_trait;
use bookie_core::{
    ColumnInfo, ConnectOptions, Connection, DatabaseDriver, DriverError, DriverMaturity, ExecResult, MAX_QUERY_ROWS,
    QueryResult, QueryResultBudget, QueryRows, TableInfo, TlsMode, Transport, Value,
};
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use scylla::response::PagingState;
use scylla::statement::Statement;
use secrecy::ExposeSecret;

mod catalog;
mod guard;
mod pin;
mod value;

const MAX_OFFSET: u64 = 100_000;

pub struct CassandraDriver;

#[async_trait]
impl DatabaseDriver for CassandraDriver {
    fn id(&self) -> &'static str {
        "cassandra"
    }

    fn display_name(&self) -> &'static str {
        "Cassandra / ScyllaDB"
    }

    fn maturity(&self) -> DriverMaturity {
        DriverMaturity::Experimental
    }

    fn default_port(&self) -> u16 {
        9042
    }

    fn default_database(&self) -> &'static str {
        ""
    }

    fn default_username(&self) -> &'static str {
        "cassandra"
    }

    fn reports_rows_affected(&self) -> bool {
        false
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        if opts.tls.mode != TlsMode::Disabled {
            return Err(DriverError::Unsupported(
                "TLS for Cassandra is not implemented yet; set TLS mode to Disabled on a trusted network".into(),
            ));
        }
        let Transport::Tcp { host, port } = opts.transport() else {
            return Err(DriverError::Unsupported("Cassandra connects over TCP only".into()));
        };
        probe_listener(host, port, wait(&opts)).await?;
        let node = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let keyspace = Some(opts.database.trim().to_string()).filter(|name| !name.is_empty());
        let contact = tokio::net::lookup_host(&node)
            .await
            .map_err(|error| DriverError::Internal(format!("cassandra address {node}: {error}")))?
            .next()
            .ok_or(DriverError::ConnectionRefused)?;
        let pin = std::sync::Arc::new(pin::ContactPin::new(contact));
        let mut builder = SessionBuilder::new()
            .known_node_addr(contact)
            .host_filter(pin.clone())
            .address_translator(pin)
            .connection_timeout(wait(&opts));
        if !opts.username.is_empty() {
            builder = builder.user(&opts.username, opts.password.expose_secret());
        }
        if let Some(keyspace) = &keyspace {
            builder = builder.use_keyspace(keyspace, true);
        }
        let session = builder.build().await.map_err(|error| map_error(&error.to_string()))?;
        Ok(Box::new(CassandraConnection { session, keyspace }))
    }
}

fn wait(opts: &ConnectOptions) -> Duration {
    opts.connect_timeout_secs
        .filter(|seconds| *seconds > 0)
        .map_or(Duration::from_secs(30), |seconds| {
            Duration::from_secs(u64::from(seconds))
        })
}

async fn probe_listener(host: &str, port: u16, wait: Duration) -> Result<(), DriverError> {
    match tokio::time::timeout(wait, tokio::net::TcpStream::connect((host, port))).await {
        Err(_) => Err(DriverError::TimedOut),
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => Err(DriverError::ConnectionRefused),
        Ok(Err(error)) => Err(DriverError::Internal(format!("cassandra listener probe: {error}"))),
    }
}

fn map_error(message: &str) -> DriverError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("authentication") || lower.contains("username and/or password") || lower.contains("unauthorized")
    {
        DriverError::AuthFailed
    } else if lower.contains("timed out") || lower.contains("timeout") {
        DriverError::TimedOut
    } else if lower.contains("connection refused") || lower.contains("broken pipe") || lower.contains("closed") {
        DriverError::Disconnected
    } else {
        DriverError::Query {
            message: message.to_string(),
            sqlstate: None,
            position: None,
        }
    }
}

pub struct CassandraConnection {
    session: Session,
    keyspace: Option<String>,
}

pub(crate) fn text(value: &Value) -> Option<String> {
    match value {
        Value::Text(text) => Some(text.clone()),
        _ => None,
    }
}

impl CassandraConnection {
    pub(crate) async fn read(&self, cql: &str, _binds: &[Value], limit: usize) -> Result<QueryResult, DriverError> {
        let mut statement = Statement::new(cql);
        let page = i32::try_from(limit.saturating_add(1).min(MAX_QUERY_ROWS + 1)).unwrap_or(i32::MAX);
        statement.set_page_size(page);
        let (result, paging) = self
            .session
            .query_single_page(statement, (), PagingState::start())
            .await
            .map_err(|error| map_error(&error.to_string()))?;
        let rows = match result.into_rows_result() {
            Ok(rows) => rows,
            Err(_) => {
                return Ok(QueryResult {
                    columns: Vec::new(),
                    rows: QueryRows::from_rows(Vec::new()),
                    truncated: false,
                });
            }
        };
        let columns: Vec<ColumnInfo> = rows
            .column_specs()
            .iter()
            .map(|spec| ColumnInfo {
                name: spec.name().to_string(),
                data_type: spec.typ().to_string(),
                nullable: true,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            })
            .collect();
        let mut budget = QueryResultBudget::default();
        let mut cells = Vec::new();
        let mut truncated = !paging.finished();
        for row in rows
            .rows::<scylla::value::Row>()
            .map_err(|error| DriverError::Internal(format!("cassandra rows: {error}")))?
        {
            let row = row.map_err(|error| DriverError::Internal(format!("cassandra row: {error}")))?;
            let values: Vec<Value> = row.columns.into_iter().map(value::cell).collect();
            if cells.len() >= limit || !budget.admit(&values) {
                truncated = true;
                break;
            }
            cells.push(values);
        }
        Ok(QueryResult {
            columns,
            rows: QueryRows::from_rows(cells),
            truncated,
        })
    }
}

fn refuse_write() -> DriverError {
    DriverError::ReadOnly
}

#[async_trait]
impl Connection for CassandraConnection {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        catalog::tables(self, catalog::TABLES_CQL).await
    }

    async fn list_views(&self) -> Result<Vec<TableInfo>, DriverError> {
        catalog::tables(self, catalog::VIEWS_CQL).await
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
        if offset > MAX_OFFSET {
            return Err(DriverError::Unsupported(format!(
                "Cassandra has no row offset; pages are limited to the first {MAX_OFFSET} rows"
            )));
        }
        let wanted = usize::try_from(offset.saturating_add(limit)).unwrap_or(MAX_QUERY_ROWS);
        let target = guard::qualified(schema.or(self.keyspace.as_deref()), table);
        let mut result = self
            .read(&format!("SELECT * FROM {target} LIMIT {wanted}"), &[], wanted)
            .await?;
        let skip = usize::try_from(offset).unwrap_or(usize::MAX);
        let rows: Vec<Vec<Value>> = result
            .rows
            .iter()
            .skip(skip)
            .take(usize::try_from(limit).unwrap_or(MAX_QUERY_ROWS))
            .map(|row| row.to_vec())
            .collect();
        result.rows = QueryRows::from_rows(rows);
        Ok(result)
    }

    async fn query(&self, cql: &str) -> Result<QueryResult, DriverError> {
        let statement = guard::read_only_statement(cql).ok_or_else(refuse_write)?;
        self.read(&statement, &[], MAX_QUERY_ROWS).await
    }

    async fn execute(&self, _cql: &str) -> Result<ExecResult, DriverError> {
        Err(refuse_write())
    }

    async fn execute_params(&self, _cql: &str, _params: &[Value]) -> Result<ExecResult, DriverError> {
        Err(refuse_write())
    }

    async fn execute_in_transaction(&self, _statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        Err(refuse_write())
    }

    async fn ping(&self) -> Result<(), DriverError> {
        self.read("SELECT release_version FROM system.local", &[], 1)
            .await
            .map(|_| ())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}
