use bookie_core::{
    ColumnInfo, DriverError, QueryResult, QueryResultBatch, QueryResultBudget, QueryRows, ServerMessages,
};
use futures::{Stream, TryStreamExt};
use sqlparser::ast::Statement as SqlStatement;
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;
use sqlx::Either;
use sqlx::mysql::{MySql, MySqlColumn, MySqlQueryResult, MySqlRow};
use sqlx::pool::PoolConnection;
use sqlx::{Column, Row, TypeInfo};

use crate::decode::{ColumnDecoder, column_decoders, decode_row_into};

const MAX_MYSQL_RESULT_SETS: usize = 256;

pub(crate) async fn text_protocol(
    connection: &mut PoolConnection<MySql>,
    sql: &str,
) -> Result<QueryResultBatch, DriverError> {
    let mut stream = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string())).fetch_many(&mut **connection);
    let collected = collect_result_sets(&mut stream, QueryResultBudget::default(), false).await;
    drop(stream);
    let (result_sets, truncated) = collected?;
    crate::forget_statements_after_use(connection, sql)
        .await
        .map_err(crate::map_sqlx_error)?;
    Ok(result_batch(result_sets, truncated))
}

pub(crate) async fn bounded_text_protocol(
    connection: &mut PoolConnection<MySql>,
    sql: &str,
    max_rows: usize,
    max_bytes: usize,
) -> Result<QueryResultBatch, DriverError> {
    let mut stream = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string())).fetch_many(&mut **connection);
    let budget = QueryResultBudget::with_limits(max_rows, max_bytes);
    let collected = collect_result_sets(&mut stream, budget, true).await;
    drop(stream);
    let (result_sets, truncated) = collected?;
    if !truncated {
        crate::forget_statements_after_use(connection, sql)
            .await
            .map_err(crate::map_sqlx_error)?;
    }
    Ok(result_batch(result_sets, truncated))
}

pub(crate) fn result_batch(result_sets: Vec<QueryResult>, truncated: bool) -> QueryResultBatch {
    QueryResultBatch {
        result_sets,
        truncated,
        messages: ServerMessages::default(),
        rows_affected: None,
    }
}

pub(crate) async fn collect_result_sets<S>(
    stream: &mut S,
    mut budget: QueryResultBudget,
    stop_on_truncation: bool,
) -> Result<(Vec<QueryResult>, bool), DriverError>
where
    S: Stream<Item = Result<Either<MySqlQueryResult, MySqlRow>, sqlx::Error>> + Unpin,
{
    let mut result_sets = Vec::new();
    let mut current = None;
    let mut decoders = Vec::new();
    let mut truncated = false;
    let mut decoded = Vec::new();
    while let Some(item) = stream.try_next().await.map_err(crate::map_sqlx_error)? {
        match item {
            Either::Right(row) => collect_row(
                row,
                &mut current,
                &mut decoders,
                &result_sets,
                &mut budget,
                &mut truncated,
                &mut decoded,
            )?,
            Either::Left(_) => finish_result_set(&mut current, &mut decoders, &mut result_sets, &mut truncated),
        }
        if truncated && stop_on_truncation {
            break;
        }
    }
    finish_result_set(&mut current, &mut decoders, &mut result_sets, &mut truncated);
    Ok((result_sets, truncated))
}

fn collect_row(
    row: MySqlRow,
    current: &mut Option<QueryResult>,
    decoders: &mut Vec<ColumnDecoder>,
    result_sets: &[QueryResult],
    budget: &mut QueryResultBudget,
    truncated: &mut bool,
    decoded: &mut Vec<bookie_core::Value>,
) -> Result<(), DriverError> {
    if current.is_none() && result_sets.len() >= MAX_MYSQL_RESULT_SETS {
        mark_truncated(current, truncated);
        return Ok(());
    }
    if current.is_none() {
        *current = Some(QueryResult {
            columns: statement_columns(row.columns()),
            rows: QueryRows::new(),
            truncated: false,
        });
        *decoders = column_decoders(row.columns());
    }
    if !decode_row_into(&row, decoders, budget, decoded)? {
        mark_truncated(current, truncated);
        return Ok(());
    }
    if let Some(result_set) = current {
        result_set.rows.push_row(decoded);
    }
    Ok(())
}

fn finish_result_set(
    current: &mut Option<QueryResult>,
    decoders: &mut Vec<ColumnDecoder>,
    result_sets: &mut Vec<QueryResult>,
    truncated: &mut bool,
) {
    decoders.clear();
    if let Some(result_set) = current.take() {
        if result_sets.len() < MAX_MYSQL_RESULT_SETS {
            result_sets.push(result_set);
        } else {
            *truncated = true;
        }
    }
}

fn mark_truncated(current: &mut Option<QueryResult>, truncated: &mut bool) {
    *truncated = true;
    if let Some(result_set) = current {
        result_set.truncated = true;
    }
}

pub(crate) fn contains_use_statement(sql: &str) -> bool {
    match Parser::parse_sql(&MySqlDialect {}, sql) {
        Ok(statements) => statements
            .iter()
            .any(|statement| matches!(statement, SqlStatement::Use(_))),
        Err(_) => sql
            .trim_start()
            .get(..3)
            .is_some_and(|head| head.eq_ignore_ascii_case("use")),
    }
}

pub(crate) fn is_call_statement(sql: &str) -> bool {
    Parser::parse_sql(&MySqlDialect {}, sql)
        .is_ok_and(|statements| matches!(statements.as_slice(), [SqlStatement::Call(_)]))
}

pub(super) fn statement_columns(columns: &[MySqlColumn]) -> Vec<ColumnInfo> {
    columns
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
            domain_type: None,
        })
        .collect()
}
