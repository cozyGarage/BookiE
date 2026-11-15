use bookie_core::{DriverError, Value};
use sqlx::{Connection as SqlxConnection, MySql};

use crate::{bind_mysql_params, confirms_cancellation, map_sqlx_error};

pub(super) async fn statement(connection: &mut sqlx::MySqlConnection, sql: &'static str) -> Result<(), DriverError> {
    sqlx::raw_sql(sql)
        .execute(connection)
        .await
        .map(|_| ())
        .map_err(map_sqlx_error)
}

pub(super) async fn begin_and_run_checked(
    connection: &mut sqlx::MySqlConnection,
    statements: &[(String, Vec<Value>)],
    expect_one: &[usize],
) -> Result<Vec<u64>, DriverError> {
    let transaction = connection.begin().await.map_err(map_sqlx_error)?;
    run_checked(transaction, statements, expect_one).await
}

pub(super) fn confirms_stopped_statement(error: &DriverError) -> bool {
    matches!(error, DriverError::Transaction { source, .. } if confirms_cancellation(source))
}

pub(super) async fn run_checked(
    mut tx: sqlx::Transaction<'_, MySql>,
    statements: &[(String, Vec<Value>)],
    expect_one: &[usize],
) -> Result<Vec<u64>, DriverError> {
    let mut affected = Vec::with_capacity(statements.len());
    for (index, (sql, params)) in statements.iter().enumerate() {
        let query = match bind_mysql_params(sqlx::query(sqlx::AssertSqlSafe(sql.as_str())), params) {
            Ok(query) => query,
            Err(error) => return Err(failure(tx, index, error).await),
        };
        match query.execute(&mut *tx).await {
            Ok(result) if expect_one.contains(&index) && result.rows_affected() != 1 => {
                return Err(failure(tx, index, DriverError::ConcurrentModification).await);
            }
            Ok(result) => affected.push(result.rows_affected()),
            Err(error) => return Err(failure(tx, index, map_sqlx_error(error)).await),
        }
    }
    tx.commit().await.map_err(map_sqlx_error)?;
    Ok(affected)
}

async fn failure(tx: sqlx::Transaction<'_, MySql>, statement_index: usize, source: DriverError) -> DriverError {
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
