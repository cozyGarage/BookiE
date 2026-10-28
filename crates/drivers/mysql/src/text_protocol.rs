use futures::TryStreamExt;
use sqlparser::ast::Statement as SqlStatement;
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;
use sqlx::mysql::{MySql, MySqlColumn, MySqlRow};
use sqlx::pool::PoolConnection;
use sqlx::{Column, Row, TypeInfo, ValueRef};
use tablepro_core::{ColumnInfo, DriverError, MAX_QUERY_ROWS, QueryResult, QueryResultBatch, QueryResultBudget};

use crate::decode::extract_value;

const MAX_MYSQL_RESULT_SETS: usize = 256;

pub(crate) async fn text_protocol(
    connection: &mut PoolConnection<MySql>,
    sql: &str,
) -> Result<QueryResultBatch, DriverError> {
    use sqlx::Either;

    let mut stream = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string())).fetch_many(&mut **connection);
    let mut result_sets = Vec::new();
    let mut current = None;
    let mut budget = QueryResultBudget::default();
    let mut retained_rows = 0usize;
    let mut truncated = false;
    while let Some(item) = stream.try_next().await.map_err(crate::map_sqlx_error)? {
        match item {
            Either::Right(row) => collect_row(
                row,
                &mut current,
                &result_sets,
                &mut budget,
                &mut retained_rows,
                &mut truncated,
            )?,
            Either::Left(_) => finish_result_set(&mut current, &mut result_sets, &mut truncated),
        }
    }
    finish_result_set(&mut current, &mut result_sets, &mut truncated);
    Ok(QueryResultBatch { result_sets, truncated })
}

fn collect_row(
    row: MySqlRow,
    current: &mut Option<QueryResult>,
    result_sets: &[QueryResult],
    budget: &mut QueryResultBudget,
    retained_rows: &mut usize,
    truncated: &mut bool,
) -> Result<(), DriverError> {
    if current.is_none() && result_sets.len() >= MAX_MYSQL_RESULT_SETS {
        mark_truncated(current, truncated);
        return Ok(());
    }
    if current.is_none() {
        *current = Some(QueryResult {
            columns: statement_columns(row.columns()),
            rows: Vec::new(),
            truncated: false,
        });
    }
    if *retained_rows >= MAX_QUERY_ROWS {
        mark_truncated(current, truncated);
        return Ok(());
    }
    let Some(values) = budget.decode_and_admit_row(row.len(), mysql_row_payload_bytes(&row), || {
        Ok::<_, DriverError>((0..row.len()).map(|index| extract_value(&row, index)).collect())
    })?
    else {
        mark_truncated(current, truncated);
        return Ok(());
    };
    if let Some(result_set) = current {
        result_set.rows.push(values);
        *retained_rows += 1;
    }
    Ok(())
}

fn finish_result_set(current: &mut Option<QueryResult>, result_sets: &mut Vec<QueryResult>, truncated: &mut bool) {
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

pub(super) fn mysql_row_payload_bytes(row: &MySqlRow) -> usize {
    (0..row.len()).fold(0usize, |total, index| {
        let length = match row.try_get_raw(index) {
            Ok(value) if value.is_null() => 0,
            Ok(value) => <&[u8] as sqlx::Decode<MySql>>::decode(value).map_or(usize::MAX, <[u8]>::len),
            Err(_) => usize::MAX,
        };
        total.saturating_add(length)
    })
}
