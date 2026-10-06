use std::time::Duration;
use std::{collections::HashMap, iter};

use async_trait::async_trait;
use secrecy::ExposeSecret;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgRow, PgTypeInfo, PgTypeKind};
use sqlx::{
    Column, Connection as SqlxConnection, Executor, Pool, Postgres, Row, SqlSafeStr, Statement, TypeInfo, ValueRef,
};

use futures::stream::StreamExt;

use tablepro_core::{
    CONTROL_SETUP_TIMEOUT, ColumnInfo, ConnectOptions, Connection, DatabaseDriver, DriverError, ExecResult,
    ForeignKeyInfo, IndexInfo, MAX_QUERY_ROWS, OperationControl, QualifiedTypeName, QueryResult, QueryResultBudget,
    TableInfo, Transport, Value, check_pre_dispatch, run_controlled_setup, run_server_cancellable,
};

mod array;
mod catalog;
mod decode;
mod numeric;
mod params;
mod session;
mod temporal;
mod transaction;

use params::{bind_pg_params, describe_query_parameters, needs_enum_type_inference, pg_parameter_type_infos};

pub struct PgDriver;

#[async_trait]
impl DatabaseDriver for PgDriver {
    fn id(&self) -> &'static str {
        "postgres"
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

    fn supports_index_metadata(&self) -> bool {
        true
    }

    fn supports_foreign_key_metadata(&self) -> bool {
        true
    }

    fn catalog_object_kinds(&self) -> &'static [tablepro_core::CatalogObjectKind] {
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

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        use tablepro_core::TlsMode;
        let mut pg_opts = match opts.transport() {
            Transport::Tcp { host, port } => PgConnectOptions::new().host(host).port(port),
            Transport::Socket {
                directory,
                identity_host,
                identity_port,
                ..
            } => PgConnectOptions::new()
                .host(identity_host)
                .port(identity_port)
                .socket(directory),
        };
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
        if let Some(name) = &opts.application_name {
            pg_opts = pg_opts.application_name(name);
        }
        let cancellation_options = pg_opts.clone();
        let session_options = pg_opts.clone();
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(pg_opts)
            .await
            .map_err(map_sqlx_connect_error)?;
        // Lazy: a saved connection with a failed-login lockout policy
        // must not see two authentication attempts for one Connect
        // click. The real dial happens only when a cancel is actually
        // requested.
        let cancellation_pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect_lazy_with(cancellation_options);
        Ok(Box::new(PgConnection {
            pool,
            cancellation_pool,
            session_options,
        }))
    }
}

struct PgConnection {
    pool: Pool<Postgres>,
    cancellation_pool: Pool<Postgres>,
    session_options: PgConnectOptions,
}

#[async_trait]
impl Connection for PgConnection {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        let rows = sqlx::query(
            "SELECT schemaname, tablename
             FROM pg_tables
             WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
             ORDER BY schemaname, tablename",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|r| TableInfo {
                schema: Some(r.get::<String, _>(0)),
                name: r.get::<String, _>(1),
            })
            .collect())
    }

    async fn list_views(&self) -> Result<Vec<TableInfo>, DriverError> {
        let rows = sqlx::query(
            "SELECT schemaname, viewname FROM (
                 SELECT schemaname, viewname FROM pg_views
                 UNION ALL
                 SELECT schemaname, matviewname FROM pg_matviews
             ) AS relations
             WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
             ORDER BY schemaname, viewname",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|r| TableInfo {
                schema: Some(r.get::<String, _>(0)),
                name: r.get::<String, _>(1),
            })
            .collect())
    }

    async fn list_objects(
        &self,
        kind: tablepro_core::CatalogObjectKind,
        schema: Option<&str>,
    ) -> Result<Vec<tablepro_core::CatalogObject>, DriverError> {
        catalog::list_objects(&self.pool, kind, schema).await
    }

    async fn fetch_columns(&self, schema: Option<&str>, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        // Source schema metadata from pg_catalog rather than
        // information_schema:
        //   - pg_attribute.attgenerated ('s' for STORED, '' otherwise)
        //     is the canonical generated-column flag. The
        //     information_schema.is_generated text column is brittle
        //     across PG versions.
        //   - pg_attribute.attidentity ('a' / 'd' for ALWAYS / BY
        //     DEFAULT identity, '' otherwise) authoritatively flags
        //     identity columns.
        //   - format_type() returns the user-facing type name including
        //     length / precision (e.g. "character varying(255)") which
        //     matches what the user wrote in CREATE TABLE.
        //   - pg_get_expr() returns the default expression text.
        //   - col_description() is the column-level comment accessor;
        //     obj_description() answers for the relation, so using it
        //     here would give every column the table's comment.
        let rows = sqlx::query(
            "SELECT
                a.attname,
                pg_catalog.format_type(a.atttypid, a.atttypmod) AS data_type,
                NOT a.attnotnull AS nullable,
                EXISTS (
                    SELECT 1 FROM pg_catalog.pg_constraint c
                    WHERE c.conrelid = a.attrelid
                      AND c.contype = 'p'
                      AND a.attnum = ANY(c.conkey)
                ) AS is_pk,
                pg_catalog.pg_get_expr(d.adbin, d.adrelid) AS default_value,
                a.attidentity <> '' AS is_identity,
                a.attgenerated <> '' AS is_generated,
                pg_catalog.col_description(a.attrelid, a.attnum) AS column_comment,
                CASE WHEN a.attcollation <> 0 AND a.attcollation <> ty.typcollation
                    THEN co.collname::text END AS collation,
                type_ns.nspname AS enum_schema,
                enum_ty.typname AS enum_name
             FROM pg_catalog.pg_attribute a
             JOIN pg_catalog.pg_class t ON a.attrelid = t.oid
             JOIN pg_catalog.pg_namespace n ON t.relnamespace = n.oid
             JOIN pg_catalog.pg_type ty ON ty.oid = a.atttypid
             LEFT JOIN LATERAL (
                 WITH RECURSIVE type_chain(oid, typtype, typbasetype, typnamespace, typname) AS (
                     SELECT candidate.oid, candidate.typtype, candidate.typbasetype,
                            candidate.typnamespace, candidate.typname
                     FROM pg_catalog.pg_type candidate
                     WHERE candidate.oid = CASE
                         WHEN ty.typcategory = 'A' AND EXISTS (
                             SELECT 1 FROM pg_catalog.pg_type element
                             WHERE element.oid = ty.typelem AND element.typtype IN ('e', 'd')
                         ) THEN ty.typelem
                         ELSE ty.oid
                     END
                     UNION ALL
                     SELECT base.oid, base.typtype, base.typbasetype,
                            base.typnamespace, base.typname
                     FROM pg_catalog.pg_type base
                     JOIN type_chain ON base.oid = type_chain.typbasetype
                     WHERE type_chain.typtype = 'd'
                 )
                 SELECT typnamespace, typname
                 FROM type_chain
                 WHERE typtype = 'e'
                    OR (ty.typcategory = 'A' AND oid = ty.typelem AND typtype = 'd')
                 ORDER BY CASE
                     WHEN ty.typcategory = 'A' AND oid = ty.typelem AND typtype = 'd' THEN 0
                     ELSE 1
                 END
                 LIMIT 1
             ) enum_ty ON TRUE
             LEFT JOIN pg_catalog.pg_namespace type_ns ON type_ns.oid = enum_ty.typnamespace
             LEFT JOIN pg_catalog.pg_collation co ON co.oid = a.attcollation
             LEFT JOIN pg_catalog.pg_attrdef d
                 ON d.adrelid = a.attrelid AND d.adnum = a.attnum
             WHERE n.nspname = COALESCE($2, current_schema())
               AND t.relname = $1
               AND a.attnum > 0
               AND NOT a.attisdropped
             ORDER BY a.attnum",
        )
        .bind(table)
        .bind(schema)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let raw_default: Option<String> = r.try_get::<Option<String>, _>(4).unwrap_or(None);
                let is_identity = r.try_get::<bool, _>(5).unwrap_or(false);
                let is_generated = r.try_get::<bool, _>(6).unwrap_or(false);
                // SERIAL / BIGSERIAL columns aren't IDENTITY in PG's
                // catalog terms but have a `nextval(...)` default; treat
                // them as auto-increment for the inline-insert UI.
                let is_serial = raw_default
                    .as_deref()
                    .map(|d| d.starts_with("nextval("))
                    .unwrap_or(false);
                // For identity / serial columns the default expression
                // is internal sequence machinery — suppress so the UI
                // doesn't leak implementation details. Otherwise
                // normalise the expression for display.
                let default_value = if is_identity || is_serial {
                    None
                } else {
                    raw_default.map(normalize_pg_default)
                };
                let enum_schema = r.try_get::<Option<String>, _>(9).map_err(map_sqlx_error)?;
                let enum_name = r.try_get::<Option<String>, _>(10).map_err(map_sqlx_error)?;
                Ok(ColumnInfo {
                    name: r.get::<String, _>(0),
                    data_type: r.get::<String, _>(1),
                    nullable: r.get::<bool, _>(2),
                    primary_key: r.get::<bool, _>(3),
                    is_auto_increment: is_identity || is_serial,
                    default_value,
                    is_generated,
                    comment: r
                        .try_get::<Option<String>, _>(7)
                        .unwrap_or(None)
                        .filter(|c| !c.is_empty()),
                    collation: r.try_get::<Option<String>, _>(8).unwrap_or(None),
                    enum_type: enum_schema
                        .zip(enum_name)
                        .map(|(schema, name)| QualifiedTypeName { schema, name }),
                })
            })
            .collect::<Result<Vec<_>, DriverError>>()?)
    }

    async fn fetch_rows(
        &self,
        schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<QueryResult, DriverError> {
        let sql = format!(
            "SELECT * FROM {} OFFSET {offset} LIMIT {limit}",
            qualified(schema, table)
        );
        stream_into_result(&self.pool, &sql, limit as usize).await
    }

    async fn query(&self, sql: &str) -> Result<QueryResult, DriverError> {
        stream_into_result(&self.pool, sql, MAX_QUERY_ROWS).await
    }

    async fn query_controlled(&self, sql: &str, control: &OperationControl) -> Result<QueryResult, DriverError> {
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let backend_pid = backend_pid_controlled(&mut connection, control).await?;
        let (connection, result) = controlled_query(
            connection,
            &self.cancellation_pool,
            backend_pid,
            sql,
            &[],
            true,
            control,
        )
        .await;
        drop(connection);
        result
    }

    async fn query_params(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        query_connection(&mut connection, sql, params).await
    }

    async fn query_params_controlled(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let backend_pid = backend_pid_controlled(&mut connection, control).await?;
        let (connection, result) = controlled_query(
            connection,
            &self.cancellation_pool,
            backend_pid,
            sql,
            params,
            true,
            control,
        )
        .await;
        drop(connection);
        result
    }

    async fn execute(&self, sql: &str) -> Result<ExecResult, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        execute_connection(&mut connection, sql, &[]).await
    }

    async fn execute_controlled(&self, sql: &str, control: &OperationControl) -> Result<ExecResult, DriverError> {
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let backend_pid = backend_pid_controlled(&mut connection, control).await?;
        let (connection, result) = controlled_execute(
            connection,
            &self.cancellation_pool,
            backend_pid,
            sql,
            &[],
            true,
            control,
        )
        .await;
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
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let backend_pid = backend_pid_controlled(&mut connection, control).await?;
        let (connection, result) = controlled_execute(
            connection,
            &self.cancellation_pool,
            backend_pid,
            sql,
            params,
            true,
            control,
        )
        .await;
        drop(connection);
        result
    }

    async fn execute_in_transaction(&self, statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        self.execute_in_transaction_checked(statements, &[]).await
    }

    async fn execute_in_transaction_checked(
        &self,
        statements: &[(String, Vec<Value>)],
        expect_one: &[usize],
    ) -> Result<Vec<u64>, DriverError> {
        transaction::execute_in_transaction_checked(&self.pool, statements, expect_one).await
    }

    async fn fetch_indexes(&self, schema: Option<&str>, table: &str) -> Result<Vec<IndexInfo>, DriverError> {
        // `indkey` holds 0 for an expression key and lists INCLUDE columns
        // after the first `indnkeyatts` key columns; the expression text
        // comes from `pg_get_indexdef` at that key position.
        let rows = sqlx::query(
            "SELECT
                i.relname AS index_name,
                ix.indisunique,
                ix.indisprimary,
                ARRAY(
                    SELECT CASE WHEN k.attnum = 0
                        THEN pg_catalog.pg_get_indexdef(ix.indexrelid, k.ordinality::int, true)
                        ELSE a.attname::text END
                    FROM unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ordinality)
                    LEFT JOIN pg_catalog.pg_attribute a ON a.attrelid = ix.indrelid AND a.attnum = k.attnum
                    WHERE k.ordinality <= ix.indnkeyatts
                    ORDER BY k.ordinality
                ) AS columns,
                pg_catalog.pg_get_expr(ix.indpred, ix.indrelid, true) AS predicate
            FROM pg_catalog.pg_class t
            JOIN pg_catalog.pg_namespace n ON t.relnamespace = n.oid
            JOIN pg_catalog.pg_index ix ON ix.indrelid = t.oid
            JOIN pg_catalog.pg_class i ON i.oid = ix.indexrelid
            WHERE n.nspname = COALESCE($2, current_schema())
              AND t.relname = $1
            ORDER BY i.relname",
        )
        .bind(table)
        .bind(schema)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter()
            .map(|r| {
                Ok(IndexInfo {
                    name: r.try_get(0).map_err(map_sqlx_error)?,
                    unique: r.try_get(1).map_err(map_sqlx_error)?,
                    primary: r.try_get(2).map_err(map_sqlx_error)?,
                    columns: r.try_get(3).map_err(map_sqlx_error)?,
                    predicate: r.try_get(4).map_err(map_sqlx_error)?,
                })
            })
            .collect()
    }

    async fn fetch_foreign_keys(&self, schema: Option<&str>, table: &str) -> Result<Vec<ForeignKeyInfo>, DriverError> {
        // pg_constraint with contype = 'f'. confkey arrays are parallel
        // to conkey via ordinality; the LATERAL join pairs them so the
        // FK column ↔ referenced column mapping survives composite
        // FKs. confdeltype / confupdtype are single chars normalised
        // to canonical SQL keyword strings.
        let rows = sqlx::query(
            "SELECT
                c.conname AS fk_name,
                array_agg(a.attname ORDER BY kf.ordinality) AS columns,
                fn_class.relname AS ref_table,
                fn_ns.nspname AS ref_schema,
                array_agg(fa.attname ORDER BY kf.ordinality) AS ref_columns,
                c.confdeltype::text AS on_delete_code,
                c.confupdtype::text AS on_update_code
            FROM pg_catalog.pg_constraint c
            JOIN pg_catalog.pg_class t ON t.oid = c.conrelid
            JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
            JOIN pg_catalog.pg_class fn_class ON fn_class.oid = c.confrelid
            JOIN pg_catalog.pg_namespace fn_ns ON fn_ns.oid = fn_class.relnamespace
            JOIN LATERAL unnest(c.conkey) WITH ORDINALITY AS kf(attnum, ordinality) ON true
            JOIN pg_catalog.pg_attribute a ON a.attrelid = t.oid AND a.attnum = kf.attnum
            JOIN LATERAL unnest(c.confkey) WITH ORDINALITY AS kfr(attnum, ordinality)
                ON kfr.ordinality = kf.ordinality
            JOIN pg_catalog.pg_attribute fa ON fa.attrelid = c.confrelid AND fa.attnum = kfr.attnum
            WHERE c.contype = 'f'
              AND n.nspname = COALESCE($2, current_schema())
              AND t.relname = $1
            GROUP BY c.conname, fn_class.relname, fn_ns.nspname, c.confdeltype, c.confupdtype
            ORDER BY c.conname",
        )
        .bind(table)
        .bind(schema)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|r| ForeignKeyInfo {
                name: r.get::<String, _>(0),
                columns: r.get::<Vec<String>, _>(1),
                ref_table: r.get::<String, _>(2),
                ref_schema: r.try_get::<Option<String>, _>(3).unwrap_or(None),
                ref_columns: r.get::<Vec<String>, _>(4),
                on_delete: pg_action_char_to_keyword(r.try_get::<String, _>(5).ok().as_deref().unwrap_or("a")),
                on_update: pg_action_char_to_keyword(r.try_get::<String, _>(6).ok().as_deref().unwrap_or("a")),
            })
            .collect())
    }

    fn supports_server_cancellation(&self) -> bool {
        true
    }

    async fn ping(&self) -> Result<(), DriverError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn open_session(&self) -> Result<Box<dyn tablepro_core::Session>, DriverError> {
        session::open(&self.session_options, self.cancellation_pool.clone()).await
    }

    async fn begin(&self) -> Result<Box<dyn tablepro_core::Transaction>, DriverError> {
        let mut connection = tokio::time::timeout(CONTROL_SETUP_TIMEOUT, self.pool.acquire())
            .await
            .map_err(|_| DriverError::TimedOut)?
            .map_err(map_sqlx_error)?;
        let backend_pid = tokio::time::timeout(CONTROL_SETUP_TIMEOUT, backend_pid(&mut connection))
            .await
            .map_err(|_| DriverError::TimedOut)??;
        sqlx::query("BEGIN")
            .execute(&mut *connection)
            .await
            .map_err(map_sqlx_error)?;
        Ok(Box::new(PgTransaction {
            connection: Some(connection),
            cancellation_pool: self.cancellation_pool.clone(),
            backend_pid,
        }))
    }

    async fn server_version(&self) -> Result<Option<String>, DriverError> {
        let row = sqlx::query("SHOW server_version")
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        let v: String = row.try_get(0).unwrap_or_default();
        Ok(Some(format!("PostgreSQL {v}")))
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        self.pool.close().await;
        self.cancellation_pool.close().await;
        Ok(())
    }
}

async fn transaction_failure(
    tx: sqlx::Transaction<'static, Postgres>,
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
    cancellation_pool: Pool<Postgres>,
    backend_pid: i32,
}

impl Drop for PgTransaction {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.as_mut() {
            connection.close_on_drop();
        }
    }
}

#[async_trait]
impl tablepro_core::Transaction for PgTransaction {
    async fn query(&mut self, sql: &str) -> Result<QueryResult, DriverError> {
        let connection = self.connection_mut()?;
        query_connection_once(connection, sql, &[]).await
    }

    async fn query_controlled(&mut self, sql: &str, control: &OperationControl) -> Result<QueryResult, DriverError> {
        self.query_params_controlled(sql, &[], control).await
    }

    async fn query_params(&mut self, sql: &str, params: &[Value]) -> Result<QueryResult, DriverError> {
        let connection = self.connection_mut()?;
        query_connection_once(connection, sql, params).await
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
            connection,
            &self.cancellation_pool,
            self.backend_pid,
            sql,
            params,
            // An error aborts an explicit transaction; the caller must roll it back.
            false,
            control,
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
            connection,
            &self.cancellation_pool,
            self.backend_pid,
            sql,
            params,
            // An error aborts an explicit transaction; the caller must roll it back.
            false,
            control,
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

async fn backend_pid_controlled(
    connection: &mut PoolConnection<Postgres>,
    control: &OperationControl,
) -> Result<i32, DriverError> {
    run_controlled_setup(backend_pid(connection), control).await?
}

async fn backend_pid(connection: &mut PoolConnection<Postgres>) -> Result<i32, DriverError> {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **connection)
        .await
        .map_err(map_sqlx_error)
}

async fn controlled_query(
    mut connection: PoolConnection<Postgres>,
    cancellation_pool: &Pool<Postgres>,
    backend_pid: i32,
    sql: &str,
    params: &[Value],
    retry_stale_type_cache: bool,
    control: &OperationControl,
) -> (Option<PoolConnection<Postgres>>, Result<QueryResult, DriverError>) {
    let result = run_server_cancellable(
        async {
            if retry_stale_type_cache {
                query_connection(&mut connection, sql, params).await
            } else {
                query_connection_once(&mut connection, sql, params).await
            }
        },
        request_cancellation(cancellation_pool, backend_pid),
        confirms_cancellation,
        control,
    )
    .await;
    finish_connection(connection, result).await
}

async fn controlled_execute(
    mut connection: PoolConnection<Postgres>,
    cancellation_pool: &Pool<Postgres>,
    backend_pid: i32,
    sql: &str,
    params: &[Value],
    retry_stale_type_cache: bool,
    control: &OperationControl,
) -> (Option<PoolConnection<Postgres>>, Result<ExecResult, DriverError>) {
    let result = run_server_cancellable(
        async {
            if retry_stale_type_cache {
                execute_connection(&mut connection, sql, params).await
            } else {
                execute_connection_once(&mut connection, sql, params).await
            }
        },
        request_cancellation(cancellation_pool, backend_pid),
        confirms_cancellation,
        control,
    )
    .await;
    finish_connection(connection, result).await
}

fn confirms_cancellation(error: &DriverError) -> bool {
    matches!(
        error,
        DriverError::Query {
            sqlstate: Some(sqlstate),
            ..
        } if sqlstate == "57014"
    )
}

async fn finish_connection<T>(
    connection: PoolConnection<Postgres>,
    result: Result<T, DriverError>,
) -> (Option<PoolConnection<Postgres>>, Result<T, DriverError>) {
    if matches!(result, Err(DriverError::OperationOutcomeUnknown { .. })) {
        let raw_connection = connection.detach();
        let _ = raw_connection.close_hard().await;
        return (None, result);
    }
    (Some(connection), result)
}

/// `run_server_cancellable` wraps this future in its own dispatch
/// timeout and moves on without it if that elapses. Run the actual
/// round trip in a spawned task instead of inline, so giving up on it
/// here can never drop it mid-response and return a protocol-desynced
/// connection to the single-slot cancellation pool -- the task always
/// finishes and explicitly closes the connection on failure rather than
/// trusting an implicit pool return.
async fn request_cancellation(pool: &Pool<Postgres>, backend_pid: i32) -> Result<(), DriverError> {
    let pool = pool.clone();
    let task = tokio::spawn(async move {
        let mut conn = pool.acquire().await.map_err(map_sqlx_error)?;
        let outcome: Result<bool, DriverError> = sqlx::query_scalar("SELECT pg_cancel_backend($1)")
            .bind(backend_pid)
            .fetch_one(&mut *conn)
            .await
            .map_err(map_sqlx_error);
        if outcome.is_err() {
            let _ = conn.detach().close_hard().await;
        }
        outcome
    });
    let cancelled = match task.await {
        Ok(result) => result?,
        Err(_) => return Err(DriverError::Cancelled),
    };
    if cancelled {
        Ok(())
    } else {
        Err(DriverError::Internal(
            "PostgreSQL rejected the cancellation request".into(),
        ))
    }
}

async fn query_connection(
    connection: &mut PoolConnection<Postgres>,
    sql: &str,
    params: &[Value],
) -> Result<QueryResult, DriverError> {
    let result = query_connection_once(connection, sql, params).await;
    // A concurrent DROP/CREATE can leave a cached plan pointing at the old
    // OID. PostgreSQL raises this before executing the statement, so clear
    // SQLx's per-connection caches and retry the same operation once.
    if result.as_ref().is_err_and(is_stale_type_cache_error) {
        connection.clear_cached_statements().await.map_err(map_sqlx_error)?;
        return query_connection_once(connection, sql, params).await;
    }
    result
}

async fn query_connection_once(
    connection: &mut PoolConnection<Postgres>,
    sql: &str,
    params: &[Value],
) -> Result<QueryResult, DriverError> {
    let description = if needs_enum_type_inference(params) {
        Some(describe_query_parameters(connection, sql, params).await?)
    } else {
        None
    };
    let inferred_text_types = description
        .as_ref()
        .map_or(&[][..], |description| description.inferred_text_types.as_slice());
    let query = bind_pg_params(sqlx::query(sqlx::AssertSqlSafe(sql)), params, inferred_text_types)?;
    let mut stream = query.fetch(&mut **connection);
    let (mut result, mut type_infos) = collect_query_rows(&mut stream, MAX_QUERY_ROWS).await?;
    drop(stream);
    if result.columns.is_empty() && result.rows.is_empty() {
        if let Some(description) = description {
            result.columns = description.columns;
            type_infos = description.column_type_infos;
        } else {
            let parameter_types = pg_parameter_type_infos(params);
            let statement = connection
                .prepare_with(sqlx::AssertSqlSafe(sql).into_sql_str(), &parameter_types)
                .await
                .map_err(map_sqlx_error)?;
            result.columns = statement_columns(statement.columns());
            type_infos = statement_type_infos(statement.columns());
        }
    }
    refresh_enum_type_names(connection, &mut result.columns, &type_infos).await?;
    Ok(result)
}

async fn execute_connection(
    connection: &mut PoolConnection<Postgres>,
    sql: &str,
    params: &[Value],
) -> Result<ExecResult, DriverError> {
    let result = execute_connection_once(connection, sql, params).await;
    if result.as_ref().is_err_and(is_stale_type_cache_error) {
        connection.clear_cached_statements().await.map_err(map_sqlx_error)?;
        return execute_connection_once(connection, sql, params).await;
    }
    result
}

async fn execute_connection_once(
    connection: &mut PoolConnection<Postgres>,
    sql: &str,
    params: &[Value],
) -> Result<ExecResult, DriverError> {
    let inferred_text_types = if needs_enum_type_inference(params) {
        describe_query_parameters(connection, sql, params)
            .await?
            .inferred_text_types
    } else {
        Vec::new()
    };
    let query = bind_pg_params(sqlx::query(sqlx::AssertSqlSafe(sql)), params, &inferred_text_types)?;
    let result = query.execute(&mut **connection).await.map_err(map_sqlx_error)?;
    Ok(ExecResult {
        rows_affected: result.rows_affected(),
    })
}

fn is_stale_type_cache_error(error: &DriverError) -> bool {
    matches!(
        error,
        DriverError::Query {
            message,
            sqlstate: Some(sqlstate),
        } if sqlstate == "XX000" && message.starts_with("cache lookup failed for type ")
    )
}

async fn stream_into_result(pool: &Pool<Postgres>, sql: &str, limit: usize) -> Result<QueryResult, DriverError> {
    let mut connection = pool.acquire().await.map_err(map_sqlx_error)?;
    let result = stream_into_result_once(&mut connection, sql, limit).await;
    if result.as_ref().is_err_and(is_stale_type_cache_error) {
        connection.clear_cached_statements().await.map_err(map_sqlx_error)?;
        return stream_into_result_once(&mut connection, sql, limit).await;
    }
    result
}

async fn stream_into_result_once(
    connection: &mut PoolConnection<Postgres>,
    sql: &str,
    limit: usize,
) -> Result<QueryResult, DriverError> {
    let mut stream = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch(&mut **connection);
    let (mut result, mut type_infos) = collect_query_rows(&mut stream, limit).await?;
    drop(stream);
    if result.columns.is_empty() && result.rows.is_empty() {
        let statement = connection
            .prepare(sqlx::AssertSqlSafe(sql).into_sql_str())
            .await
            .map_err(map_sqlx_error)?;
        result.columns = statement_columns(statement.columns());
        type_infos = statement_type_infos(statement.columns());
    }
    refresh_enum_type_names(connection, &mut result.columns, &type_infos).await?;
    Ok(result)
}

fn statement_columns(columns: &[sqlx::postgres::PgColumn]) -> Vec<ColumnInfo> {
    columns
        .iter()
        .map(|column| ColumnInfo {
            name: column.name().to_string(),
            data_type: column.type_info().name().to_ascii_lowercase(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
        })
        .collect()
}

fn statement_type_infos(columns: &[sqlx::postgres::PgColumn]) -> Vec<PgTypeInfo> {
    columns.iter().map(|column| column.type_info().clone()).collect()
}

fn has_enum_leaf(type_info: &PgTypeInfo) -> bool {
    match type_info.kind() {
        PgTypeKind::Enum(_) => true,
        PgTypeKind::Array(element) | PgTypeKind::Domain(element) => has_enum_leaf(element),
        _ => false,
    }
}

async fn refresh_enum_type_names(
    connection: &mut PoolConnection<Postgres>,
    columns: &mut [ColumnInfo],
    type_infos: &[PgTypeInfo],
) -> Result<(), DriverError> {
    let type_oids: Vec<i64> = type_infos
        .iter()
        .filter(|type_info| has_enum_leaf(type_info))
        .filter_map(|type_info| type_info.oid().map(|oid| i64::from(oid.0)))
        .collect();
    if type_oids.is_empty() {
        return Ok(());
    }

    let rows = sqlx::query(
        "SELECT type.oid::bigint, pg_catalog.format_type(type.oid, NULL) \
         FROM pg_catalog.pg_type AS type \
         WHERE type.oid IN (SELECT unnest($1::bigint[])::oid)",
    )
    .bind(&type_oids)
    .fetch_all(&mut **connection)
    .await
    .map_err(map_sqlx_error)?;
    let mut names = HashMap::with_capacity(rows.len());
    for row in rows {
        let oid: i64 = row.try_get(0).map_err(map_sqlx_error)?;
        let name: String = row.try_get(1).map_err(map_sqlx_error)?;
        names.insert(oid, name);
    }

    for (column, type_info) in iter::zip(columns, type_infos) {
        if has_enum_leaf(type_info)
            && let Some(oid) = type_info.oid().map(|oid| i64::from(oid.0))
            && let Some(name) = names.get(&oid)
        {
            column.data_type.clone_from(name);
        }
    }
    Ok(())
}

async fn collect_query_rows<'a, S>(stream: &mut S, limit: usize) -> Result<(QueryResult, Vec<PgTypeInfo>), DriverError>
where
    S: futures::Stream<Item = Result<PgRow, sqlx::Error>> + Unpin + 'a,
{
    let mut columns = Vec::new();
    let mut type_infos = Vec::new();
    let mut type_names = Vec::new();
    let mut rows = Vec::new();
    let mut budget = QueryResultBudget::default();
    let mut truncated = false;
    while let Some(row_result) = stream.next().await {
        let row = row_result.map_err(map_sqlx_error)?;
        if rows.len() >= limit {
            truncated = true;
            break;
        }
        if rows.is_empty() {
            type_infos = statement_type_infos(row.columns());
            type_names = row
                .columns()
                .iter()
                .map(|column| column.type_info().name().to_ascii_uppercase())
                .collect::<Vec<_>>();
            columns = row
                .columns()
                .iter()
                .map(|column| ColumnInfo {
                    name: column.name().to_string(),
                    data_type: column.type_info().name().to_string(),
                    nullable: true,
                    primary_key: false,
                    is_auto_increment: false,
                    default_value: None,
                    is_generated: false,
                    comment: None,
                    collation: None,
                    enum_type: None,
                })
                .collect();
        }
        // Decode while streaming so the wire rows and the final Value matrix
        // do not both occupy memory for the entire result.
        let values = type_names
            .iter()
            .enumerate()
            .map(|(index, type_name)| extract_value(&row, index, type_name))
            .collect::<Result<Vec<_>, _>>()?;
        if !budget.admit(&values) {
            truncated = true;
            break;
        }
        rows.push(values);
    }
    Ok((
        QueryResult {
            columns,
            rows,
            truncated,
        },
        type_infos,
    ))
}

fn extract_value(row: &PgRow, idx: usize, type_name: &str) -> Result<Value, DriverError> {
    let raw = row.try_get_raw(idx).map_err(map_sqlx_error)?;
    if raw.is_null() {
        return Ok(Value::Null);
    }
    if matches!(raw.type_info().kind(), sqlx::postgres::PgTypeKind::Enum(_)) {
        return Ok(extract_enum_value(&raw, idx, type_name));
    }
    if matches!(raw.type_info().kind(), sqlx::postgres::PgTypeKind::Array(_)) {
        return Ok(array::decode(&raw).unwrap_or_else(|| undecodable(idx, type_name)));
    }
    if type_name == "NUMERIC" {
        let value = match raw.format() {
            sqlx::postgres::PgValueFormat::Binary => raw.as_bytes().ok().and_then(numeric::decode_binary),
            sqlx::postgres::PgValueFormat::Text => raw.as_str().ok().and_then(numeric::decode_text),
        };
        return Ok(value.unwrap_or_else(|| undecodable(idx, type_name)));
    }
    if raw.format() == sqlx::postgres::PgValueFormat::Binary {
        if matches!(type_name, "TIME" | "TIMETZ") {
            return Ok(raw
                .as_bytes()
                .ok()
                .and_then(|bytes| temporal::decode_time(bytes, type_name == "TIMETZ"))
                .unwrap_or_else(|| undecodable(idx, type_name)));
        }
        if matches!(type_name, "DATE" | "TIMESTAMP" | "TIMESTAMPTZ") {
            return Ok(raw
                .as_bytes()
                .ok()
                .and_then(|bytes| temporal::decode_temporal(bytes, type_name))
                .unwrap_or_else(|| undecodable(idx, type_name)));
        }
        if let Some(text) = raw
            .as_bytes()
            .ok()
            .and_then(|bytes| decode::decode_pg_binary_text(type_name, bytes))
        {
            return Ok(Value::Text(text));
        }
    }
    let decoded = match type_name {
        "BOOL" => row.try_get::<bool, _>(idx).map(Value::Bool),
        "INT2" => row.try_get::<i16, _>(idx).map(|v| Value::Int(v as i64)),
        "INT4" => row.try_get::<i32, _>(idx).map(|v| Value::Int(v as i64)),
        "INT8" => row.try_get::<i64, _>(idx).map(Value::Int),
        "FLOAT4" => row.try_get::<f32, _>(idx).map(|v| Value::Float(v as f64)),
        "FLOAT8" => row.try_get::<f64, _>(idx).map(Value::Float),
        "DATE" => row.try_get::<chrono::NaiveDate, _>(idx).map(Value::Date),
        "TIME" => row.try_get::<chrono::NaiveTime, _>(idx).map(Value::Time),
        "TIMESTAMP" => row.try_get::<chrono::NaiveDateTime, _>(idx).map(Value::DateTime),
        "TIMESTAMPTZ" => row
            .try_get::<chrono::DateTime<chrono::Utc>, _>(idx)
            .map(Value::TimestampTz),
        "UUID" => row.try_get::<uuid::Uuid, _>(idx).map(Value::Uuid),
        "JSON" | "JSONB" => row.try_get::<serde_json::Value, _>(idx).map(Value::Json),
        "BYTEA" => row.try_get::<Vec<u8>, _>(idx).map(Value::Bytes),
        _ => row.try_get::<String, _>(idx).map(Value::Text),
    };
    Ok(decoded.unwrap_or_else(|_| undecodable(idx, type_name)))
}

fn extract_enum_value(raw: &sqlx::postgres::PgValueRef<'_>, idx: usize, type_name: &str) -> Value {
    let label = match raw.format() {
        sqlx::postgres::PgValueFormat::Binary => raw.as_bytes().ok().and_then(|bytes| std::str::from_utf8(bytes).ok()),
        sqlx::postgres::PgValueFormat::Text => raw.as_str().ok(),
    };
    label
        .map(|value| Value::Text(value.to_owned()))
        .unwrap_or_else(|| undecodable(idx, type_name))
}

fn undecodable(idx: usize, type_name: &str) -> Value {
    tracing::warn!(
        column_index = idx + 1,
        type_name,
        "postgres column value could not be decoded; showing it as undecodable"
    );
    Value::Undecodable(type_name.to_string())
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
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
    match schema {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(table)),
        None => quote_ident(table),
    }
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

fn is_certificate_failure(err: &std::io::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(current) = source {
        let text = current.to_string().to_ascii_lowercase();
        if text.contains("certificate") || text.contains("tls handshake") {
            return true;
        }
        source = current.source();
    }
    false
}

fn map_sqlx_error(err: sqlx::Error) -> DriverError {
    if is_enum_label_metadata_null(&err) {
        return DriverError::Unsupported(
            "PostgreSQL enum result metadata could not be resolved; the result was not read".into(),
        );
    }
    if matches!(&err, sqlx::Error::Protocol(message) if message.starts_with("unable to resolve type OIDs:")) {
        return DriverError::Unsupported("PostgreSQL type hierarchy exceeds the driver's resolvable depth".into());
    }
    use sqlx::Error::*;
    match err {
        Database(e) => {
            let sqlstate = e.code().map(|c| c.to_string());
            if is_server_disconnect_sqlstate(sqlstate.as_deref()) {
                DriverError::Disconnected
            } else {
                DriverError::Query {
                    message: e.message().to_string(),
                    sqlstate,
                }
            }
        }
        Io(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => DriverError::ConnectionRefused,
        Io(e) if is_certificate_failure(&e) => DriverError::Tls(e.to_string()),
        Io(_) => DriverError::Disconnected,
        Tls(e) => DriverError::Tls(e.to_string()),
        PoolClosed | PoolTimedOut => DriverError::Disconnected,
        other => DriverError::Internal(format!("{other}")),
    }
}

fn is_enum_label_metadata_null(err: &sqlx::Error) -> bool {
    let sqlx::Error::ColumnDecode { index, source } = err else {
        return false;
    };
    index == "\"enum_labels\"" && source.downcast_ref::<sqlx::error::UnexpectedNullError>().is_some()
}

fn map_sqlx_connect_error(err: sqlx::Error) -> DriverError {
    if is_pool_startup_timeout(&err) {
        // SQLx retries refused pool connections until the acquire deadline,
        // then erases the underlying I/O error into PoolTimedOut. During
        // initial setup there is no established connection to disconnect.
        DriverError::ConnectionRefused
    } else {
        map_sqlx_error(err)
    }
}

fn is_pool_startup_timeout(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::PoolTimedOut)
}

fn is_server_disconnect_sqlstate(sqlstate: Option<&str>) -> bool {
    matches!(sqlstate, Some("57P01" | "57P02" | "57P03" | "57P04"))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
