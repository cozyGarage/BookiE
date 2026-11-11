use super::*;
use sqlx::Connection as _;

pub(super) async fn execute_in_transaction_checked(
    pool: &Pool<Sqlite>,
    statements: &[(String, Vec<Value>)],
    expect_one: &[usize],
) -> Result<Vec<u64>, DriverError> {
    let tx = pool.begin().await.map_err(map_sqlx_error)?;
    run_statements(tx, statements, expect_one, &|| false).await
}

pub(super) async fn execute_in_transaction_on(
    connection: &mut PoolConnection<Sqlite>,
    statements: &[(String, Vec<Value>)],
    stop_requested: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<u64>, DriverError> {
    let tx = connection.begin().await.map_err(map_sqlx_error)?;
    run_statements(tx, statements, &[], stop_requested).await
}

async fn run_statements(
    mut tx: sqlx::Transaction<'_, Sqlite>,
    statements: &[(String, Vec<Value>)],
    expect_one: &[usize],
    stop_requested: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<u64>, DriverError> {
    let mut affected = Vec::with_capacity(statements.len());
    for (idx, (sql, params)) in statements.iter().enumerate() {
        if stop_requested() {
            return Err(stopped(tx, idx).await);
        }
        let query = match drop_guard::refuse_referenced_drop(&mut tx, sql)
            .await
            .and_then(|()| bind_sqlite_params(sqlx::query(sqlx::AssertSqlSafe(sql.as_str())), params))
        {
            Ok(query) => query,
            Err(error) => {
                let _ = tx.rollback().await;
                return Err(DriverError::Transaction {
                    statement_index: idx,
                    source: Box::new(error),
                });
            }
        };
        match query.execute(&mut *tx).await {
            Ok(result) => {
                let total = result.rows_affected();
                if expect_one.contains(&idx) && total != 1 {
                    let source = DriverError::ConcurrentModification;
                    return Err(match tx.rollback().await {
                        Ok(()) => DriverError::Transaction {
                            statement_index: idx,
                            source: Box::new(source),
                        },
                        Err(error) => DriverError::TransactionRollbackFailed {
                            statement_index: idx,
                            source: Box::new(source),
                            rollback_error: Box::new(map_sqlx_error(error)),
                        },
                    });
                }
                affected.push(total);
            }
            Err(error) => {
                let _ = tx.rollback().await;
                return Err(DriverError::Transaction {
                    statement_index: idx,
                    source: Box::new(map_sqlx_error(error)),
                });
            }
        }
    }
    if stop_requested() {
        return Err(stopped(tx, statements.len()).await);
    }
    tx.commit().await.map_err(map_sqlx_error)?;
    Ok(affected)
}

async fn stopped(tx: sqlx::Transaction<'_, Sqlite>, statement_index: usize) -> DriverError {
    match tx.rollback().await {
        Ok(()) => DriverError::Transaction {
            statement_index,
            source: Box::new(DriverError::Cancelled),
        },
        Err(error) => DriverError::TransactionRollbackFailed {
            statement_index,
            source: Box::new(DriverError::Cancelled),
            rollback_error: Box::new(map_sqlx_error(error)),
        },
    }
}
