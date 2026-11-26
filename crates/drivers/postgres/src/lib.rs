use std::{future::Future, time::Duration};

use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgCancelToken, PgConnectOptions};
use sqlx::{Connection as SqlxConnection, Pool, Postgres, Row};

use bookie_core::sql_dialect::{qualified_table, quote_ident};
use bookie_core::sql_syntax::script::LexicalSettings;
use bookie_core::{
    CANCELLATION_DISPATCH_TIMEOUT, CONTROL_SETUP_TIMEOUT, ColumnInfo, CommitResolution, CommitToken, DriverError,
    ExecResult, ForeignKeyInfo, InboundForeignKeyInfo, IndexInfo, MAX_QUERY_ROWS, OperationControl, QualifiedTypeName,
    QueryResult, QueryResultBatch, SchemaTableMetadataVisitor, ServerMessages, TableInfo, Transport, Value,
    check_pre_dispatch, run_controlled_setup, run_server_cancellable,
    run_server_cancellable_preserving_transaction_rollback,
};

pub use notifications::{
    MAX_SAMPLE_MESSAGES, NotificationMessage, NotificationSample, check_notification_request, follow_notifications,
    sample_notifications,
};

mod array;
mod catalog;
mod commit_resolution;
mod connection;
mod copy_in;
mod decode;
mod driver;
mod errors;
mod introspection;
mod notices;
mod notifications;
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
mod undecodable_counts;
mod view_editability;

pub use driver::PgDriver;
pub(crate) use driver::full_connect_options;
#[cfg(test)]
pub(crate) use driver::postgres_connect_options;

use errors::*;
use introspection::*;
use params::{bind_pg_params, describe_query_parameters, needs_text_type_inference};
use query::{
    QueryStop, execute_connection, execute_connection_once, query_connection, query_connection_once,
    query_connection_once_with_rows_affected, query_connection_with_rows_affected, stream_into_result,
};

struct PgConnection {
    pool: Pool<Postgres>,
    session_options: PgConnectOptions,
}

struct PgTransaction {
    connection: Option<PoolConnection<Postgres>>,
    cancel_token: PgCancelToken,
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
        controlled_query_with_messages(connection, sql, params, retry_stale_type_cache, true, control).await;
    (connection, result.map(|(rows, _, _)| rows))
}

pub(crate) type MessagedResult = Result<(QueryResult, ServerMessages, Option<u64>), DriverError>;

async fn controlled_query_with_messages(
    mut connection: PoolConnection<Postgres>,
    sql: &str,
    params: &[Value],
    retry_stale_type_cache: bool,
    log_decoding: bool,
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
            } else if log_decoding {
                query_connection_once_with_rows_affected(&mut connection, sql, params, stop).await
            } else {
                query::query_connection_once_unlogged(&mut connection, sql, params, stop).await
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
    let target = qualified_table("postgres", schema, table);
    let column_ident = quote_ident("postgres", column);
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
