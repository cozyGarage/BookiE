use async_trait::async_trait;
use secrecy::ExposeSecret;
use sqlparser::ast::Statement as SqlStatement;
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;
use sqlx::mysql::{MySql, MySqlConnectOptions, MySqlPoolOptions, MySqlRow};
use sqlx::pool::PoolConnection;
use sqlx::{Connection as SqlxConnection, Executor, Pool, Row, SqlSafeStr, Statement};

use futures::stream::StreamExt;

mod attributes;
mod catalog;
mod decode;
mod errors;
mod introspection;
mod spatial;
use decode::extract_value;
mod packed;
mod pool_saturation;
mod session;
mod text_protocol;
mod transaction;
mod warnings;
pub(crate) use text_protocol::{contains_use_statement, is_call_statement, text_protocol};
use text_protocol::{mysql_row_payload_bytes, statement_columns};
mod temporal;
mod view_editability;

use bookie_core::sql_syntax::script::LexicalSettings;
use bookie_core::{
    ColumnInfo, ConnectOptions, Connection, DatabaseDriver, DriverError, DriverValueInputCapabilities, EnumInputStyle,
    ExecResult, ForeignKeyInfo, InboundForeignKeyInfo, IndexInfo, MAX_QUERY_ROWS, OperationControl, QueryResult,
    QueryResultBatch, QueryResultBudget, QueryRows, TableInfo, Transport, Value, check_pre_dispatch,
    run_controlled_setup, run_server_cancellable, run_server_cancellable_preserving_transaction_rollback,
};
use errors::*;
use introspection::*;

/// Name of the SSH-forwarded rendezvous socket inside the private directory
/// `SshTunnel::open_chain_socket` creates. Not a real mysqld socket; just a
/// name shared between `forwarded_socket_name` (which tells the transport
/// layer to forward one) and `connect` (which dials it).
const FORWARDED_SOCKET_NAME: &str = "mysqld.sock";

#[cfg(test)]
#[path = "mysql_socket_tests.rs"]
mod mysql_socket_tests;

pub struct MysqlDriver;

#[async_trait]
impl DatabaseDriver for MysqlDriver {
    fn value_input_capabilities(&self) -> DriverValueInputCapabilities {
        DriverValueInputCapabilities {
            integer_range_validation: true,
            spatial_edit_requires_native_value: true,
            ..DriverValueInputCapabilities::default()
        }
    }

    fn enum_input_style(&self) -> EnumInputStyle {
        EnumInputStyle::DeclaredMemberList
    }

    fn supports_database_listing(&self) -> bool {
        true
    }

    fn id(&self) -> &'static str {
        "mysql"
    }

    fn supports_value_search(&self) -> bool {
        true
    }
    fn supports_reverse_keyset(&self) -> bool {
        true
    }

    fn import_upsert(&self) -> bookie_core::ImportUpsertStyle {
        bookie_core::ImportUpsertStyle::OnDuplicateKey
    }

    fn display_name(&self) -> &'static str {
        "MySQL"
    }

    fn default_port(&self) -> u16 {
        3306
    }

    fn default_database(&self) -> &'static str {
        "mysql"
    }

    fn default_username(&self) -> &'static str {
        "root"
    }

    fn supports_index_metadata(&self) -> bool {
        true
    }

    fn supports_foreign_key_metadata(&self) -> bool {
        true
    }

    fn forwarded_socket_name(&self, _service_port: u16) -> Option<String> {
        Some(FORWARDED_SOCKET_NAME.to_string())
    }

    fn supports_client_tls_auth(&self) -> bool {
        true
    }

    fn supports_table_default_collation(&self) -> bool {
        true
    }

    fn supports_explicit_auto_increment_values(&self) -> bool {
        true
    }

    fn supports_local_socket(&self) -> bool {
        true
    }

    fn supports_local_socket_file(&self) -> bool {
        true
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        let mysql_opts = mysql_connect_options(&opts);
        let cancellation_options = mysql_opts.clone();
        let session_options = mysql_opts.clone();
        let read_only = opts.read_only;
        let database: std::sync::Arc<str> = opts.database.as_str().into();
        let acquire_timeout = opts.pool_acquire_timeout();
        let pool = MySqlPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(acquire_timeout)
            .after_connect(move |connection, _| {
                Box::pin(async move {
                    let server_read_only = prepare_session(connection, read_only).await?;
                    report_read_only_setup(read_only, server_read_only);
                    Ok(())
                })
            })
            .before_acquire(move |connection, _| {
                let database = std::sync::Arc::clone(&database);
                Box::pin(async move {
                    prepare_session(connection, read_only).await?;
                    restore_database(connection, &database).await?;
                    Ok(true)
                })
            })
            .connect_with(mysql_opts)
            .await
            .map_err(map_sqlx_connect_error)?;
        // Lazy: a saved connection with a failed-login lockout policy
        // must not see two authentication attempts for one Connect
        // click. The real dial happens only when a cancel is actually
        // requested.
        let cancellation_pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(acquire_timeout)
            .connect_lazy_with(cancellation_options);
        Ok(Box::new(MysqlConnection {
            pool,
            cancellation_pool,
            session_options,
            read_only,
        }))
    }
}

macro_rules! stoppable {
    ($self:ident, $control:ident, |$connection:ident| $statement:expr) => {{
        let mut $connection = acquire_controlled(&$self.pool, $control).await?;
        let connection_id = connection_id_controlled(&mut $connection, $control).await?;
        let result = run_server_cancellable(
            $statement,
            request_cancellation(&$self.cancellation_pool, connection_id),
            confirms_cancellation,
            $control,
        )
        .await;
        release_connection($connection, &result).await;
        result
    }};
}

struct MysqlConnection {
    pool: Pool<MySql>,
    cancellation_pool: Pool<MySql>,
    session_options: MySqlConnectOptions,
    read_only: bool,
}

impl MysqlConnection {
    fn transaction_on(&self, connection: PoolConnection<MySql>, connection_id: u64) -> MysqlTransaction {
        MysqlTransaction {
            connection: Some(connection),
            connection_id,
            cancellation_pool: self.cancellation_pool.clone(),
        }
    }
}

#[async_trait]
impl Connection for MysqlConnection {
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
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        catalog::list_relations(&mut connection, None).await
    }

    async fn list_tables_controlled(&self, control: &OperationControl) -> Result<Vec<TableInfo>, DriverError> {
        stoppable!(self, control, |connection| catalog::list_relations(
            &mut connection,
            None
        ))
    }

    async fn list_tables_in(
        &self,
        schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let mut tables = self.list_tables_controlled(control).await?;
        tables.retain(|table| table.schema.as_ref().is_none_or(|schema| schemas.contains(schema)));
        Ok(tables)
    }

    async fn list_tables_excluding_controlled(
        &self,
        schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let mut tables = self.list_tables_controlled(control).await?;
        tables.retain(|table| table.schema.as_ref().is_none_or(|schema| !schemas.contains(schema)));
        Ok(tables)
    }

    async fn list_views(&self) -> Result<Vec<TableInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        catalog::list_relations(&mut connection, Some("VIEW")).await
    }

    async fn list_views_controlled(&self, control: &OperationControl) -> Result<Vec<TableInfo>, DriverError> {
        stoppable!(self, control, |connection| catalog::list_relations(
            &mut connection,
            Some("VIEW")
        ))
    }

    async fn list_views_excluding_controlled(
        &self,
        schemas: &[String],
        control: &OperationControl,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let mut views = self.list_views_controlled(control).await?;
        views.retain(|view| view.schema.as_ref().is_none_or(|schema| !schemas.contains(schema)));
        Ok(views)
    }

    async fn fetch_columns(&self, schema: Option<&str>, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        read_fetch_columns(&mut connection, schema, table).await
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

    async fn fetch_rows(
        &self,
        schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<QueryResult, DriverError> {
        let sql = rows_page_sql(schema, table, offset, limit);
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        params_into_result(&mut connection, &sql, &[], limit as usize).await
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
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        params_into_result(&mut connection, sql, &[], MAX_QUERY_ROWS).await
    }

    async fn query_params(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        params_into_result(&mut connection, sql, params, MAX_QUERY_ROWS).await
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
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let connection_id = connection_id_controlled(&mut connection, control).await?;
        let result = session::query_with_text_fallback(
            &mut connection,
            &self.cancellation_pool,
            connection_id,
            sql,
            params,
            control,
        )
        .await;
        release_connection(connection, &result).await;
        result.map(QueryResultBatch::into_first)
    }

    async fn query_result_sets_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let connection_id = connection_id_controlled(&mut connection, control).await?;
        let result = session::query_with_text_fallback(
            &mut connection,
            &self.cancellation_pool,
            connection_id,
            sql,
            params,
            control,
        )
        .await;
        release_connection(connection, &result).await;
        result
    }

    async fn execute(&self, sql: &str) -> Result<ExecResult, DriverError> {
        let res = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(ExecResult {
            rows_affected: res.rows_affected(),
        })
    }

    async fn execute_params(&self, sql: &str, params: &[Value]) -> Result<ExecResult, DriverError> {
        let q = bind_mysql_params(sqlx::query(sqlx::AssertSqlSafe(sql)), params)?;
        let res = q.execute(&self.pool).await.map_err(map_sqlx_error)?;
        Ok(ExecResult {
            rows_affected: res.rows_affected(),
        })
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
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let connection_id = connection_id_controlled(&mut connection, control).await?;
        let result = run_server_cancellable(
            execute_on(&mut *connection, sql, params),
            request_cancellation(&self.cancellation_pool, connection_id),
            confirms_cancellation,
            control,
        )
        .await;
        release_connection(connection, &result).await;
        result
    }

    fn supports_server_cancellation(&self) -> bool {
        true
    }

    async fn execute_in_transaction(&self, statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        self.execute_in_transaction_checked(statements, &[]).await
    }

    async fn execute_in_transaction_checked(
        &self,
        statements: &[(String, Vec<Value>)],
        expect_one: &[usize],
    ) -> Result<Vec<u64>, DriverError> {
        validate_atomic_batch(statements)?;
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
        validate_atomic_batch(statements)?;
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let connection_id = connection_id_controlled(&mut connection, control).await?;
        let result = run_server_cancellable_preserving_transaction_rollback(
            transaction::begin_and_run_checked(&mut connection, statements, expect_one),
            request_cancellation(&self.cancellation_pool, connection_id),
            transaction::confirms_stopped_statement,
            control,
        )
        .await;
        release_connection(connection, &result).await;
        result
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
        session::open(&self.session_options, self.cancellation_pool.clone(), self.read_only).await
    }

    async fn lexical_settings_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Option<LexicalSettings>, DriverError> {
        stoppable!(self, control, |connection| session_lexical_settings(&mut *connection)).map(Some)
    }

    async fn begin(&self) -> Result<Box<dyn bookie_core::Transaction>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        let connection_id = connection_id(&mut connection).await?;
        transaction::statement(&mut connection, "BEGIN").await?;
        Ok(Box::new(self.transaction_on(connection, connection_id)))
    }

    async fn begin_controlled(
        &self,
        control: &OperationControl,
    ) -> Result<Box<dyn bookie_core::Transaction>, DriverError> {
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let connection_id = connection_id_controlled(&mut connection, control).await?;
        let result = run_server_cancellable(
            transaction::statement(&mut connection, "BEGIN"),
            request_cancellation(&self.cancellation_pool, connection_id),
            confirms_cancellation,
            control,
        )
        .await;
        match result {
            Ok(()) => Ok(Box::new(self.transaction_on(connection, connection_id))),
            Err(error) => Err(retire_if_uncertain(connection, error).await),
        }
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

fn validate_atomic_batch(statements: &[(String, Vec<Value>)]) -> Result<(), DriverError> {
    let dialect = MySqlDialect {};
    for (index, (sql, _)) in statements.iter().enumerate() {
        let parsed = Parser::parse_sql(&dialect, sql).map_err(|_| atomic_batch_refusal(index))?;
        let [statement] = parsed.as_slice() else {
            return Err(atomic_batch_refusal(index));
        };
        if !matches!(
            statement,
            SqlStatement::Insert(_) | SqlStatement::Update { .. } | SqlStatement::Delete(_)
        ) {
            return Err(atomic_batch_refusal(index));
        }
    }
    Ok(())
}

fn atomic_batch_refusal(statement_index: usize) -> DriverError {
    DriverError::Unsupported(format!(
        "statement {} cannot be guaranteed atomic on MySQL",
        statement_index + 1
    ))
}

struct MysqlTransaction {
    connection: Option<PoolConnection<MySql>>,
    connection_id: u64,
    cancellation_pool: Pool<MySql>,
}

impl Drop for MysqlTransaction {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.as_mut() {
            connection.close_on_drop();
        }
    }
}

impl MysqlTransaction {
    fn open(&mut self) -> Result<&mut PoolConnection<MySql>, DriverError> {
        self.connection
            .as_mut()
            .ok_or_else(|| DriverError::Internal("transaction is unusable".into()))
    }

    async fn retire_if_uncertain<T>(&mut self, result: &Result<T, DriverError>) {
        if matches!(result, Err(DriverError::OperationOutcomeUnknown { .. }))
            && let Some(connection) = self.connection.take()
        {
            let _ = connection.detach().close_hard().await;
        }
    }

    async fn end(&mut self, sql: &'static str) -> Result<(), DriverError> {
        let connection = self.open()?;
        transaction::statement(connection, sql).await?;
        self.connection = None;
        Ok(())
    }
}

async fn transaction_query(
    connection: &mut sqlx::MySqlConnection,
    sql: &str,
    params: &[Value],
) -> Result<QueryResult, DriverError> {
    let timestamp_is_utc = timezone_is_utc(&mut *connection).await?;
    let mut result = params_into_result(connection, sql, params, MAX_QUERY_ROWS).await?;
    if !timestamp_is_utc {
        refuse_non_utc_timestamps(&mut result);
    }
    Ok(result)
}

#[async_trait]
impl bookie_core::Transaction for MysqlTransaction {
    async fn query(&mut self, sql: &str) -> Result<QueryResult, DriverError> {
        transaction_query(self.open()?, sql, &[]).await
    }

    async fn query_controlled(&mut self, sql: &str, control: &OperationControl) -> Result<QueryResult, DriverError> {
        self.query_params_controlled(sql, &[], control).await
    }

    async fn query_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        let (cancellation_pool, connection_id) = (self.cancellation_pool.clone(), self.connection_id);
        let result = run_server_cancellable(
            transaction_query(self.open()?, sql, params),
            request_cancellation(&cancellation_pool, connection_id),
            confirms_cancellation,
            control,
        )
        .await;
        self.retire_if_uncertain(&result).await;
        result
    }

    async fn execute(&mut self, sql: &str) -> Result<ExecResult, DriverError> {
        execute_on(&mut **self.open()?, sql, &[]).await
    }

    async fn execute_controlled(&mut self, sql: &str, control: &OperationControl) -> Result<ExecResult, DriverError> {
        self.execute_params_controlled(sql, &[], control).await
    }

    async fn execute_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<ExecResult, DriverError> {
        let (cancellation_pool, connection_id) = (self.cancellation_pool.clone(), self.connection_id);
        let result = run_server_cancellable(
            execute_on(&mut **self.open()?, sql, params),
            request_cancellation(&cancellation_pool, connection_id),
            confirms_cancellation,
            control,
        )
        .await;
        self.retire_if_uncertain(&result).await;
        result
    }

    async fn commit(mut self: Box<Self>) -> Result<(), DriverError> {
        self.end("COMMIT").await
    }

    async fn rollback(mut self: Box<Self>) -> Result<(), DriverError> {
        self.end("ROLLBACK").await
    }

    async fn rollback_controlled(mut self: Box<Self>, control: &OperationControl) -> Result<(), DriverError> {
        let (cancellation_pool, connection_id) = (self.cancellation_pool.clone(), self.connection_id);
        let result = run_server_cancellable(
            transaction::statement(self.open()?, "ROLLBACK"),
            request_cancellation(&cancellation_pool, connection_id),
            confirms_cancellation,
            control,
        )
        .await;
        if result.is_ok() {
            self.connection = None;
        }
        result
    }
}

async fn set_utc_timezone(connection: &mut sqlx::MySqlConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SET time_zone = '+00:00'").execute(connection).await?;
    Ok(())
}

async fn restore_database(connection: &mut sqlx::MySqlConnection, database: &str) -> Result<(), sqlx::Error> {
    if database.is_empty() {
        return Ok(());
    }
    let statement = format!("USE `{}`", database.replace('`', "``"));
    sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
        .execute(&mut *connection)
        .await?;
    connection.clear_cached_statements().await
}

pub(crate) async fn forget_statements_after_use(
    connection: &mut sqlx::MySqlConnection,
    sql: &str,
) -> Result<(), sqlx::Error> {
    if contains_use_statement(sql) {
        connection.clear_cached_statements().await?;
    }
    Ok(())
}

async fn prepare_session(connection: &mut sqlx::MySqlConnection, read_only: bool) -> Result<bool, sqlx::Error> {
    set_utc_timezone(connection).await?;
    if !read_only {
        return Ok(true);
    }
    match sqlx::query("SET SESSION TRANSACTION READ ONLY")
        .execute(connection)
        .await
    {
        Ok(_) => Ok(true),
        Err(error) if is_unsupported_read_only_transaction_error(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

fn is_unsupported_read_only_transaction_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>())
        .is_some_and(|error| unsupported_read_only_transaction_error_number(error.number()))
}

fn unsupported_read_only_transaction_error_number(number: u16) -> bool {
    matches!(number, 1064 | 1193)
}

fn report_read_only_setup(read_only: bool, server_read_only: bool) {
    if read_only && !server_read_only {
        tracing::warn!(
            server_read_only = false,
            "MySQL server lacks session-level read-only transactions; PolicyGuard enforces this connection's read-only profile"
        );
    }
}

async fn session_lexical_settings<'e, E>(executor: E) -> Result<LexicalSettings, DriverError>
where
    E: sqlx::Executor<'e, Database = MySql>,
{
    let sql_mode: String = sqlx::query_scalar("SELECT @@SESSION.sql_mode")
        .fetch_one(executor)
        .await
        .map_err(map_sqlx_error)?;
    Ok(LexicalSettings::from_mysql_sql_mode(&sql_mode))
}

async fn timezone_is_utc<'e, E>(executor: E) -> Result<bool, DriverError>
where
    E: sqlx::Executor<'e, Database = MySql>,
{
    let timezone: String = sqlx::query_scalar("SELECT @@session.time_zone")
        .fetch_one(executor)
        .await
        .map_err(map_sqlx_error)?;
    Ok(timezone == "+00:00")
}

fn refuse_non_utc_timestamps(result: &mut QueryResult) {
    for (column_index, column) in result.columns.iter().enumerate() {
        if column.data_type.eq_ignore_ascii_case("TIMESTAMP") {
            for row in &mut result.rows {
                if !matches!(row.get(column_index), Some(Value::Null)) {
                    row[column_index] = Value::Undecodable("TIMESTAMP (session time zone is not UTC)".into());
                }
            }
        }
    }
}

async fn params_into_result(
    connection: &mut sqlx::MySqlConnection,
    sql: &str,
    params: &[Value],
    limit: usize,
) -> Result<QueryResult, DriverError> {
    let statement = connection
        .prepare(sqlx::AssertSqlSafe(sql).into_sql_str())
        .await
        .map_err(map_sqlx_error)?;
    let mut columns = statement_columns(statement.columns());
    let query = bind_mysql_params(sqlx::query(sqlx::AssertSqlSafe(sql)), params)?;
    let mut stream = query.fetch(&mut *connection);
    let mut rows = QueryRows::with_capacity(limit.min(1024), 0);
    let mut decoded = Vec::with_capacity(columns.len());
    let mut budget = QueryResultBudget::default();
    let mut truncated = false;
    while let Some(row_result) = stream.next().await {
        let row = row_result.map_err(map_sqlx_error)?;
        if rows.len() >= limit {
            truncated = true;
            break;
        }
        if columns.is_empty() {
            columns = statement_columns(row.columns());
        }
        if !budget.decode_and_admit_row_into(row.len(), mysql_row_payload_bytes(&row), &mut decoded, |values| {
            values.extend((0..columns.len()).map(|index| extract_value(&row, index)));
            Ok(())
        })? {
            truncated = true;
            break;
        }
        rows.push_row(&mut decoded);
    }
    drop(stream);
    forget_statements_after_use(&mut *connection, sql)
        .await
        .map_err(map_sqlx_error)?;
    Ok(QueryResult {
        columns,
        rows,
        truncated,
    })
}

async fn execute_on<'e, E>(executor: E, sql: &str, params: &[Value]) -> Result<ExecResult, DriverError>
where
    E: sqlx::Executor<'e, Database = MySql>,
{
    let query = bind_mysql_params(sqlx::query(sqlx::AssertSqlSafe(sql)), params)?;
    let result = query.execute(executor).await.map_err(map_sqlx_error)?;
    Ok(ExecResult {
        rows_affected: result.rows_affected(),
    })
}

fn rows_page_sql(schema: Option<&str>, table: &str, offset: u64, limit: u64) -> String {
    bookie_core::sql_dialect::rows_page_sql("mysql", schema, table, offset, limit)
}

async fn acquire_controlled(
    pool: &Pool<MySql>,
    control: &OperationControl,
) -> Result<PoolConnection<MySql>, DriverError> {
    check_pre_dispatch(control)?;
    run_controlled_setup(pool.acquire(), control)
        .await?
        .map_err(map_sqlx_error)
}

async fn connection_id_controlled(
    connection: &mut PoolConnection<MySql>,
    control: &OperationControl,
) -> Result<u64, DriverError> {
    run_controlled_setup(connection_id(connection), control).await?
}

async fn connection_id(connection: &mut PoolConnection<MySql>) -> Result<u64, DriverError> {
    sqlx::query_scalar("SELECT CONNECTION_ID()")
        .fetch_one(&mut **connection)
        .await
        .map_err(map_sqlx_error)
}

/// MySQL has no request-scoped cancel channel, so the statement is
/// stopped by `KILL QUERY` from a second session. That aborts the
/// running statement and leaves the session alive, which is what makes
/// the connection reusable afterwards.
///
/// `run_server_cancellable` wraps this future in its own dispatch
/// timeout and moves on without it if that elapses. The KILL QUERY
/// round trip runs in a spawned task instead of inline so giving up on
/// it here can never drop it mid-response and return a
/// protocol-desynced connection to the single-slot cancellation pool --
/// the task always finishes and explicitly closes the connection on
/// failure rather than trusting an implicit pool return.
async fn request_cancellation(pool: &Pool<MySql>, connection_id: u64) -> Result<(), DriverError> {
    let pool = pool.clone();
    let task = tokio::spawn(async move {
        let mut conn = pool.acquire().await.map_err(map_sqlx_error)?;
        let outcome = sqlx::query(sqlx::AssertSqlSafe(format!("KILL QUERY {connection_id}")))
            .execute(&mut *conn)
            .await
            .map_err(map_sqlx_error);
        if outcome.is_err() {
            let _ = conn.detach().close_hard().await;
        }
        outcome.map(|_| ())
    });
    match task.await {
        Ok(result) => result,
        Err(_) => Err(DriverError::Cancelled),
    }
}

/// `ER_QUERY_INTERRUPTED` is the only outcome that proves the server
/// aborted the statement rather than the client giving up on it.
fn confirms_cancellation(error: &DriverError) -> bool {
    error.has_sqlstate("70100")
}

async fn retire_if_uncertain(connection: PoolConnection<MySql>, error: DriverError) -> DriverError {
    if matches!(error, DriverError::OperationOutcomeUnknown { .. }) {
        let _ = connection.detach().close_hard().await;
    }
    error
}

async fn release_connection<T>(connection: PoolConnection<MySql>, result: &Result<T, DriverError>) {
    if matches!(result, Err(DriverError::OperationOutcomeUnknown { .. })) {
        let _ = connection.detach().close_hard().await;
    }
}

fn bind_mysql_params<'q>(
    mut q: sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments>,
    params: &'q [Value],
) -> Result<sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments>, DriverError> {
    for p in params {
        q = match p {
            Value::Null => q.bind(Option::<&str>::None),
            Value::Bool(b) => q.bind(*b),
            Value::Int(i) => q.bind(*i),
            Value::Float(f) => q.bind(*f),
            Value::Text(s) => q.bind(s.clone()),
            Value::JsonText(s) => q.bind(s.clone()),
            Value::Bytes(b) => q.bind(b.clone()),
            Value::Date(d) => q.bind(*d),
            Value::Time(t) => q.bind(*t),
            Value::DateTime(dt) => q.bind(*dt),
            Value::TimestampTz(ts) => q.bind(*ts),
            Value::Decimal(d) => q.bind(*d),
            Value::Uuid(u) => q.bind(u.to_string()),
            Value::Json(j) => q.bind(j.clone()),
            Value::Undecodable(_) => {
                return Err(DriverError::Unsupported(
                    "undecodable cell cannot be bound as a parameter".into(),
                ));
            }
        };
    }
    Ok(q)
}

/// Dial through the SSH-forwarded socket when there is one, keeping the real
/// service hostname as the TLS identity sqlx verifies against; otherwise
/// dial and verify the same TCP host, exactly as an untunnelled connection
/// already did.
fn mysql_connect_options(opts: &ConnectOptions) -> MySqlConnectOptions {
    use bookie_core::TlsMode;
    let mut mysql_opts = match opts.transport() {
        Transport::Tcp { host, port } => MySqlConnectOptions::new().host(host).port(port),
        Transport::LocalSocketFile { path } => MySqlConnectOptions::new().socket(path),
        Transport::Socket {
            directory,
            identity_host,
            identity_port,
            ..
        } => MySqlConnectOptions::new()
            .host(identity_host)
            .port(identity_port)
            .socket(directory.join(FORWARDED_SOCKET_NAME)),
    };
    mysql_opts = mysql_opts
        .database(&opts.database)
        .username(&opts.username)
        .ssl_mode(match opts.tls.mode {
            TlsMode::Disabled => sqlx::mysql::MySqlSslMode::Disabled,
            TlsMode::Prefer => sqlx::mysql::MySqlSslMode::Preferred,
            TlsMode::Require => sqlx::mysql::MySqlSslMode::Required,
            TlsMode::VerifyCa => sqlx::mysql::MySqlSslMode::VerifyCa,
            TlsMode::VerifyFull => sqlx::mysql::MySqlSslMode::VerifyIdentity,
        });
    if !opts.password.expose_secret().is_empty() {
        mysql_opts = mysql_opts.password(opts.password.expose_secret());
    }
    if let Some(path) = &opts.tls.root_cert {
        mysql_opts = mysql_opts.ssl_ca(path);
    }
    if opts.tls.mode != TlsMode::Disabled {
        if let Some(path) = &opts.tls.client_cert {
            mysql_opts = mysql_opts.ssl_client_cert(path);
        }
        if let Some(path) = &opts.tls.client_key {
            mysql_opts = mysql_opts.ssl_client_key(path);
        }
    }
    mysql_opts
}

const UNQUOTED_DEFAULT_TYPE_PREFIXES: [&str; 12] = [
    "tinyint",
    "smallint",
    "mediumint",
    "int",
    "bigint",
    "decimal",
    "numeric",
    "float",
    "double",
    "real",
    "bit",
    "year",
];

async fn index_key_expression(
    connection: &mut sqlx::MySqlConnection,
    schema: Option<&str>,
    table: &str,
    index: &str,
    seq_in_index: i64,
) -> Result<String, DriverError> {
    let expression: Option<String> = sqlx::query_scalar(
        "SELECT CAST(expression AS CHAR)
        FROM information_schema.statistics
        WHERE table_schema = COALESCE(?, DATABASE())
          AND table_name = ? AND index_name = ? AND seq_in_index = ?",
    )
    .bind(schema)
    .bind(table)
    .bind(index)
    .bind(seq_in_index)
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sqlx_error)?
    .flatten();
    Ok(format!("({})", expression.unwrap_or_else(|| "expression".to_string())))
}

// MySQL reports a literal default in information_schema.column_default as
// its bare value (`pending`, an empty string for ''), which is not a valid
// DEFAULT clause. MySQL 8 flags an expression default with
// DEFAULT_GENERATED in `extra`; 5.7 reports CURRENT_TIMESTAMP bare on a
// temporal column. Anything else on a non-numeric column is a string
// literal and is quoted back so restating the column keeps its default.
// MariaDB 10.2.7+ already reports the DEFAULT clause itself and reports a
// column without a default as the text NULL.
fn default_expression(raw: Option<String>, column_type: &str, extra: &str, mariadb: bool) -> Option<String> {
    let raw = attributes::with_on_update(raw?, column_type, extra);
    if mariadb {
        return (!raw.eq_ignore_ascii_case("null")).then_some(raw);
    }
    let lower_type = column_type.to_ascii_lowercase();
    let numeric = UNQUOTED_DEFAULT_TYPE_PREFIXES
        .iter()
        .any(|prefix| lower_type.starts_with(prefix));
    let temporal_now = (lower_type.starts_with("timestamp") || lower_type.starts_with("datetime"))
        && raw.to_ascii_lowercase().starts_with("current_timestamp");
    if numeric || temporal_now || extra.contains("default_generated") {
        return Some(raw);
    }
    bookie_core::sql_literal::render_sql_literal("mysql", &Value::Text(raw)).ok()
}

fn row_to_column_info(r: &MySqlRow) -> ColumnInfo {
    let extra = r.try_get::<String, _>(4).unwrap_or_default().to_ascii_lowercase();
    let column_type = attributes::with_invisible(r.get::<String, _>(1), &extra);
    let mariadb = r.try_get::<i64, _>(9).unwrap_or(0) == 1;
    let default_value = default_expression(
        r.try_get::<Option<String>, _>(5).unwrap_or(None),
        &column_type,
        &extra,
        mariadb,
    );
    let generation_expr: Option<String> = r.try_get::<Option<String>, _>(6).unwrap_or(None);
    let column_type = attributes::with_generation(column_type, generation_expr.as_deref(), &extra);
    ColumnInfo {
        name: r.get::<String, _>(0),
        data_type: column_type,
        nullable: r.get::<String, _>(2) == "YES",
        primary_key: r.get::<String, _>(3) == "PRI",
        is_auto_increment: extra.contains("auto_increment"),
        default_value,
        // Two false-positives to guard against:
        //   1. MySQL 8.0.13+ marks expression-default columns
        //      (e.g. DEFAULT CURRENT_TIMESTAMP) with extra =
        //      "DEFAULT_GENERATED" — contains "generated" but
        //      not a generated column. Match the explicit
        //      keywords instead.
        //   2. information_schema.generation_expression returns
        //      an *empty string* for non-generated columns,
        //      not NULL — so `generation_expr.is_some()` is
        //      true even for plain columns. Check non-empty.
        is_generated: generation_expr.as_deref().is_some_and(|s| !s.is_empty())
            || extra.contains("virtual generated")
            || extra.contains("stored generated"),
        // information_schema.column_comment is an empty
        // string, never NULL, for a column with no comment.
        comment: r
            .try_get::<Option<String>, _>(7)
            .unwrap_or(None)
            .filter(|s| !s.is_empty()),
        collation: r
            .try_get::<Option<String>, _>(8)
            .unwrap_or(None)
            .filter(|c| !c.is_empty()),
        enum_type: None,
        domain_type: None,
    }
}

const PLAINTEXT_PASSWORD_REFUSAL: &str = "refusing to send the password without TLS";

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
