use async_trait::async_trait;
use sqlx::mysql::{MySql, MySqlConnectOptions, MySqlPoolOptions};
use sqlx::pool::PoolConnection;
use sqlx::{Connection as SqlxConnection, Pool};
use tablepro_core::{
    DriverError, MAX_QUERY_ROWS, OperationControl, QueryResult, Value, check_pre_dispatch, run_server_cancellable,
};

use crate::{confirms_cancellation, connection_id, map_sqlx_error, params_into_result, request_cancellation};

const SESSION_SETUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub(crate) struct MysqlSession {
    connection: Option<PoolConnection<MySql>>,
    pool: Pool<MySql>,
    cancellation_pool: Pool<MySql>,
    connection_id: u64,
}

pub(crate) async fn open(
    options: &MySqlConnectOptions,
    cancellation_pool: Pool<MySql>,
) -> Result<Box<dyn tablepro_core::Session>, DriverError> {
    let pool = MySqlPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(SESSION_SETUP_TIMEOUT)
        .connect_lazy_with(options.clone());
    let mut connection = tokio::time::timeout(SESSION_SETUP_TIMEOUT, pool.acquire())
        .await
        .map_err(|_| DriverError::TimedOut)?
        .map_err(map_sqlx_error)?;
    connection.close_on_drop();
    let connection_id = tokio::time::timeout(SESSION_SETUP_TIMEOUT, connection_id(&mut connection))
        .await
        .map_err(|_| DriverError::TimedOut)??;
    Ok(Box::new(MysqlSession {
        connection: Some(connection),
        pool,
        cancellation_pool,
        connection_id,
    }))
}

#[async_trait]
impl tablepro_core::Session for MysqlSession {
    async fn query_params_controlled(
        &mut self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        check_pre_dispatch(control)?;
        let mut connection = self
            .connection
            .take()
            .ok_or_else(|| DriverError::Internal("the session's connection was lost".into()))?;
        let mut result = run_server_cancellable(
            params_into_result(&mut *connection, sql, params, MAX_QUERY_ROWS),
            request_cancellation(&self.cancellation_pool, self.connection_id),
            confirms_cancellation,
            control,
        )
        .await;
        if params.is_empty() && result.as_ref().is_err_and(needs_text_protocol) {
            result = run_server_cancellable(
                text_protocol(&mut connection, sql),
                request_cancellation(&self.cancellation_pool, self.connection_id),
                confirms_cancellation,
                control,
            )
            .await;
        }
        if matches!(result, Err(DriverError::OperationOutcomeUnknown { .. })) {
            let _ = connection.detach().close_hard().await;
        } else {
            self.connection = Some(connection);
        }
        result
    }

    fn is_usable(&self) -> bool {
        self.connection.is_some()
    }

    async fn close(mut self: Box<Self>) -> Result<(), DriverError> {
        if let Some(connection) = self.connection.take() {
            let _ = connection.close().await;
        }
        self.pool.close().await;
        Ok(())
    }
}

// MySQL refuses some statements, START TRANSACTION among them, over the
// prepared-statement protocol (ER_UNSUPPORTED_PS, 1295). Only those are
// re-sent as plain text; everything else keeps typed result decoding.
fn needs_text_protocol(error: &DriverError) -> bool {
    matches!(error, DriverError::Query { message, .. } if message.contains("prepared statement protocol"))
}

async fn text_protocol(connection: &mut PoolConnection<MySql>, sql: &str) -> Result<QueryResult, DriverError> {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(&mut **connection)
        .await
        .map_err(map_sqlx_error)?;
    Ok(QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        truncated: false,
    })
}
