use std::{collections::HashMap, iter};

use futures::stream::StreamExt;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgRow, PgTypeInfo, PgTypeKind};
use sqlx::{Column, Connection, Executor, Pool, Postgres, Row, SqlSafeStr, Statement, TypeInfo, ValueRef};
use tablepro_core::{ColumnInfo, DriverError, ExecResult, MAX_QUERY_ROWS, QueryResult, QueryResultBudget, Value};

use super::params::{bind_pg_params, describe_query_parameters, needs_enum_type_inference, pg_parameter_type_infos};
use super::{array, decode, map_sqlx_error, numeric, temporal};

pub(super) async fn query_connection(
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

pub(super) async fn query_connection_once(
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

pub(super) async fn execute_connection(
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

pub(super) async fn execute_connection_once(
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

pub(super) async fn stream_into_result(
    pool: &Pool<Postgres>,
    sql: &str,
    limit: usize,
) -> Result<QueryResult, DriverError> {
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

pub(super) fn statement_columns(columns: &[sqlx::postgres::PgColumn]) -> Vec<ColumnInfo> {
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

pub(super) fn statement_type_infos(columns: &[sqlx::postgres::PgColumn]) -> Vec<PgTypeInfo> {
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

pub(super) fn undecodable(idx: usize, type_name: &str) -> Value {
    tracing::warn!(
        column_index = idx + 1,
        type_name,
        "postgres column value could not be decoded; showing it as undecodable"
    );
    Value::Undecodable(type_name.to_string())
}
