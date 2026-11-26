use async_trait::async_trait;
use sqlx::Connection as SqlxConnection;

use super::*;

pub(super) async fn begin_and_run_checked(
    connection: &mut sqlx::PgConnection,
    statements: &[(String, Vec<Value>)],
    expect_one: &[usize],
) -> Result<Vec<u64>, DriverError> {
    let transaction = connection.begin().await.map_err(map_sqlx_error)?;
    run_checked(transaction, statements, expect_one).await
}

pub(super) fn confirms_stopped_statement(error: &DriverError) -> bool {
    match error {
        DriverError::Transaction { source, .. } => confirms_cancellation(source),
        other => confirms_cancellation(other),
    }
}

pub(super) async fn run_checked(
    mut tx: sqlx::Transaction<'_, Postgres>,
    statements: &[(String, Vec<Value>)],
    expect_one: &[usize],
) -> Result<Vec<u64>, DriverError> {
    let xid: i64 = sqlx::query_scalar("SELECT pg_catalog.txid_current()")
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;
    let mut affected = Vec::with_capacity(statements.len());
    for (index, (sql, params)) in statements.iter().enumerate() {
        let inferred_text_types = if needs_text_type_inference(params) {
            match describe_query_parameters(&mut tx, sql, params).await {
                Ok(description) => description.inferred_text_types,
                Err(error) => return Err(transaction_failure(tx, index, error.into_driver()).await),
            }
        } else {
            Vec::new()
        };
        let query = match bind_pg_params(
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str())),
            params,
            &inferred_text_types,
        ) {
            Ok(query) => query,
            Err(error) => return Err(transaction_failure(tx, index, error).await),
        };
        match query.execute(&mut *tx).await {
            Ok(result) => {
                let total = result.rows_affected();
                if expect_one.contains(&index) && total != 1 {
                    return Err(transaction_failure(tx, index, DriverError::ConcurrentModification).await);
                }
                affected.push(total);
            }
            Err(error) => return Err(transaction_failure(tx, index, map_sqlx_error(error)).await),
        }
    }
    match tx.commit().await {
        Ok(()) => Ok(affected),
        Err(sqlx::Error::Io(_)) => Err(DriverError::OperationOutcomeUnknown {
            source: Box::new(DriverError::CommitUnconfirmed(CommitToken::PostgresXid {
                xid: u64::try_from(xid).unwrap_or_default(),
                legacy_txid: true,
            })),
        }),
        Err(error) => Err(map_sqlx_error(error)),
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
