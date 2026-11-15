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
