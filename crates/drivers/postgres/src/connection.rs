use super::*;
use async_trait::async_trait;
use bookie_core::Connection;

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

    async fn table_sizes(&self, schema: Option<&str>) -> Result<Vec<bookie_core::TableSize>, DriverError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        introspection::read_table_sizes(&mut connection, schema).await
    }

    async fn table_sizes_controlled(
        &self,
        schema: Option<&str>,
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::TableSize>, DriverError> {
        stoppable!(self, control, |connection| introspection::read_table_sizes(
            &mut connection,
            schema
        ))
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

    async fn fetch_schema_table_metadata_controlled(
        &self,
        tables: &[TableInfo],
        include_foreign_keys: bool,
        control: &OperationControl,
        visit: &mut SchemaTableMetadataVisitor<'_>,
    ) -> Result<(), DriverError> {
        stoppable!(self, control, |connection| read_fetch_schema_table_metadata(
            &mut connection,
            tables,
            include_foreign_keys,
            visit
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
        if let Some(parts) = copy_in::parse(sql)? {
            stoppable!(self, control, |connection| copy_in::run(&mut connection, &parts))?;
            return Ok(copy_in::empty_result());
        }
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
        if let Some(parts) = copy_in::parse(sql)? {
            let copied = stoppable!(self, control, |connection| copy_in::run(&mut connection, &parts))?;
            return Ok(QueryResultBatch::single(copy_in::empty_result()).with_rows_affected(Some(copied)));
        }
        let connection = acquire_controlled(&self.pool, control).await?;
        let (connection, result) = controlled_query_with_messages(connection, sql, params, true, true, control).await;
        drop(connection);
        let (rows, messages, rows_affected) = result?;
        Ok(QueryResultBatch::single(rows)
            .with_messages(messages)
            .with_rows_affected(rows_affected))
    }

    async fn query_result_sets_bounded_controlled(
        &self,
        sql: &str,
        params: &[Value],
        max_rows: usize,
        max_bytes: usize,
        control: &OperationControl,
    ) -> Result<QueryResultBatch, DriverError> {
        if copy_in::parse(sql)?.is_some() {
            return Err(DriverError::Unsupported("bounded COPY".into()));
        }
        let mut connection = acquire_controlled(&self.pool, control).await?;
        let cancel_token = connection.cancel_token();
        let stop = Some(QueryStop {
            cancel_token: cancel_token.clone(),
        });
        notices::discard(&mut connection);
        let result = run_server_cancellable(
            async {
                let (rows, affected) =
                    query::query_connection_bounded(&mut connection, sql, params, stop, max_rows, max_bytes).await?;
                Ok((rows, notices::drain(&mut connection), affected))
            },
            request_cancellation(cancel_token),
            confirms_cancellation,
            control,
        )
        .await;
        let (mut connection, result) = finish_connection(connection, result).await;
        if (result.is_err() || result.as_ref().is_ok_and(|(rows, _, _)| rows.truncated))
            && let Some(connection) = &mut connection
        {
            connection.close_on_drop();
        }
        drop(connection);
        let (rows, messages, affected) = result?;
        Ok(QueryResultBatch::single(rows)
            .with_messages(messages)
            .with_rows_affected(affected))
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
        if let Some(parts) = copy_in::parse(sql)? {
            stoppable!(self, control, |connection| copy_in::run(&mut connection, &parts))?;
            return Ok(copy_in::empty_result());
        }
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
        if let Some(parts) = copy_in::parse(sql)? {
            let rows_affected = stoppable!(self, control, |connection| copy_in::run(&mut connection, &parts))?;
            return Ok(ExecResult { rows_affected });
        }
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
