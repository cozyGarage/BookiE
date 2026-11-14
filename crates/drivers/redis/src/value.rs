use super::*;
use redis::AsyncCommands;

pub(super) struct RedisValueQuery {
    database: u8,
    key: Vec<u8>,
}

pub(super) async fn browse_connection(
    connection: &RedisConnection,
    database: u8,
) -> Result<redis::aio::MultiplexedConnection, DriverError> {
    let mut conn = tokio::time::timeout(
        CONNECT_TIMEOUT,
        connection.browse_client.get_multiplexed_async_connection(),
    )
    .await
    .map_err(|_| DriverError::TimedOut)?
    .map_err(map_redis_error)?;
    redis::cmd("SELECT")
        .arg(database)
        .query_async::<()>(&mut conn)
        .await
        .map_err(map_redis_error)?;
    Ok(conn)
}

pub(super) fn parse_value_query(sql: &str, params: &[Value]) -> Option<RedisValueQuery> {
    let rest = sql.trim().strip_prefix("SELECT \"Value\" FROM \"db")?;
    let (database, suffix) = rest.split_once("\" WHERE \"Key\" = ?")?;
    if !suffix.is_empty()
        || params.len() != 1
        || database.is_empty()
        || !database.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let database = database.parse().ok()?;
    let key = match &params[0] {
        Value::Text(text) => text.as_bytes().to_vec(),
        Value::Bytes(bytes) => bytes.clone(),
        _ => return None,
    };
    Some(RedisValueQuery { database, key })
}

pub(super) async fn fetch_string_value(
    connection: &RedisConnection,
    query: RedisValueQuery,
) -> Result<QueryResult, DriverError> {
    let mut conn = browse_connection(connection, query.database).await?;
    let key_type: String = conn.key_type(&query.key).await.map_err(map_redis_error)?;
    if key_type == "none" {
        return Ok(empty_value_result());
    }
    if key_type != "string" {
        return Err(DriverError::Unsupported(
            "Redis collection values cannot be fetched with a hard byte bound".into(),
        ));
    }
    let max_bytes = u64::try_from(MAX_QUERY_RESULT_BYTES)
        .map_err(|_| DriverError::Unsupported("Redis value limit exceeds the protocol range".into()))?;
    let length: u64 = redis::cmd("STRLEN")
        .arg(&query.key)
        .query_async(&mut conn)
        .await
        .map_err(map_redis_error)?;
    if length > max_bytes {
        return Err(DriverError::Unsupported(
            "Redis value exceeds the 64 MiB query limit".into(),
        ));
    }
    let range_end = i64::try_from(MAX_QUERY_RESULT_BYTES - 1)
        .map_err(|_| DriverError::Unsupported("Redis value limit exceeds the protocol range".into()))?;
    let (exists, current_length, bytes): (u8, u64, Option<Vec<u8>>) = redis::pipe()
        .atomic()
        .cmd("EXISTS")
        .arg(&query.key)
        .cmd("STRLEN")
        .arg(&query.key)
        .cmd("GETRANGE")
        .arg(&query.key)
        .arg(0)
        .arg(range_end)
        .query_async(&mut conn)
        .await
        .map_err(map_redis_error)?;
    if exists == 0 {
        return Ok(empty_value_result());
    }
    let Some(bytes) = bytes else {
        if current_length == 0 {
            return Ok(empty_value_result());
        }
        return Err(changed_value_error());
    };
    if current_length > max_bytes || usize::try_from(current_length).ok() != Some(bytes.len()) {
        return Err(changed_value_error());
    }
    Ok(QueryResult {
        columns: vec![text_col("Value")],
        rows: vec![vec![bytes_to_value(bytes)]].into(),
        truncated: false,
    })
}

fn empty_value_result() -> QueryResult {
    QueryResult {
        columns: vec![text_col("Value")],
        rows: Vec::new().into(),
        truncated: false,
    }
}

fn changed_value_error() -> DriverError {
    DriverError::Unsupported("Redis value changed while it was being read".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_queries_accept_only_bound_text_or_binary_keys_for_known_databases() {
        let key = Value::Bytes(vec![0xff, b'?', b'\n']);
        let parsed = parse_value_query(
            "SELECT \"Value\" FROM \"db12\" WHERE \"Key\" = ?",
            std::slice::from_ref(&key),
        )
        .expect("valid value query");
        assert_eq!(parsed.database, 12);
        assert_eq!(parsed.key, vec![0xff, b'?', b'\n']);

        for (sql, params) in [
            (
                "SELECT \"Value\" FROM \"db12\" WHERE \"Key\" = ? OR 1=1",
                vec![Value::Text("x".into())],
            ),
            (
                "SELECT \"Type\" FROM \"db12\" WHERE \"Key\" = ?",
                vec![Value::Text("x".into())],
            ),
            (
                "SELECT \"Value\" FROM \"db256\" WHERE \"Key\" = ?",
                vec![Value::Text("x".into())],
            ),
            ("SELECT \"Value\" FROM \"db12\" WHERE \"Key\" = ?", vec![Value::Int(1)]),
        ] {
            assert!(parse_value_query(sql, &params).is_none(), "accepted {sql}");
        }
    }
}
