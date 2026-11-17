use std::{future::Future, time::Duration};

use async_trait::async_trait;
use secrecy::ExposeSecret;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgCancelToken, PgConnectOptions};
use sqlx::{Connection as SqlxConnection, Pool, Postgres, Row};

use bookie_core::sql_syntax::script::LexicalSettings;
use bookie_core::{
    CANCELLATION_DISPATCH_TIMEOUT, CONTROL_SETUP_TIMEOUT, ColumnInfo, CommitResolution, CommitToken, ConnectOptions,
    Connection, DatabaseDriver, DriverError, DriverValueInputCapabilities, ExecResult, ForeignKeyInfo,
    InboundForeignKeyInfo, IndexInfo, MAX_QUERY_ROWS, OperationControl, QualifiedTypeName, QueryResult,
    QueryResultBatch, ServerMessages, TableInfo, Transport, Value, check_pre_dispatch, run_controlled_setup,
    run_server_cancellable, run_server_cancellable_preserving_transaction_rollback,
};

mod array;
mod catalog;
mod commit_resolution;
mod decode;
mod errors;
mod introspection;
mod notices;
mod numeric;
mod params;
mod pool;
mod pool_saturation;
mod query;
mod relations;
mod session;
mod temporal;
#[cfg(test)]
mod test_rng;
mod transaction;
mod view_editability;

use errors::*;
use introspection::*;
use params::{bind_pg_params, describe_query_parameters, needs_text_type_inference};
use query::{
    QueryStop, execute_connection, execute_connection_once, query_connection, query_connection_once,
    query_connection_once_with_rows_affected, query_connection_with_rows_affected, stream_into_result,
};

pub struct PgDriver;

#[async_trait]
impl DatabaseDriver for PgDriver {
    fn value_input_capabilities(&self) -> DriverValueInputCapabilities {
        DriverValueInputCapabilities {
            special_numeric_literals: true,
            extended_calendar_temporals: true,
            ..DriverValueInputCapabilities::default()
        }
    }

    fn id(&self) -> &'static str {
        "postgres"
    }

    fn supports_value_search(&self) -> bool {
        true
    }

    fn import_upsert(&self) -> bookie_core::ImportUpsertStyle {
        bookie_core::ImportUpsertStyle::OnConflict
    }

    fn supports_snapshot_export(&self) -> bool {
        true
    }
    fn supports_reverse_keyset(&self) -> bool {
        true
    }

    fn supports_updatable_views(&self) -> bool {
        true
    }

    fn display_name(&self) -> &'static str {
        "PostgreSQL"
    }

    fn default_port(&self) -> u16 {
        5432
    }

    fn default_database(&self) -> &'static str {
        "postgres"
    }

    fn default_username(&self) -> &'static str {
        "postgres"
    }

    fn ddl_is_transactional(&self) -> bool {
        true
    }

    fn supports_database_listing(&self) -> bool {
        true
    }

    fn supports_index_metadata(&self) -> bool {
        true
    }

    fn supports_table_health_report(&self) -> bool {
        true
    }

    fn supports_foreign_key_metadata(&self) -> bool {
        true
    }

    fn catalog_object_kinds(&self) -> &'static [bookie_core::CatalogObjectKind] {
        catalog::SUPPORTED_KINDS
    }

    fn supports_view_metadata(&self) -> bool {
        true
    }

    fn forwarded_socket_name(&self, service_port: u16) -> Option<String> {
        Some(format!(".s.PGSQL.{service_port}"))
    }

    fn supports_local_socket(&self) -> bool {
        true
    }

    fn supports_local_socket_directory(&self) -> bool {
        true
    }

    fn supports_sequence_restart(&self) -> bool {
        true
    }

    fn supports_client_tls_auth(&self) -> bool {
        true
    }

    fn supports_explicit_auto_increment_values(&self) -> bool {
        true
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        use bookie_core::TlsMode;
        let mut pg_opts = postgres_connect_options(&opts)?;
        pg_opts = pg_opts
            .database(&opts.database)
            .username(&opts.username)
            .password(opts.password.expose_secret())
            .ssl_mode(match opts.tls.mode {
                TlsMode::Disabled => sqlx::postgres::PgSslMode::Disable,
                TlsMode::Prefer => sqlx::postgres::PgSslMode::Prefer,
                TlsMode::Require => sqlx::postgres::PgSslMode::Require,
                TlsMode::VerifyCa => sqlx::postgres::PgSslMode::VerifyCa,
                TlsMode::VerifyFull => sqlx::postgres::PgSslMode::VerifyFull,
            });
        if let Some(path) = &opts.tls.root_cert {
            pg_opts = pg_opts.ssl_root_cert(path);
        }
        if let Some(path) = &opts.tls.client_cert {
            pg_opts = pg_opts.ssl_client_cert(path);
        }
        if let Some(path) = &opts.tls.client_key {
            pg_opts = pg_opts.ssl_client_key(path);
        }
        if let Some(name) = &opts.application_name {
            pg_opts = pg_opts.application_name(name);
        }
        if opts.read_only {
            pg_opts = pg_opts.options([("default_transaction_read_only", "on")]);
        }
        pg_opts = pg_opts.statement_cache_capacity(0);
        let session_options = pg_opts.clone();
        let pool = pool::options(opts.pool_acquire_timeout())
            .connect_with(pg_opts)
            .await
            .map_err(map_sqlx_connect_error)?;
        Ok(Box::new(PgConnection { pool, session_options }))
    }
}

fn postgres_connect_options(opts: &ConnectOptions) -> Result<PgConnectOptions, DriverError> {
    match opts.transport() {
        Transport::Tcp { host, port } => Ok(PgConnectOptions::new().host(host).port(port)),
        Transport::LocalSocketFile { .. } => {
            Err(DriverError::Unsupported("local socket files are not supported".into()))
        }
        Transport::Socket {
            directory,
            identity_host,
            identity_port,
            ..
        } => Ok(PgConnectOptions::new()
            .host(identity_host)
            .port(identity_port)
            .socket(directory)),
    }
}

macro_rules! stoppable {
    ($self:ident, $control:ident, |$connection:ident| $statement:expr) => {{
        let mut $connection = acquire_controlled(&$self.pool, $control).await?;
        let cancel_token = $connection.cancel_token();
        let result = run_server_cancellable(
            $statement,
            request_cancellation(cancel_token),
            confirms_cancellation,
            $control,
        )
        .await;
        finish_connection($connection, result).await.1
    }};
}

struct PgConnection {
    pool: Pool<Postgres>,
    session_options: PgConnectOptions,
}

#[async_trait]
impl Connection for PgConnection {
    async fn list_collations(&self) -> Result<Vec<bookie_core::CollationInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_list_collations(&mut connection).await
    }

    async fn list_collations_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::CollationInfo>, DriverError> {
        stoppable!(self, control, |connection| read_list_collations(&mut connection))
    }

    async fn list_databases(&self) -> Result<Vec<String>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_list_databases(&mut connection).await
    }

    async fn list_databases_controlled(&self, control: &OperationControl) -> Result<Vec<String>, DriverError> {
        stoppable!(self, control, |connection| read_list_databases(&mut connection))
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        self.list_tables_excluding(&[]).await
    }

    async fn list_tables_controlled(&self, control: &OperationControl) -> Result<Vec<TableInfo>, DriverError> {
        self.list_tables_excluding_controlled(&[], control).await
    }

    async fn list_tables_excluding(&self, schemas: &[String]) -> Result<Vec<TableInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        relations::list_tables(&mut connection, schemas).await
    }

    async fn list_tables_excluding_controlled(
        &self,
        schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        stoppable!(self, control, |connection| relations::list_tables(
            &mut connection,
            schemas
        ))
    }

    async fn list_tables_scoped(&self, schemas: &[String]) -> Result<Vec<TableInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_list_tables_scoped(&mut connection, schemas).await
    }

    async fn list_tables_in(
        &self,
        schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        stoppable!(self, control, |connection| read_list_tables_scoped(
            &mut connection,
            schemas
        ))
    }

    async fn list_views(&self) -> Result<Vec<TableInfo>, DriverError> {
        self.list_views_excluding(&[]).await
    }

    async fn list_views_controlled(&self, control: &OperationControl) -> Result<Vec<TableInfo>, DriverError> {
        self.list_views_excluding_controlled(&[], control).await
    }

    async fn list_views_excluding(&self, schemas: &[String]) -> Result<Vec<TableInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        relations::list_views(&mut connection, schemas).await
    }

    async fn list_views_excluding_controlled(
        &self,
        schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        stoppable!(self, control, |connection| relations::list_views(
            &mut connection,
            schemas
        ))
    }

    async fn list_objects(
        &self,
        kind: bookie_core::CatalogObjectKind,
        schema: Option<&str>,
    ) -> Result<Vec<bookie_core::CatalogObject>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        catalog::list_objects(&mut connection, kind, schema).await
    }

    async fn list_objects_controlled(
        &self,
        kind: bookie_core::CatalogObjectKind,
        schema: Option<&str>,
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::CatalogObject>, DriverError> {
        stoppable!(self, control, |connection| catalog::list_objects(
            &mut connection,
            kind,
            schema
        ))
    }

    async fn list_objects_excluding(
        &self,
        kind: bookie_core::CatalogObjectKind,
        schema: Option<&str>,
        excluded_schemas: &[String],
    ) -> Result<Vec<bookie_core::CatalogObject>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        catalog::list_objects_excluding(&mut connection, kind, schema, excluded_schemas).await
    }

    async fn list_objects_excluding_controlled(
        &self,
        kind: bookie_core::CatalogObjectKind,
        schema: Option<&str>,
        excluded_schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::CatalogObject>, DriverError> {
        stoppable!(self, control, |connection| catalog::list_objects_excluding(
            &mut connection,
            kind,
            schema,
            excluded_schemas
        ))
    }

    async fn routine_definition(
        &self,
        routine: &bookie_core::RoutineRef,
    ) -> Result<bookie_core::RoutineLookup, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        catalog::routine_definition(&mut connection, routine).await
    }

    async fn routine_definition_controlled(
        &self,
        routine: &bookie_core::RoutineRef,
        control: &OperationControl,
    ) -> Result<bookie_core::RoutineLookup, DriverError> {
        stoppable!(self, control, |connection| catalog::routine_definition(
            &mut connection,
            routine
        ))
    }

    async fn fetch_columns(&self, schema: Option<&str>, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_fetch_columns(&mut connection, schema, table).await
    }

    async fn fetch_columns_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<Vec<ColumnInfo>, DriverError> {
        stoppable!(self, control, |connection| read_fetch_columns(
            &mut connection,
            schema,
            table
        ))
    }

    async fn fetch_view_editability(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<bookie_core::ViewEditability, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        view_editability::fetch(&mut connection, schema, table).await
    }

    async fn fetch_view_editability_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<bookie_core::ViewEditability, DriverError> {
        stoppable!(self, control, |connection| view_editability::fetch(
            &mut connection,
            schema,
            table
        ))
    }

    async fn fetch_rows(
        &self,
        schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<QueryResult, DriverError> {
        stream_into_result(&self.pool, &rows_page_sql(schema, table, offset, limit), limit as usize).await
    }

    async fn fetch_rows_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        self.query_controlled(&rows_page_sql(schema, table, offset, limit), control)
            .await
    }

    async fn query(&self, sql: &str) -> Result<QueryResult, DriverError> {
        stream_into_result(&self.pool, sql, MAX_QUERY_ROWS).await
    }

    async fn query_controlled(&self, sql: &str, control: &OperationControl) -> Result<QueryResult, DriverError> {
        let connection = acquire_controlled(&self.pool, control).await?;
        let (connection, result) = controlled_query(connection, sql, &[], true, control).await;
        drop(connection);
        result
    }

    async fn query_result_sets_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        let connection = acquire_controlled(&self.pool, control).await?;
        let (connection, result) = controlled_query_with_messages(connection, sql, params, true, control).await;
        drop(connection);
        let (rows, messages, rows_affected) = result?;
        Ok(QueryResultBatch::single(rows)
            .with_messages(messages)
            .with_rows_affected(rows_affected))
    }

    async fn query_params(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        let stop = Some(QueryStop {
            cancel_token: connection.cancel_token(),
        });
        query_connection(&mut connection, sql, params, stop).await
    }

    async fn query_params_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        let connection = acquire_controlled(&self.pool, control).await?;
        let (connection, result) = controlled_query(connection, sql, params, true, control).await;
        drop(connection);
        result
    }

    async fn execute(&self, sql: &str) -> Result<ExecResult, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        execute_connection(&mut connection, sql, &[]).await
    }

    async fn execute_controlled(&self, sql: &str, control: &OperationControl) -> Result<ExecResult, DriverError> {
        let connection = acquire_controlled(&self.pool, control).await?;
        let (connection, result) = controlled_execute(connection, sql, &[], true, control).await;
        drop(connection);
        result
    }

    async fn execute_params(&self, sql: &str, params: &[Value]) -> Result<ExecResult, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        execute_connection(&mut connection, sql, params).await
    }

    async fn execute_params_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<ExecResult, DriverError> {
        let connection = acquire_controlled(&self.pool, control).await?;
        let (connection, result) = controlled_execute(connection, sql, params, true, control).await;
        drop(connection);
        result
    }

    async fn synchronize_auto_increment_sequences(
        &self,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
    ) -> Result<(), DriverError> {
        self.synchronize_auto_increment_sequences_controlled(
            schema,
            table,
            columns,
            &OperationControl::with_timeout(Duration::from_secs(5)),
        )
        .await
    }

    async fn synchronize_auto_increment_sequences_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
        control: &OperationControl,
    ) -> Result<(), DriverError> {
        for column in columns {
            let (sql, params) = sequence_sync_query(schema, table, column);
            let result = self.query_params_controlled(&sql, &params, control).await?;
            if let Some(reason) = sequence_sync_failure(&result) {
                return Err(DriverError::Unsupported(reason.into()));
            }
        }
        Ok(())
    }

    async fn execute_in_transaction(&self, statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        self.execute_in_transaction_checked(statements, &[]).await
    }

    async fn execute_in_transaction_checked(
        &self,
        statements: &[(String, Vec<Value>)],
        expect_one: &[usize],
    ) -> Result<Vec<u64>, DriverError> {
        let transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        transaction::run_checked(transaction, statements, expect_one).await
    }

    async fn execute_in_transaction_controlled(
        &self,
        statements: &[(String, Vec<Value>)],
        control: &OperationControl,
    ) -> Result<Vec<u64>, DriverError> {
        self.execute_in_transaction_checked_controlled(statements, &[], control)
            .await
    }

    async fn execute_in_transaction_checked_controlled(
        &self,
        statements: &[(String, Vec<Value>)],
        expect_one: &[usize],
        control: &OperationControl,
    ) -> Result<Vec<u64>, DriverError> {
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let cancel_token = connection.cancel_token();
        let result = run_server_cancellable_preserving_transaction_rollback(
            transaction::begin_and_run_checked(&mut connection, statements, expect_one),
            request_cancellation(cancel_token),
            transaction::confirms_stopped_statement,
            control,
        )
        .await;
        finish_connection(connection, result).await.1
    }

    async fn fetch_indexes(&self, schema: Option<&str>, table: &str) -> Result<Vec<IndexInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_fetch_indexes(&mut connection, schema, table).await
    }

    async fn fetch_indexes_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<Vec<IndexInfo>, DriverError> {
        stoppable!(self, control, |connection| read_fetch_indexes(
            &mut connection,
            schema,
            table
        ))
    }

    async fn fetch_enum_labels(&self, schema: &str, name: &str) -> Result<Vec<String>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_fetch_enum_labels(&mut connection, schema, name).await
    }

    async fn fetch_enum_labels_controlled(
        &self,
        schema: &str,
        name: &str,
        control: &OperationControl,
    ) -> Result<Vec<String>, DriverError> {
        stoppable!(self, control, |connection| read_fetch_enum_labels(
            &mut connection,
            schema,
            name
        ))
    }

    async fn fetch_foreign_keys(&self, schema: Option<&str>, table: &str) -> Result<Vec<ForeignKeyInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_fetch_foreign_keys(&mut connection, schema, table).await
    }

    async fn fetch_foreign_keys_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<Vec<ForeignKeyInfo>, DriverError> {
        stoppable!(self, control, |connection| read_fetch_foreign_keys(
            &mut connection,
            schema,
            table
        ))
    }

    async fn fetch_inbound_foreign_keys(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Vec<InboundForeignKeyInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_fetch_inbound_foreign_keys(&mut connection, schema, table).await
    }

    async fn fetch_inbound_foreign_keys_controlled(
        &self,
        schema: Option<&str>,
        table: &str,
        control: &OperationControl,
    ) -> Result<Vec<InboundForeignKeyInfo>, DriverError> {
        stoppable!(self, control, |connection| read_fetch_inbound_foreign_keys(
            &mut connection,
            schema,
            table
        ))
    }

    fn supports_server_cancellation(&self) -> bool {
        true
    }

    async fn resolve_commit(&self, token: &CommitToken) -> Result<CommitResolution, DriverError> {
        commit_resolution::resolve(&self.pool, token).await
    }

    async fn ping(&self) -> Result<(), DriverError> {
        if pool_saturation::is_saturated(&self.pool) {
            return Ok(());
        }
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn open_session(&self) -> Result<Box<dyn bookie_core::Session>, DriverError> {
        session::open(&self.session_options).await
    }

    async fn lexical_settings_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Option<LexicalSettings>, DriverError> {
        let connection = run_controlled_setup(self.pool.acquire(), control)
            .await?
            .map_err(map_sqlx_error)?;
        reported_lexical_settings(&connection).map(Some)
    }

    async fn begin(&self) -> Result<Box<dyn bookie_core::Transaction>, DriverError> {
        self.begin_controlled(&OperationControl::with_timeout(CONTROL_SETUP_TIMEOUT))
            .await
    }

    async fn begin_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Box<dyn bookie_core::Transaction>, DriverError> {
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let cancel_token = connection.cancel_token();
        let result = run_server_cancellable(
            async {
                sqlx::query("BEGIN")
                    .execute(&mut *connection)
                    .await
                    .map(|_| ())
                    .map_err(map_sqlx_error)
            },
            request_cancellation(cancel_token.clone()),
            confirms_cancellation,
            control,
        )
        .await;
        let (connection, result) = finish_connection(connection, result).await;
        result?;
        let connection =
            connection.ok_or_else(|| DriverError::Internal("PostgreSQL transaction connection was retired".into()))?;
        Ok(Box::new(PgTransaction {
            connection: Some(connection),
            cancel_token,
        }))
    }

    async fn server_version(&self) -> Result<Option<String>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_server_version(&mut connection).await
    }

    async fn server_version_controlled(&self, control: &OperationControl) -> Result<Option<String>, DriverError> {
        stoppable!(self, control, |connection| read_server_version(&mut connection))
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        self.pool.close().await;
        Ok(())
    }
}

async fn transaction_failure(
    tx: sqlx::Transaction<'_, Postgres>,
    statement_index: usize,
    source: DriverError,
) -> DriverError {
    match tx.rollback().await {
        Ok(()) => DriverError::Transaction {
            statement_index,
            source: Box::new(source),
        },
        Err(error) => DriverError::TransactionRollbackFailed {
            statement_index,
            source: Box::new(source),
            rollback_error: Box::new(map_sqlx_error(error)),
        },
    }
}

struct PgTransaction {
    connection: Option<PoolConnection<Postgres>>,
    cancel_token: PgCancelToken,
}

impl Drop for PgTransaction {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.as_mut() {
            connection.close_on_drop();
        }
    }
}

#[async_trait]
impl bookie_core::Transaction for PgTransaction {
    async fn query(&mut self, sql: &str) -> Result<QueryResult, DriverError> {
        let stop = Some(QueryStop {
            cancel_token: self.cancel_token.clone(),
        });
        let connection = self.connection_mut()?;
        query_connection_once(connection, sql, &[], stop).await
    }

    async fn query_controlled(&mut self, sql: &str, control: &OperationControl) -> Result<QueryResult, DriverError> {
        self.query_params_controlled(sql, &[], control).await
    }

    async fn query_params(&mut self, sql: &str, params: &[Value]) -> Result<QueryResult, DriverError> {
        let stop = Some(QueryStop {
            cancel_token: self.cancel_token.clone(),
        });
        let connection = self.connection_mut()?;
        query_connection_once(connection, sql, params, stop).await
    }

    async fn query_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        check_pre_dispatch(control)?;
        let connection = self.take_connection()?;
        let (connection, result) = controlled_query(
            connection, sql, params,
            // An error aborts an explicit transaction; the caller must roll it back.
            false, control,
        )
        .await;
        self.connection = connection;
        result
    }

    async fn execute(&mut self, sql: &str) -> Result<ExecResult, DriverError> {
        let connection = self.connection_mut()?;
        execute_connection_once(connection, sql, &[]).await
    }

    async fn execute_controlled(&mut self, sql: &str, control: &OperationControl) -> Result<ExecResult, DriverError> {
        self.execute_params_controlled(sql, &[], control).await
    }

    async fn execute_params(&mut self, sql: &str, params: &[Value]) -> Result<ExecResult, DriverError> {
        let connection = self.connection_mut()?;
        execute_connection_once(connection, sql, params).await
    }

    async fn execute_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<ExecResult, DriverError> {
        check_pre_dispatch(control)?;
        let connection = self.take_connection()?;
        let (connection, result) = controlled_execute(
            connection, sql, params,
            // An error aborts an explicit transaction; the caller must roll it back.
            false, control,
        )
        .await;
        self.connection = connection;
        result
    }

    async fn commit(mut self: Box<Self>) -> Result<(), DriverError> {
        let connection = self.connection_mut()?;
        sqlx::query("COMMIT")
            .execute(&mut **connection)
            .await
            .map_err(map_sqlx_error)?;
        let _ = self.take_connection()?;
        Ok(())
    }

    async fn rollback(mut self: Box<Self>) -> Result<(), DriverError> {
        let connection = self.connection_mut()?;
        sqlx::query("ROLLBACK")
            .execute(&mut **connection)
            .await
            .map_err(map_sqlx_error)?;
        let _ = self.take_connection()?;
        Ok(())
    }
}

impl PgTransaction {
    fn connection_mut(&mut self) -> Result<&mut PoolConnection<Postgres>, DriverError> {
        self.connection
            .as_mut()
            .ok_or_else(|| DriverError::Internal("transaction is unusable".into()))
    }

    fn take_connection(&mut self) -> Result<PoolConnection<Postgres>, DriverError> {
        self.connection
            .take()
            .ok_or_else(|| DriverError::Internal("transaction is unusable".into()))
    }
}

async fn acquire_controlled(
    pool: &Pool<Postgres>,
    control: &OperationControl,
) -> Result<PoolConnection<Postgres>, DriverError> {
    run_controlled_setup(pool.acquire(), control)
        .await?
        .map_err(map_sqlx_error)
}

async fn controlled_query(
    connection: PoolConnection<Postgres>,
    sql: &str,
    params: &[Value],
    retry_stale_type_cache: bool,
    control: &OperationControl,
) -> (Option<PoolConnection<Postgres>>, Result<QueryResult, DriverError>) {
    let (connection, result) =
        controlled_query_with_messages(connection, sql, params, retry_stale_type_cache, control).await;
    (connection, result.map(|(rows, _, _)| rows))
}

pub(crate) type MessagedResult = Result<(QueryResult, ServerMessages, Option<u64>), DriverError>;

async fn controlled_query_with_messages(
    mut connection: PoolConnection<Postgres>,
    sql: &str,
    params: &[Value],
    retry_stale_type_cache: bool,
    control: &OperationControl,
) -> (Option<PoolConnection<Postgres>>, MessagedResult) {
    let cancel_token = connection.cancel_token();
    let stop = Some(QueryStop {
        cancel_token: cancel_token.clone(),
    });
    notices::discard(&mut connection);
    let result = run_server_cancellable(
        async {
            let (rows, rows_affected) = if retry_stale_type_cache {
                query_connection_with_rows_affected(&mut connection, sql, params, stop).await
            } else {
                query_connection_once_with_rows_affected(&mut connection, sql, params, stop).await
            }?;
            Ok((rows, notices::drain(&mut connection), rows_affected))
        },
        request_cancellation(cancel_token),
        confirms_cancellation,
        control,
    )
    .await;
    finish_connection(connection, result).await
}

async fn controlled_execute(
    mut connection: PoolConnection<Postgres>,
    sql: &str,
    params: &[Value],
    retry_stale_type_cache: bool,
    control: &OperationControl,
) -> (Option<PoolConnection<Postgres>>, Result<ExecResult, DriverError>) {
    let cancel_token = connection.cancel_token();
    let result = run_server_cancellable(
        async {
            if retry_stale_type_cache {
                execute_connection(&mut connection, sql, params).await
            } else {
                execute_connection_once(&mut connection, sql, params).await
            }
        },
        request_cancellation(cancel_token),
        confirms_cancellation,
        control,
    )
    .await;
    finish_connection(connection, result).await
}

fn confirms_cancellation(error: &DriverError) -> bool {
    error.has_sqlstate("57014")
}

fn sequence_sync_query(schema: Option<&str>, table: &str, column: &str) -> (String, Vec<Value>) {
    let target = qualified(schema, table);
    let column_ident = quote_ident(column);
    let sql = format!(
        "WITH seq AS (
             SELECT pg_catalog.pg_get_serial_sequence($1, $2)::regclass AS name
         ), sequence_state AS (
             SELECT seq.name, s.seqincrement, s.seqstart, s.seqcycle,
                    pg_catalog.pg_sequence_last_value(seq.name) AS last_value
             FROM seq
             LEFT JOIN pg_catalog.pg_sequence s ON s.seqrelid = seq.name
         ), table_values AS (
             SELECT (SELECT MAX({column_ident}) FROM {target}) AS high,
                    (SELECT MIN({column_ident}) FROM {target}) AS low
         )
         SELECT sequence_state.name IS NULL,
                sequence_state.seqcycle,
                CASE WHEN sequence_state.name IS NULL THEN NULL::bigint
                     WHEN sequence_state.seqincrement > 0
                     THEN pg_catalog.setval(
                         sequence_state.name,
                         GREATEST(COALESCE(sequence_state.last_value, sequence_state.seqstart), table_values.high),
                         true
                     )
                     ELSE pg_catalog.setval(
                         sequence_state.name,
                         LEAST(COALESCE(sequence_state.last_value, sequence_state.seqstart), table_values.low),
                         true
                     )
                END
         FROM sequence_state CROSS JOIN table_values
         WHERE sequence_state.name IS NULL
            OR CASE WHEN sequence_state.seqincrement > 0
                    THEN table_values.high IS NOT NULL
                    ELSE table_values.low IS NOT NULL
               END"
    );
    (sql, vec![Value::Text(target), Value::Text(column.to_owned())])
}

fn sequence_sync_failure(result: &QueryResult) -> Option<&'static str> {
    if result
        .rows
        .iter()
        .any(|row| matches!(row.first(), Some(Value::Bool(true))))
    {
        return Some("the column has no owned sequence");
    }
    result
        .rows
        .iter()
        .any(|row| matches!(row.get(1), Some(Value::Bool(true))))
        .then_some("the sequence is cyclic")
}

async fn finish_connection<T>(
    connection: PoolConnection<Postgres>,
    result: Result<T, DriverError>,
) -> (Option<PoolConnection<Postgres>>, Result<T, DriverError>) {
    if matches!(result, Err(DriverError::OperationOutcomeUnknown { .. })) {
        let raw_connection = connection.detach();
        let _ = bounded_cleanup(CANCELLATION_DISPATCH_TIMEOUT, raw_connection.close_hard()).await;
        return (None, result);
    }
    (Some(connection), result)
}

async fn bounded_cleanup<F: Future>(timeout: Duration, cleanup: F) -> bool {
    tokio::time::timeout(timeout, cleanup).await.is_ok()
}

pub(crate) async fn request_cancellation(cancel_token: PgCancelToken) -> Result<(), DriverError> {
    tokio::time::timeout(CANCELLATION_DISPATCH_TIMEOUT, cancel_token.cancel())
        .await
        .map_err(|_| DriverError::TimedOut)?
        .map_err(map_sqlx_error)
}

fn rows_page_sql(schema: Option<&str>, table: &str, offset: u64, limit: u64) -> String {
    bookie_core::sql_dialect::rows_page_sql("postgres", schema, table, offset, limit)
}

fn quote_ident(name: &str) -> String {
    bookie_core::sql_dialect::quote_ident("postgres", name)
}

fn normalize_pg_default(raw: String) -> String {
    match raw.rsplit_once("::") {
        Some((operand, _)) if is_plain_literal(operand) => operand.to_string(),
        _ => raw,
    }
}

fn is_plain_literal(operand: &str) -> bool {
    is_quoted_literal(operand) || operand.eq_ignore_ascii_case("null") || operand.parse::<f64>().is_ok()
}

fn is_quoted_literal(operand: &str) -> bool {
    let Some(inner) = operand.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\'')) else {
        return false;
    };
    !inner.replace("''", "").contains('\'')
}

fn qualified(schema: Option<&str>, table: &str) -> String {
    bookie_core::sql_dialect::qualified_table("postgres", schema, table)
}

/// Map `pg_constraint.confdeltype` / `confupdtype` single-char codes
/// to canonical SQL action keywords. Returns `None` for the default
/// "no action" so the FK builder can omit the redundant ON clause.
fn pg_action_char_to_keyword(code: &str) -> Option<String> {
    match code {
        "r" => Some("RESTRICT".into()),
        "c" => Some("CASCADE".into()),
        "n" => Some("SET NULL".into()),
        "d" => Some("SET DEFAULT".into()),
        // 'a' = NO ACTION is the default; surface as None so the
        // generated DDL stays clean.
        _ => None,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Reads the string rules from the parameter status the server reports on every change,
/// so it needs no query and still answers inside an aborted transaction.
fn reported_lexical_settings(connection: &sqlx::PgConnection) -> Result<LexicalSettings, DriverError> {
    let value = connection
        .parameter_status("standard_conforming_strings")
        .ok_or_else(|| DriverError::Unsupported("the server did not report standard_conforming_strings".into()))?;
    LexicalSettings::from_postgres_standard_conforming_strings(value)
        .ok_or_else(|| DriverError::Unsupported(format!("standard_conforming_strings has an unknown value {value:?}")))
}
