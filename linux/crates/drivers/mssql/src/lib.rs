use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures::TryStreamExt;
use secrecy::ExposeSecret;
use tiberius::{AuthMethod, Client, Column, ColumnType, Config, EncryptionLevel, QueryItem, ToSql};
use tokio::net::TcpStream;
use tokio::sync::{MappedMutexGuard, Mutex, Notify};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use tablepro_core::sql_dialect::build_order_and_pagination;
use tablepro_core::{
    AuthMode, ColumnInfo, ConnectOptions, Connection, DatabaseDriver, DriverError, ExecResult, ForeignKeyInfo,
    IndexInfo, MAX_QUERY_ROWS, OperationControl, QueryResult, TableInfo, Value, check_pre_dispatch,
    run_server_cancellable,
};

mod codec;
mod session;
mod variant_guard;
mod zoned;

type MssqlClient = Client<Compat<TcpStream>>;

/// Matches the `acquire_timeout` the sqlx-backed drivers give their
/// pools, so a dead host fails at the same speed on every engine.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
static KERBEROS_CONNECTING: AtomicBool = AtomicBool::new(false);

struct KerberosAttempt;

impl KerberosAttempt {
    fn acquire() -> Result<Self, DriverError> {
        KERBEROS_CONNECTING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| DriverError::IntegratedAuth("another Kerberos connection attempt is still running".into()))
    }
}

impl Drop for KerberosAttempt {
    fn drop(&mut self) {
        KERBEROS_CONNECTING.store(false, Ordering::Release);
    }
}

pub struct MssqlDriver;

#[async_trait]
impl DatabaseDriver for MssqlDriver {
    fn id(&self) -> &'static str {
        "mssql"
    }

    fn display_name(&self) -> &'static str {
        "SQL Server"
    }

    fn default_port(&self) -> u16 {
        1433
    }

    fn default_database(&self) -> &'static str {
        "master"
    }

    fn default_username(&self) -> &'static str {
        "sa"
    }

    fn ddl_is_transactional(&self) -> bool {
        true
    }

    fn supports_index_metadata(&self) -> bool {
        true
    }

    fn supports_foreign_key_metadata(&self) -> bool {
        true
    }

    fn supports_integrated_auth(&self) -> bool {
        cfg!(feature = "kerberos")
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        let target = build_target(&opts)?;
        let client = match opts.auth_mode {
            AuthMode::Password => tokio::time::timeout(CONNECT_TIMEOUT, open_client(target))
                .await
                .map_err(|_| DriverError::ConnectionRefused)??,
            AuthMode::Kerberos => open_kerberos_client(target).await?,
        };

        Ok(Box::new(MssqlConnection::new(client, opts)))
    }
}

struct MssqlTarget {
    config: Config,
    dial_host: String,
    dial_port: u16,
}

fn build_target(opts: &ConnectOptions) -> Result<MssqlTarget, DriverError> {
    let (service_host, service_port) = opts.service_address();
    let mut config = Config::new();
    config.host(service_host);
    config.port(service_port);
    config.database(&opts.database);
    config.authentication(auth_method(opts)?);
    use tablepro_core::TlsMode;
    match opts.tls.mode {
        TlsMode::Disabled => {
            config.encryption(EncryptionLevel::Off);
            config.trust_cert();
        }
        TlsMode::Prefer | TlsMode::Require => {
            config.encryption(EncryptionLevel::Required);
            config.trust_cert();
        }
        TlsMode::VerifyCa | TlsMode::VerifyFull => {
            config.encryption(EncryptionLevel::Required);
            if let Some(root_cert) = &opts.tls.root_cert {
                config.trust_cert_ca(root_cert.display().to_string());
            }
        }
    }

    Ok(MssqlTarget {
        config,
        dial_host: dial_host(&opts.host).to_string(),
        dial_port: opts.port,
    })
}

fn auth_method(opts: &ConnectOptions) -> Result<AuthMethod, DriverError> {
    match opts.auth_mode {
        AuthMode::Password => Ok(AuthMethod::sql_server(&opts.username, opts.password.expose_secret())),
        #[cfg(feature = "kerberos")]
        AuthMode::Kerberos => Ok(AuthMethod::Integrated),
        #[cfg(not(feature = "kerberos"))]
        AuthMode::Kerberos => Err(DriverError::IntegratedAuth(
            "Kerberos support is not enabled in this build".into(),
        )),
    }
}

fn dial_host(host: &str) -> &str {
    if host == "." { "localhost" } else { host }
}

async fn open_kerberos_client(target: MssqlTarget) -> Result<MssqlClient, DriverError> {
    let attempt = KerberosAttempt::acquire()?;
    let handle = tokio::runtime::Handle::current();
    let connecting = tokio::task::spawn_blocking(move || {
        let _attempt = attempt;
        handle.block_on(open_client(target))
    });
    match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(DriverError::Internal(error.to_string())),
        Err(_) => Err(DriverError::ConnectionRefused),
    }
}

async fn open_client(target: MssqlTarget) -> Result<MssqlClient, DriverError> {
    let tcp = TcpStream::connect((target.dial_host.as_str(), target.dial_port))
        .await
        .map_err(map_io_error)?;
    tcp.set_nodelay(true).map_err(map_io_error)?;
    Client::connect(target.config, tcp.compat_write())
        .await
        .map_err(map_tiberius_error)
}

struct MssqlConnection {
    client: Mutex<Option<MssqlClient>>,
    usable: AtomicBool,
    fault: std::sync::Mutex<Option<Arc<Notify>>>,
    session_options: ConnectOptions,
}

impl MssqlConnection {
    fn new(client: MssqlClient, session_options: ConnectOptions) -> Self {
        Self {
            client: Mutex::new(Some(client)),
            usable: AtomicBool::new(true),
            fault: std::sync::Mutex::new(None),
            session_options,
        }
    }

    #[cfg(test)]
    fn unconnected() -> Self {
        Self {
            client: Mutex::new(None),
            usable: AtomicBool::new(true),
            fault: std::sync::Mutex::new(None),
            session_options: ConnectOptions::default(),
        }
    }

    /// Every statement goes through here so an abandoned one cannot
    /// strand the next caller. tiberius has no way to resynchronise a
    /// client whose token stream was left half-read, and the client sits
    /// behind a mutex, so a single abandoned statement would otherwise
    /// block every later operation on this connection forever.
    async fn client(&self) -> Result<MappedMutexGuard<'_, MssqlClient>, DriverError> {
        if !self.usable.load(Ordering::Acquire) {
            return Err(DriverError::Disconnected);
        }
        let guard = self.client.lock().await;
        match tokio::sync::MutexGuard::try_map(guard, Option::as_mut) {
            Ok(client) => Ok(client),
            Err(_guard) => Err(DriverError::Disconnected),
        }
    }

    /// Runs `operation` under the caller's cancellation and deadline.
    /// tiberius 0.12 exposes no way to send the TDS attention packet, so
    /// an interruption cannot stop the statement on the server: the
    /// outcome stays unknown and the connection is retired rather than
    /// reused. `supports_server_cancellation` reports false for exactly
    /// this reason.
    async fn run_abandonable<T, F>(&self, operation: F, control: &OperationControl) -> Result<T, DriverError>
    where
        F: std::future::Future<Output = Result<T, DriverError>>,
    {
        check_pre_dispatch(control)?;
        run_server_cancellable(operation, self.retire(), |_| false, control).await
    }

    async fn retire(&self) -> Result<(), DriverError> {
        self.usable.store(false, Ordering::Release);
        let guard = self.fault.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(fault) = guard.as_ref() {
            fault.notify_one();
        }
        Ok(())
    }

    async fn query_result(
        &self,
        client: &mut MssqlClient,
        sql: &str,
        params: &[Value],
        limit: usize,
    ) -> Result<QueryResult, DriverError> {
        let result = run_query(client, sql, params, limit).await;
        if result.as_ref().is_err_and(variant_guard::is_unsupported_result) {
            self.retire().await?;
        }
        result
    }
}

#[async_trait]
impl Connection for MssqlConnection {
    fn attach_fault_notify(&self, notify: Arc<Notify>) {
        *self.fault.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(notify);
    }

    async fn open_session(&self) -> Result<Box<dyn tablepro_core::Session>, DriverError> {
        session::open(&self.session_options).await
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        let sql = "SELECT s.name AS schema_name, t.name AS table_name \
                   FROM sys.tables t \
                   JOIN sys.schemas s ON t.schema_id = s.schema_id \
                   ORDER BY s.name, t.name";
        let mut client = self.client().await?;
        let result = self.query_result(&mut client, sql, &[], MAX_QUERY_ROWS).await?;
        Ok(result
            .rows
            .iter()
            .filter_map(|row| {
                let name = as_text(row.get(1))?;
                Some(TableInfo {
                    schema: as_text(row.first()),
                    name,
                })
            })
            .collect())
    }

    async fn fetch_columns(&self, schema: Option<&str>, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        // sys catalog is authoritative: is_identity/is_computed are exact
        // flags, sys.default_constraints.definition carries the DEFAULT
        // expression, and the primary-key join flags PK members. Type text
        // is rebuilt from sys.types + length/precision so it reads like the
        // user's CREATE TABLE (e.g. `nvarchar(255)`, `decimal(18,2)`).
        // A column comment is an extended property named MS_Description,
        // the convention SSMS reads and writes. Its value is sql_variant,
        // which tiberius cannot decode, so it is CAST to nvarchar here.
        let sql = "SELECT \
                       c.name AS col_name, \
                       ty.name AS type_name, \
                       c.max_length, \
                       c.precision, \
                       c.scale, \
                       c.is_nullable, \
                       c.is_identity, \
                       c.is_computed, \
                       dc.definition AS default_def, \
                       CASE WHEN pk.column_id IS NOT NULL THEN 1 ELSE 0 END AS is_pk, \
                       CAST(ep.value AS nvarchar(max)) AS column_comment, \
                       c.collation_name AS collation_name \
                   FROM sys.columns c \
                   JOIN sys.objects o ON c.object_id = o.object_id \
                   JOIN sys.schemas sc ON o.schema_id = sc.schema_id \
                   JOIN sys.types ty ON c.user_type_id = ty.user_type_id \
                   LEFT JOIN sys.default_constraints dc ON dc.object_id = c.default_object_id \
                   LEFT JOIN ( \
                       SELECT ic.object_id, ic.column_id \
                       FROM sys.index_columns ic \
                       JOIN sys.indexes i ON i.object_id = ic.object_id AND i.index_id = ic.index_id \
                       WHERE i.is_primary_key = 1 \
                   ) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id \
                   LEFT JOIN sys.extended_properties ep ON ep.class = 1 \
                       AND ep.major_id = c.object_id AND ep.minor_id = c.column_id \
                       AND ep.name = 'MS_Description' \
                   WHERE o.name = @P1 AND sc.name = COALESCE(@P2, SCHEMA_NAME()) \
                   ORDER BY c.column_id";
        let mut client = self.client().await?;
        let result = self
            .query_result(
                &mut client,
                sql,
                &[text_param(table), schema_param(schema)],
                MAX_QUERY_ROWS,
            )
            .await?;
        Ok(result.rows.iter().map(|r| row_to_column_info(r.as_slice())).collect())
    }

    async fn fetch_rows(
        &self,
        schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<QueryResult, DriverError> {
        let sql = format!(
            "SELECT * FROM {}{}",
            qualified(schema, table),
            build_order_and_pagination("mssql", None, limit, offset)
        );
        let mut client = self.client().await?;
        self.query_result(&mut client, &sql, &[], limit as usize).await
    }

    async fn query(&self, sql: &str) -> Result<QueryResult, DriverError> {
        let mut client = self.client().await?;
        self.query_result(&mut client, sql, &[], MAX_QUERY_ROWS).await
    }

    async fn query_controlled(&self, sql: &str, control: &OperationControl) -> Result<QueryResult, DriverError> {
        self.query_params_controlled(sql, &[], control).await
    }

    async fn query_params_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        let mut client = self.client().await?;
        self.run_abandonable(self.query_result(&mut client, sql, params, MAX_QUERY_ROWS), control)
            .await
    }

    async fn execute_controlled(&self, sql: &str, control: &OperationControl) -> Result<ExecResult, DriverError> {
        self.execute_params_controlled(sql, &[], control).await
    }

    async fn execute_params_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<ExecResult, DriverError> {
        let mut client = self.client().await?;
        let rows_affected = self
            .run_abandonable(run_execute(&mut client, sql, params), control)
            .await?;
        Ok(ExecResult { rows_affected })
    }

    async fn query_params(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DriverError> {
        let mut client = self.client().await?;
        self.query_result(&mut client, sql, params, MAX_QUERY_ROWS).await
    }

    async fn execute(&self, sql: &str) -> Result<ExecResult, DriverError> {
        let mut client = self.client().await?;
        let rows_affected = run_execute(&mut client, sql, &[]).await?;
        Ok(ExecResult { rows_affected })
    }

    async fn execute_params(&self, sql: &str, params: &[Value]) -> Result<ExecResult, DriverError> {
        let mut client = self.client().await?;
        let rows_affected = run_execute(&mut client, sql, params).await?;
        Ok(ExecResult { rows_affected })
    }

    async fn execute_in_transaction(&self, statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        let mut client = self.client().await?;
        exec_simple(&mut client, "BEGIN TRANSACTION").await?;
        let mut affected = Vec::with_capacity(statements.len());
        for (idx, (sql, params)) in statements.iter().enumerate() {
            let boxes = match boxed_params(params) {
                Ok(boxes) => boxes,
                Err(error) => {
                    let _ = exec_simple(&mut client, "ROLLBACK").await;
                    return Err(DriverError::Transaction {
                        statement_index: idx,
                        source: Box::new(error),
                    });
                }
            };
            let refs: Vec<&dyn ToSql> = boxes.iter().map(|b| &**b as &dyn ToSql).collect();
            match client.execute(sql.as_str(), &refs).await {
                Ok(res) => affected.push(res.total()),
                Err(e) => {
                    let _ = exec_simple(&mut client, "ROLLBACK").await;
                    return Err(DriverError::Transaction {
                        statement_index: idx,
                        source: Box::new(map_tiberius_error(e)),
                    });
                }
            }
        }
        exec_simple(&mut client, "COMMIT").await?;
        Ok(affected)
    }

    async fn fetch_indexes(&self, schema: Option<&str>, table: &str) -> Result<Vec<IndexInfo>, DriverError> {
        // Flat rows (one per index column) ordered by key_ordinal; grouped
        // into IndexInfo below since TDS has no array aggregation.
        // INCLUDE columns are not part of the key and carry key_ordinal
        // 0, so leaving them in would both list them as key columns and
        // sort them ahead of the real ones.
        let sql = "SELECT i.name AS index_name, i.is_unique, i.is_primary_key, c.name AS col_name \
                   FROM sys.indexes i \
                   JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
                   JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
                   JOIN sys.objects o ON o.object_id = i.object_id \
                   JOIN sys.schemas s ON s.schema_id = o.schema_id \
                   WHERE o.name = @P1 AND s.name = COALESCE(@P2, SCHEMA_NAME()) \
                     AND i.name IS NOT NULL AND i.type > 0 \
                     AND ic.is_included_column = 0 \
                   ORDER BY i.name, ic.key_ordinal";
        let mut client = self.client().await?;
        let result = self
            .query_result(
                &mut client,
                sql,
                &[text_param(table), schema_param(schema)],
                MAX_QUERY_ROWS,
            )
            .await?;
        let mut out: Vec<IndexInfo> = Vec::new();
        for row in &result.rows {
            let Some(name) = as_text(row.first()) else {
                continue;
            };
            let unique = as_bool(row.get(1)).unwrap_or(false);
            let primary = as_bool(row.get(2)).unwrap_or(false);
            let col = as_text(row.get(3)).unwrap_or_default();
            match out.iter_mut().find(|ix| ix.name == name) {
                Some(ix) => ix.columns.push(col),
                None => out.push(IndexInfo {
                    name,
                    columns: vec![col],
                    unique,
                    primary,
                    predicate: None,
                }),
            }
        }
        Ok(out)
    }

    async fn fetch_foreign_keys(&self, schema: Option<&str>, table: &str) -> Result<Vec<ForeignKeyInfo>, DriverError> {
        let sql = "SELECT fk.name AS fk_name, \
                       cpar.name AS col_name, \
                       rs.name AS ref_schema, \
                       rt.name AS ref_table, \
                       cref.name AS ref_col, \
                       fk.delete_referential_action_desc, \
                       fk.update_referential_action_desc \
                   FROM sys.foreign_keys fk \
                   JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id \
                   JOIN sys.objects o ON o.object_id = fk.parent_object_id \
                   JOIN sys.schemas s ON s.schema_id = o.schema_id \
                   JOIN sys.columns cpar ON cpar.object_id = fk.parent_object_id AND cpar.column_id = fkc.parent_column_id \
                   JOIN sys.objects rt ON rt.object_id = fk.referenced_object_id \
                   JOIN sys.schemas rs ON rs.schema_id = rt.schema_id \
                   JOIN sys.columns cref ON cref.object_id = fk.referenced_object_id AND cref.column_id = fkc.referenced_column_id \
                   WHERE o.name = @P1 AND s.name = COALESCE(@P2, SCHEMA_NAME()) \
                   ORDER BY fk.name, fkc.constraint_column_id";
        let mut client = self.client().await?;
        let result = self
            .query_result(
                &mut client,
                sql,
                &[text_param(table), schema_param(schema)],
                MAX_QUERY_ROWS,
            )
            .await?;
        let mut out: Vec<ForeignKeyInfo> = Vec::new();
        for row in &result.rows {
            let Some(name) = as_text(row.first()) else {
                continue;
            };
            let col = as_text(row.get(1)).unwrap_or_default();
            let ref_col = as_text(row.get(4)).unwrap_or_default();
            match out.iter_mut().find(|fk| fk.name == name) {
                Some(fk) => {
                    fk.columns.push(col);
                    fk.ref_columns.push(ref_col);
                }
                None => out.push(ForeignKeyInfo {
                    name,
                    columns: vec![col],
                    ref_schema: as_text(row.get(2)),
                    ref_table: as_text(row.get(3)).unwrap_or_default(),
                    ref_columns: vec![ref_col],
                    on_delete: as_text(row.get(5)).and_then(|d| map_referential_action(&d)),
                    on_update: as_text(row.get(6)).and_then(|d| map_referential_action(&d)),
                }),
            }
        }
        Ok(out)
    }

    async fn ping(&self) -> Result<(), DriverError> {
        let mut client = self.client().await?;
        exec_simple(&mut client, "SELECT 1").await
    }

    fn supports_server_cancellation(&self) -> bool {
        false
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        if !self.usable.load(Ordering::Acquire) {
            return Ok(());
        }
        match self.client.into_inner() {
            Some(client) => client.close().await.map_err(map_tiberius_error),
            None => Ok(()),
        }
    }
}

async fn run_query(
    client: &mut MssqlClient,
    sql: &str,
    params: &[Value],
    limit: usize,
) -> Result<QueryResult, DriverError> {
    variant_guard::catch_tiberius_variant_panic(async { run_query_inner(client, sql, params, limit).await }).await
}

async fn run_query_inner(
    client: &mut MssqlClient,
    sql: &str,
    params: &[Value],
    limit: usize,
) -> Result<QueryResult, DriverError> {
    let boxes = boxed_params(params)?;
    let refs: Vec<&dyn ToSql> = boxes.iter().map(|b| &**b as &dyn ToSql).collect();
    let stream = client.query(sql, &refs).await.map_err(map_tiberius_error)?;
    collect_result(stream, limit).await
}

// A parameterized query travels through sp_executesql, whose scope drops a
// #temp table and refuses a transaction left open when it returns. A
// statement without parameters is sent as a plain batch so both survive.
async fn run_batch(client: &mut MssqlClient, sql: &str, limit: usize) -> Result<QueryResult, DriverError> {
    variant_guard::catch_tiberius_variant_panic(async {
        let stream = client.simple_query(sql).await.map_err(map_tiberius_error)?;
        collect_result(stream, limit).await
    })
    .await
}

async fn collect_result(mut stream: tiberius::QueryStream<'_>, limit: usize) -> Result<QueryResult, DriverError> {
    let mut columns: Vec<ColumnInfo> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut truncated = false;
    let mut result_sets = 0usize;
    // The stream is read to its end rather than dropped early: tiberius only
    // drains unread tokens at the start of the next call, so an early exit
    // leaves the batch running on the server holding its locks, and discards
    // any error the server raises after the first result set.
    while let Some(item) = stream.try_next().await.map_err(map_tiberius_error)? {
        match item {
            QueryItem::Metadata(meta) => {
                result_sets += 1;
                if result_sets == 1 {
                    columns = meta.columns().iter().map(codec::col_to_info).collect();
                }
            }
            QueryItem::Row(_) if result_sets > 1 => {}
            QueryItem::Row(_) if rows.len() >= limit => truncated = true,
            QueryItem::Row(row) => {
                let types: Vec<ColumnType> = row.columns().iter().map(Column::column_type).collect();
                rows.push(
                    row.into_iter()
                        .zip(types)
                        .map(|(data, column_type)| codec::column_data_to_value_for_type(&data, column_type))
                        .collect(),
                );
            }
        }
    }
    Ok(QueryResult {
        columns,
        rows,
        truncated,
    })
}

async fn run_execute(client: &mut MssqlClient, sql: &str, params: &[Value]) -> Result<u64, DriverError> {
    let boxes = boxed_params(params)?;
    let refs: Vec<&dyn ToSql> = boxes.iter().map(|b| &**b as &dyn ToSql).collect();
    let res = client.execute(sql, &refs).await.map_err(map_tiberius_error)?;
    Ok(res.total())
}

/// Run a statement whose result set we don't consume (transaction control,
/// `SELECT 1` liveness). The stream must be drained so the DONE token is
/// read and the connection is left ready for the next command.
async fn exec_simple(client: &mut MssqlClient, sql: &str) -> Result<(), DriverError> {
    let stream = client.simple_query(sql).await.map_err(map_tiberius_error)?;
    stream.into_results().await.map_err(map_tiberius_error)?;
    Ok(())
}

fn boxed_params(params: &[Value]) -> Result<Vec<Box<dyn ToSql>>, DriverError> {
    params
        .iter()
        .map(|p| -> Result<Box<dyn ToSql>, DriverError> {
            Ok(match p {
                // Type NULL as nvarchar: a typed-int NULL breaks COALESCE /
                // comparisons against string columns (e.g. the introspection
                // `COALESCE(@P, SCHEMA_NAME())`), while NULL always converts
                // cleanly into any target column type on INSERT/UPDATE.
                Value::Null => Box::new(Option::<String>::None),
                Value::Bool(b) => Box::new(*b),
                Value::Int(i) => Box::new(*i),
                Value::Float(f) => Box::new(*f),
                Value::Text(s) => Box::new(s.clone()),
                Value::Bytes(b) => Box::new(b.clone()),
                Value::Date(d) => Box::new(*d),
                Value::Time(t) => Box::new(*t),
                Value::DateTime(dt) => Box::new(*dt),
                Value::TimestampTz(ts) => Box::new(*ts),
                Value::Decimal(d) => Box::new(*d),
                Value::Uuid(u) => Box::new(*u),
                // TDS has no JSON type; SQL Server stores JSON as nvarchar.
                Value::Json(j) => Box::new(serde_json::to_string(j).unwrap_or_default()),
                Value::Undecodable(_) => {
                    return Err(DriverError::Unsupported(
                        "undecodable cell cannot be bound as a parameter".into(),
                    ));
                }
            })
        })
        .collect()
}

fn row_to_column_info(row: &[Value]) -> ColumnInfo {
    let type_name = as_text(row.get(1)).unwrap_or_default();
    let max_length = as_i64(row.get(2)).unwrap_or(0);
    let precision = as_i64(row.get(3)).unwrap_or(0);
    let scale = as_i64(row.get(4)).unwrap_or(0);
    let is_identity = as_bool(row.get(6)).unwrap_or(false);
    let default_raw = as_text(row.get(8));
    ColumnInfo {
        name: as_text(row.first()).unwrap_or_default(),
        data_type: format_mssql_type(&type_name, max_length, precision, scale),
        nullable: as_bool(row.get(5)).unwrap_or(true),
        primary_key: as_bool(row.get(9)).unwrap_or(false),
        is_auto_increment: is_identity,
        // IDENTITY columns carry an internal seed/increment, not a user
        // DEFAULT — suppress so the inline-insert UI treats them as auto.
        default_value: if is_identity {
            None
        } else {
            default_raw.map(|d| normalize_mssql_default(&d))
        },
        is_generated: as_bool(row.get(7)).unwrap_or(false),
        comment: as_text(row.get(10)).filter(|c| !c.is_empty()),
        collation: as_text(row.get(11)).filter(|c| !c.is_empty()),
    }
}

/// Rebuild a user-facing type string from `sys.types` metadata. `nvarchar`
/// / `nchar` store `max_length` in bytes (two per character); `-1` is the
/// `(max)` sentinel. Numeric types carry precision/scale.
fn format_mssql_type(type_name: &str, max_length: i64, precision: i64, scale: i64) -> String {
    match type_name.to_ascii_lowercase().as_str() {
        "varchar" | "char" | "varbinary" | "binary" => {
            if max_length < 0 {
                format!("{type_name}(max)")
            } else {
                format!("{type_name}({max_length})")
            }
        }
        "nvarchar" | "nchar" => {
            if max_length < 0 {
                format!("{type_name}(max)")
            } else {
                format!("{type_name}({})", max_length / 2)
            }
        }
        "decimal" | "numeric" => format!("{type_name}({precision},{scale})"),
        _ => type_name.to_string(),
    }
}

// sys.default_constraints.definition wraps the expression in one or two
// pairs of parentheses (`((0))`, `('pending')`); they are peeled so the
// default reads as the DEFAULT clause that created it.
fn normalize_mssql_default(raw: &str) -> String {
    let mut s = raw.trim();
    while outer_parens_wrap(s) {
        s = s[1..s.len() - 1].trim();
    }
    s.to_string()
}

fn outer_parens_wrap(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'(' || bytes[bytes.len() - 1] != b')' {
        return false;
    }
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                // A close that returns to depth 0 before the final byte
                // means the leading `(` does not wrap the whole string
                // (e.g. `(a)+(b)`); don't strip.
                if depth == 0 && i != bytes.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Map SQL Server's `*_referential_action_desc` text to the canonical SQL
/// keyword the FK/DDL layer expects. `NO_ACTION` returns `None` so the DDL
/// builder omits the redundant clause.
fn map_referential_action(desc: &str) -> Option<String> {
    match desc.to_ascii_uppercase().as_str() {
        "CASCADE" => Some("CASCADE".into()),
        "SET_NULL" => Some("SET NULL".into()),
        "SET_DEFAULT" => Some("SET DEFAULT".into()),
        _ => None,
    }
}

fn quote_ident(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

fn qualified(schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(table)),
        None => quote_ident(table),
    }
}

fn text_param(s: &str) -> Value {
    Value::Text(s.to_string())
}

fn schema_param(schema: Option<&str>) -> Value {
    match schema {
        Some(s) => Value::Text(s.to_string()),
        None => Value::Null,
    }
}

fn as_text(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::Text(s)) => Some(s.clone()),
        _ => None,
    }
}

fn as_i64(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Int(i)) => Some(*i),
        _ => None,
    }
}

fn as_bool(v: Option<&Value>) -> Option<bool> {
    match v {
        Some(Value::Bool(b)) => Some(*b),
        // sys catalog bit columns and the CASE-derived is_pk flag can arrive
        // as an integer depending on the shape of the projection.
        Some(Value::Int(i)) => Some(*i != 0),
        _ => None,
    }
}

fn map_io_error(e: std::io::Error) -> DriverError {
    if e.kind() == std::io::ErrorKind::ConnectionRefused {
        DriverError::ConnectionRefused
    } else {
        DriverError::Internal(e.to_string())
    }
}

fn map_tiberius_error(err: tiberius::error::Error) -> DriverError {
    use tiberius::error::Error as E;
    match err {
        E::Io { .. } => DriverError::Disconnected,
        E::Tls(msg) => DriverError::Tls(msg),
        E::Server(token) => {
            // 18456 = "Login failed for user"; surface as an auth failure so
            // the UI shows the right remediation instead of a raw SQL error.
            if token.code() == 18456 {
                DriverError::AuthFailed
            } else {
                DriverError::Query {
                    message: token.message().to_string(),
                    sqlstate: Some(token.state().to_string()),
                }
            }
        }
        #[cfg(feature = "kerberos")]
        E::Gssapi(detail) => DriverError::IntegratedAuth(detail),
        E::Routing { host, port } => DriverError::Internal(format!("server requested routing to {host}:{port}")),
        other => DriverError::Internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undecodable_cell_cannot_be_bound_as_null() {
        assert!(matches!(
            boxed_params(&[Value::Undecodable("geometry".into())]),
            Err(DriverError::Unsupported(_))
        ));
    }

    #[test]
    fn structure_metadata_declarations_match_the_connection_impl() {
        let d = MssqlDriver;
        let source = include_str!("lib.rs");
        assert!(d.supports_index_metadata());
        assert!(d.supports_foreign_key_metadata());
        assert!(source.contains(&["async fn ", "fetch_indexes("].concat()));
        assert!(source.contains(&["async fn ", "fetch_foreign_keys("].concat()));
    }

    #[test]
    fn driver_metadata() {
        let d = MssqlDriver;
        assert_eq!(d.id(), "mssql");
        assert_eq!(d.display_name(), "SQL Server");
        assert_eq!(d.default_port(), 1433);
        assert_eq!(d.default_database(), "master");
        assert_eq!(d.default_username(), "sa");
        assert!(!d.is_file_based());
        assert_eq!(d.supports_integrated_auth(), cfg!(feature = "kerberos"));
    }

    fn direct_options() -> ConnectOptions {
        ConnectOptions {
            host: "sql.corp.example".into(),
            port: 1433,
            database: "sales".into(),
            ..Default::default()
        }
    }

    #[test]
    fn direct_connection_uses_service_identity_as_dial_endpoint() {
        let target = build_target(&direct_options()).unwrap();

        assert_eq!(target.config.get_addr(), "sql.corp.example:1433");
        assert_eq!(
            (target.dial_host.as_str(), target.dial_port),
            ("sql.corp.example", 1433)
        );
    }

    #[test]
    fn tunnel_connection_uses_service_identity_for_tls_and_spn() {
        let options = ConnectOptions {
            host: "127.0.0.1".into(),
            port: 54321,
            service_endpoint: Some(("sql.corp.example".into(), 1433)),
            ..direct_options()
        };
        let target = build_target(&options).unwrap();

        assert_eq!(target.config.get_addr(), "sql.corp.example:1433");
        assert_eq!((target.dial_host.as_str(), target.dial_port), ("127.0.0.1", 54321));
    }

    #[test]
    fn local_server_shorthand_dials_localhost() {
        let options = ConnectOptions {
            host: ".".into(),
            ..direct_options()
        };
        let target = build_target(&options).unwrap();

        assert_eq!(target.config.get_addr(), "localhost:1433");
        assert_eq!(target.dial_host, "localhost");
    }

    #[test]
    fn verify_ca_with_a_named_authority_does_not_panic_on_conflicting_trust_settings() {
        // tiberius panics if trust_cert() and trust_cert_ca() are both
        // called on the same Config -- build_target must route a saved
        // root_cert into trust_cert_ca() only, never trust_cert(), for
        // VerifyCa/VerifyFull.
        let options = ConnectOptions {
            tls: tablepro_core::TlsConfig {
                mode: tablepro_core::TlsMode::VerifyCa,
                root_cert: Some(std::path::PathBuf::from("/etc/ssl/private-ca.pem")),
                ..Default::default()
            },
            ..direct_options()
        };
        let _ = build_target(&options).unwrap();
    }

    #[test]
    fn verify_full_without_a_named_authority_does_not_panic() {
        let options = ConnectOptions {
            tls: tablepro_core::TlsConfig {
                mode: tablepro_core::TlsMode::VerifyFull,
                ..Default::default()
            },
            ..direct_options()
        };
        let _ = build_target(&options).unwrap();
    }

    #[test]
    fn kerberos_connect_attempts_do_not_accumulate() {
        let first = KerberosAttempt::acquire().unwrap();
        assert!(KerberosAttempt::acquire().is_err());
        drop(first);
        assert!(KerberosAttempt::acquire().is_ok());
    }

    #[test]
    #[cfg(feature = "kerberos")]
    fn kerberos_uses_integrated_authentication() {
        let options = ConnectOptions {
            auth_mode: AuthMode::Kerberos,
            username: "ignored".into(),
            ..direct_options()
        };

        assert_eq!(auth_method(&options).unwrap(), AuthMethod::Integrated);
        assert_eq!(auth_method(&direct_options()).unwrap(), AuthMethod::sql_server("", ""));
    }

    #[test]
    #[cfg(feature = "kerberos")]
    fn gssapi_errors_are_classified_as_integrated_authentication_failures() {
        let error = map_tiberius_error(tiberius::error::Error::Gssapi("ticket expired".into()));

        assert!(matches!(error, DriverError::IntegratedAuth(detail) if detail == "ticket expired"));
    }

    #[test]
    #[cfg(not(feature = "kerberos"))]
    fn disabled_kerberos_is_rejected_before_connecting() {
        let options = ConnectOptions {
            auth_mode: AuthMode::Kerberos,
            ..direct_options()
        };
        assert!(!MssqlDriver.supports_integrated_auth());
        assert!(matches!(build_target(&options), Err(DriverError::IntegratedAuth(_))));
    }

    #[test]
    fn quote_ident_brackets_and_escapes() {
        assert_eq!(quote_ident("users"), "[users]");
        assert_eq!(quote_ident("My Table"), "[My Table]");
        assert_eq!(quote_ident("weird]name"), "[weird]]name]");
    }

    #[test]
    fn qualified_uses_bracket_quoting() {
        assert_eq!(qualified(Some("dbo"), "users"), "[dbo].[users]");
        assert_eq!(qualified(None, "users"), "[users]");
    }

    #[test]
    fn format_type_lengths_and_precision() {
        assert_eq!(format_mssql_type("int", 4, 10, 0), "int");
        assert_eq!(format_mssql_type("varchar", 255, 0, 0), "varchar(255)");
        assert_eq!(format_mssql_type("varchar", -1, 0, 0), "varchar(max)");
        // nvarchar max_length is in bytes: 510 bytes -> 255 chars.
        assert_eq!(format_mssql_type("nvarchar", 510, 0, 0), "nvarchar(255)");
        assert_eq!(format_mssql_type("nvarchar", -1, 0, 0), "nvarchar(max)");
        assert_eq!(format_mssql_type("decimal", 9, 18, 2), "decimal(18,2)");
    }

    #[test]
    fn normalize_default_peels_parens_and_keeps_literal_quotes() {
        assert_eq!(normalize_mssql_default("((0))"), "0");
        assert_eq!(normalize_mssql_default("('pending')"), "'pending'");
        assert_eq!(normalize_mssql_default("('')"), "''");
        assert_eq!(normalize_mssql_default("(NULL)"), "NULL");
        assert_eq!(normalize_mssql_default("(getdate())"), "getdate()");
        assert_eq!(normalize_mssql_default("(N'x')"), "N'x'");
        assert_eq!(normalize_mssql_default("('it''s')"), "'it''s'");
    }

    #[test]
    fn normalize_default_leaves_unbalanced_alone() {
        // A leading `(` that doesn't wrap the whole expression must not be
        // stripped, or the value would be corrupted.
        assert_eq!(normalize_mssql_default("(a)+(b)"), "(a)+(b)");
    }

    #[test]
    fn referential_actions_map_to_keywords() {
        assert_eq!(map_referential_action("CASCADE"), Some("CASCADE".to_string()));
        assert_eq!(map_referential_action("SET_NULL"), Some("SET NULL".to_string()));
        assert_eq!(map_referential_action("SET_DEFAULT"), Some("SET DEFAULT".to_string()));
        assert_eq!(map_referential_action("NO_ACTION"), None);
    }

    #[tokio::test(start_paused = true)]
    async fn retire_wakes_the_fault_sink_and_leaves_the_outcome_unknown() {
        let fault = Arc::new(Notify::new());
        let connection = MssqlConnection::unconnected();
        connection.attach_fault_notify(Arc::clone(&fault));

        let notified = fault.notified();
        tokio::pin!(notified);
        assert!(!notified.as_mut().enable(), "the sink must be idle until retire");

        let control = OperationControl::with_timeout(std::time::Duration::from_secs(1));
        let result = connection
            .run_abandonable(
                async {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    Err::<(), DriverError>(DriverError::Disconnected)
                },
                &control,
            )
            .await;

        assert!(
            matches!(
                result,
                Err(DriverError::OperationOutcomeUnknown { source })
                    if matches!(source.as_ref(), DriverError::Disconnected)
            ),
            "an in-flight call must stay unknown when cancellation is not confirmed"
        );
        assert!(!connection.usable.load(Ordering::Acquire));

        let woke = tokio::time::timeout(std::time::Duration::from_millis(1), notified).await;
        assert!(woke.is_ok(), "retire must wake the fault sink");
    }
}
