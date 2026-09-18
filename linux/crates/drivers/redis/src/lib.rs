use std::time::Duration;

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client, RedisError, TlsCertificates, Value as RedisValue};
use secrecy::ExposeSecret;
use tokio::sync::Mutex;

use tablepro_core::{
    ColumnInfo, ConnectOptions, Connection, DatabaseDriver, DriverError, DriverMaturity, ExecResult, MAX_QUERY_ROWS,
    QueryResult, TableInfo, Value, error_chain_text, looks_like_tls_failure,
};

pub struct RedisDriver;

#[async_trait]
impl DatabaseDriver for RedisDriver {
    fn id(&self) -> &'static str {
        "redis"
    }

    fn display_name(&self) -> &'static str {
        "Redis"
    }

    fn maturity(&self) -> DriverMaturity {
        DriverMaturity::Experimental
    }

    fn default_port(&self) -> u16 {
        6379
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        let scheme = match opts.tls.mode {
            tablepro_core::TlsMode::Disabled => "redis",
            _ => "rediss",
        };
        let password = opts.password.expose_secret();
        let auth = if !opts.username.is_empty() && !password.is_empty() {
            format!("{}:{}@", urlencoding_lite(&opts.username), urlencoding_lite(password))
        } else if !password.is_empty() {
            format!(":{}@", urlencoding_lite(password))
        } else {
            String::new()
        };
        let db_index = parse_db_index(&opts.database)?;
        let verifies = opts.tls.mode.verifies_cert();
        let suffix = if opts.tls.mode.encrypts() && !verifies {
            "#insecure"
        } else {
            ""
        };
        let url = format!("{scheme}://{auth}{}:{}/{}{suffix}", opts.host, opts.port, db_index);
        let client = match root_certificate(&opts.tls, verifies)? {
            Some(root_cert) => Client::build_with_tls(
                url,
                TlsCertificates {
                    client_tls: None,
                    root_cert: Some(root_cert),
                },
            )
            .map_err(|err| map_redis_connect_error(err, verifies))?,
            None => Client::open(url).map_err(|err| map_redis_connect_error(err, verifies))?,
        };
        let manager = match tokio::time::timeout(CONNECT_TIMEOUT, establish_connection_manager(client.clone())).await {
            Ok(result) => result.map_err(|err| map_redis_connect_error(err, verifies))?,
            Err(_) => return Err(DriverError::ConnectionRefused),
        };
        Ok(Box::new(RedisConnection {
            conn: Mutex::new(manager),
            db_count: 16,
            browse_client: client,
        }))
    }
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Read the certificate authority the connection names. Only a verifying mode
/// consults it, so an encrypt-only session never fails on a path it ignores.
fn root_certificate(config: &tablepro_core::TlsConfig, verifies: bool) -> Result<Option<Vec<u8>>, DriverError> {
    if !verifies {
        return Ok(None);
    }
    let Some(path) = &config.root_cert else {
        return Ok(None);
    };
    std::fs::read(path).map(Some).map_err(|error| {
        DriverError::Tls(format!(
            "cannot read the certificate authority at {}: {error}",
            path.display()
        ))
    })
}

struct RedisConnection {
    conn: Mutex<ConnectionManager>,
    db_count: usize,
    // Never clone the normal ConnectionManager for browsing: clones share SELECT state.
    browse_client: Client,
}

fn redis_columns() -> Vec<ColumnInfo> {
    vec![
        ColumnInfo {
            name: "Key".into(),
            data_type: "string".into(),
            nullable: false,
            primary_key: true,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        },
        ColumnInfo {
            name: "Type".into(),
            data_type: "string".into(),
            nullable: false,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        },
        ColumnInfo {
            name: "TTL".into(),
            data_type: "integer".into(),
            nullable: false,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        },
        ColumnInfo {
            name: "Value".into(),
            data_type: "string".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        },
    ]
}

#[async_trait]
impl Connection for RedisConnection {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        let mut conn = self.conn.lock().await;
        let count = match redis::cmd("CONFIG")
            .arg("GET")
            .arg("databases")
            .query_async::<Vec<String>>(&mut *conn)
            .await
        {
            Ok(pairs) => pairs
                .get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(self.db_count),
            Err(_) => self.db_count,
        };
        Ok((0..count)
            .map(|i| TableInfo {
                schema: None,
                name: format!("db{i}"),
            })
            .collect())
    }

    async fn fetch_columns(&self, _schema: Option<&str>, _table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        Ok(redis_columns())
    }

    async fn fetch_rows(
        &self,
        _schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<QueryResult, DriverError> {
        let db = parse_db_name(table)?;
        // A non-reconnecting, operation-local connection cannot leak SELECT state
        // into commands, or reconnect mid-page onto the configured database.
        let mut conn = tokio::time::timeout(CONNECT_TIMEOUT, self.browse_client.get_multiplexed_async_connection())
            .await
            .map_err(|_| DriverError::TimedOut)?
            .map_err(map_redis_error)?;
        redis::cmd("SELECT")
            .arg(db)
            .query_async::<()>(&mut conn)
            .await
            .map_err(map_redis_error)?;
        fetch_page(&mut conn, offset, limit).await
    }

    async fn query(&self, sql: &str) -> Result<QueryResult, DriverError> {
        let trimmed = sql.trim();
        if trimmed.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                truncated: false,
            });
        }
        let mut conn = self.conn.lock().await;
        let args = split_redis_cli(trimmed);
        if args.is_empty() {
            return Err(DriverError::Query {
                message: "empty Redis command".into(),
                sqlstate: None,
            });
        }
        let mut cmd = redis::cmd(&args[0]);
        for arg in &args[1..] {
            cmd.arg(arg);
        }
        let value: RedisValue = cmd.query_async(&mut *conn).await.map_err(map_redis_error)?;
        Ok(redis_value_to_result(value))
    }

    async fn execute(&self, sql: &str) -> Result<ExecResult, DriverError> {
        let result = self.query(sql).await?;
        Ok(ExecResult {
            rows_affected: result.rows.len() as u64,
        })
    }

    async fn execute_params(&self, sql: &str, params: &[Value]) -> Result<ExecResult, DriverError> {
        if params.is_empty() {
            return self.execute(sql).await;
        }
        Err(DriverError::Unsupported(
            "Redis execute_params does not support bound parameters".into(),
        ))
    }

    async fn execute_in_transaction(&self, statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        let mut affected = Vec::with_capacity(statements.len());
        for (idx, (sql, params)) in statements.iter().enumerate() {
            match self.execute_params(sql, params).await {
                Ok(res) => affected.push(res.rows_affected),
                Err(e) => {
                    return Err(DriverError::Transaction {
                        statement_index: idx,
                        source: Box::new(e),
                    });
                }
            }
        }
        Ok(affected)
    }

    async fn ping(&self) -> Result<(), DriverError> {
        let mut conn = self.conn.lock().await;
        redis::cmd("PING")
            .query_async::<String>(&mut *conn)
            .await
            .map_err(map_redis_error)?;
        Ok(())
    }

    async fn server_version(&self) -> Result<Option<String>, DriverError> {
        let mut conn = self.conn.lock().await;
        let info: String = redis::cmd("INFO")
            .arg("server")
            .query_async(&mut *conn)
            .await
            .map_err(map_redis_error)?;
        let version = info
            .lines()
            .find_map(|line| line.strip_prefix("redis_version:").map(str::trim).map(str::to_string));
        Ok(version.map(|v| format!("Redis {v}")))
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

async fn fetch_page(
    conn: &mut redis::aio::MultiplexedConnection,
    offset: u64,
    limit: u64,
) -> Result<QueryResult, DriverError> {
    let keys = scan_keys(conn, "*", (offset + limit) as usize).await?;
    let skip = offset as usize;
    let page: Vec<String> = keys.into_iter().skip(skip).take(limit as usize).collect();
    let mut rows = Vec::with_capacity(page.len());
    for key in page {
        rows.push(key_row(conn, &key).await?);
    }
    Ok(QueryResult {
        columns: redis_columns(),
        rows,
        truncated: false,
    })
}

async fn scan_keys(
    conn: &mut redis::aio::MultiplexedConnection,
    pattern: &str,
    limit: usize,
) -> Result<Vec<String>, DriverError> {
    let mut cursor: u64 = 0;
    let mut keys = Vec::new();
    loop {
        let (next, batch): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(conn)
            .await
            .map_err(map_redis_error)?;
        keys.extend(batch);
        cursor = next;
        if cursor == 0 || keys.len() >= limit {
            break;
        }
    }
    keys.truncate(limit);
    Ok(keys)
}

async fn key_row(conn: &mut redis::aio::MultiplexedConnection, key: &str) -> Result<Vec<Value>, DriverError> {
    let key_type: String = conn.key_type(key).await.map_err(map_redis_error)?;
    let ttl: i64 = conn.ttl(key).await.map_err(map_redis_error)?;
    let preview = match key_type.as_str() {
        "string" => {
            let v: String = conn.get(key).await.map_err(map_redis_error)?;
            v
        }
        "hash" => {
            let v: Vec<(String, String)> = conn.hgetall(key).await.map_err(map_redis_error)?;
            format!("{v:?}")
        }
        "list" => {
            let v: Vec<String> = conn.lrange(key, 0, 20).await.map_err(map_redis_error)?;
            format!("{v:?}")
        }
        "set" => {
            let v: Vec<String> = conn.smembers(key).await.map_err(map_redis_error)?;
            format!("{v:?}")
        }
        "zset" => {
            let v: Vec<String> = conn.zrange(key, 0, 20).await.map_err(map_redis_error)?;
            format!("{v:?}")
        }
        other => format!("<{other}>"),
    };
    Ok(vec![
        Value::Text(key.to_string()),
        Value::Text(key_type),
        Value::Int(ttl),
        Value::Text(preview),
    ])
}

fn redis_value_to_result(value: RedisValue) -> QueryResult {
    match value {
        RedisValue::Nil => QueryResult {
            columns: vec![text_col("result")],
            rows: vec![vec![Value::Null]],
            truncated: false,
        },
        RedisValue::Okay => QueryResult {
            columns: vec![text_col("result")],
            rows: vec![vec![Value::Text("OK".into())]],
            truncated: false,
        },
        RedisValue::SimpleString(s) => QueryResult {
            columns: vec![text_col("result")],
            rows: vec![vec![Value::Text(s)]],
            truncated: false,
        },
        RedisValue::Int(i) => QueryResult {
            columns: vec![ColumnInfo {
                name: "result".into(),
                data_type: "integer".into(),
                nullable: false,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
            }],
            rows: vec![vec![Value::Int(i)]],
            truncated: false,
        },
        RedisValue::Double(f) => QueryResult {
            columns: vec![ColumnInfo {
                name: "result".into(),
                data_type: "double".into(),
                nullable: false,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
            }],
            rows: vec![vec![Value::Float(f)]],
            truncated: false,
        },
        RedisValue::Boolean(b) => QueryResult {
            columns: vec![ColumnInfo {
                name: "result".into(),
                data_type: "boolean".into(),
                nullable: false,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
            }],
            rows: vec![vec![Value::Bool(b)]],
            truncated: false,
        },
        RedisValue::BulkString(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            QueryResult {
                columns: vec![text_col("result")],
                rows: vec![vec![Value::Text(text)]],
                truncated: false,
            }
        }
        RedisValue::Array(items) | RedisValue::Set(items) => {
            let mut rows = Vec::new();
            let mut truncated = false;
            for item in items {
                if rows.len() >= MAX_QUERY_ROWS {
                    truncated = true;
                    break;
                }
                rows.push(vec![redis_scalar(item)]);
            }
            QueryResult {
                columns: vec![text_col("value")],
                rows,
                truncated,
            }
        }
        RedisValue::Map(pairs) => {
            let rows = pairs
                .into_iter()
                .take(MAX_QUERY_ROWS)
                .map(|(k, v)| vec![redis_scalar(k), redis_scalar(v)])
                .collect();
            QueryResult {
                columns: vec![text_col("key"), text_col("value")],
                rows,
                truncated: false,
            }
        }
        RedisValue::VerbatimString { text, .. } => QueryResult {
            columns: vec![text_col("result")],
            rows: vec![vec![Value::Text(text)]],
            truncated: false,
        },
        other => QueryResult {
            columns: vec![text_col("result")],
            rows: vec![vec![Value::Text(format!("{other:?}"))]],
            truncated: false,
        },
    }
}

fn text_col(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "string".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
    }
}

fn redis_scalar(value: RedisValue) -> Value {
    match value {
        RedisValue::Nil => Value::Null,
        RedisValue::Int(i) => Value::Int(i),
        RedisValue::Double(f) => Value::Float(f),
        RedisValue::Boolean(b) => Value::Bool(b),
        RedisValue::SimpleString(s) => Value::Text(s),
        RedisValue::BulkString(b) => Value::Text(String::from_utf8_lossy(&b).into_owned()),
        RedisValue::Okay => Value::Text("OK".into()),
        RedisValue::VerbatimString { text, .. } => Value::Text(text),
        other => Value::Text(format!("{other:?}")),
    }
}

fn parse_db_name(table: &str) -> Result<u8, DriverError> {
    let stripped = table.strip_prefix("db").unwrap_or(table);
    stripped.parse::<u8>().map_err(|_| DriverError::Query {
        message: format!("invalid Redis database name: {table}"),
        sqlstate: None,
    })
}

/// Minimal Redis CLI tokenizer: splits on whitespace, respects double quotes.
pub fn split_redis_cli(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => in_quotes = !in_quotes,
            '\\' if in_quotes => {
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// An empty database means "use Redis's default (0)". Anything else must be
/// a plain non-negative integer -- `"12 "`, `"db3"` and similar previously
/// fell back to db 0 silently, which is usually the production database.
fn parse_db_index(database: &str) -> Result<u8, DriverError> {
    let trimmed = database.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    trimmed
        .parse::<u8>()
        .map_err(|_| DriverError::Unsupported(format!("'{database}' is not a valid Redis database index")))
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn establish_connection_manager(client: Client) -> Result<ConnectionManager, RedisError> {
    client.get_multiplexed_async_connection().await?;
    ConnectionManager::new(client).await
}

fn redis_error_can_hide_tls(err: &RedisError) -> bool {
    matches!(err.kind(), redis::ErrorKind::Io | redis::ErrorKind::Client)
        || err.is_connection_refusal()
        || err.is_connection_dropped()
        || err.is_timeout()
}

fn map_redis_error(err: RedisError) -> DriverError {
    map_redis_connect_error(err, false)
}

fn map_redis_connect_error(err: RedisError, verifies_cert: bool) -> DriverError {
    let chain = error_chain_text(&err);
    if redis_error_can_hide_tls(&err) && looks_like_tls_failure(&chain) {
        return DriverError::Tls(chain);
    }
    let msg = err.to_string();
    if msg.contains("Connection refused") || err.is_connection_refusal() {
        DriverError::ConnectionRefused
    } else if msg.contains("NOAUTH") || msg.contains("WRONGPASS") || msg.contains("invalid password") {
        DriverError::AuthFailed
    } else if verifies_cert && err.is_connection_dropped() {
        DriverError::Tls("certificate hostname mismatch; connection closed during TLS verification".into())
    } else {
        DriverError::Query {
            message: msg,
            sqlstate: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure_metadata_is_not_declared_without_a_fetch() {
        let d = RedisDriver;
        let source = include_str!("lib.rs");
        assert!(!d.supports_index_metadata());
        assert!(!d.supports_foreign_key_metadata());
        assert!(!source.contains(&["async fn ", "fetch_indexes("].concat()));
        assert!(!source.contains(&["async fn ", "fetch_foreign_keys("].concat()));
    }

    #[test]
    fn an_empty_database_defaults_to_zero() {
        assert_eq!(parse_db_index("").unwrap(), 0);
        assert_eq!(parse_db_index("   ").unwrap(), 0);
    }

    #[test]
    fn a_valid_index_is_used_even_with_surrounding_whitespace() {
        assert_eq!(parse_db_index("12").unwrap(), 12);
        assert_eq!(parse_db_index(" 12 ").unwrap(), 12);
    }

    /// A typo or stray text must refuse the connection, not silently land
    /// on db 0 -- usually the production database.
    #[test]
    fn a_malformed_database_index_is_refused_instead_of_defaulting_to_zero() {
        for bad in ["db3", "16abc", "-1", "1.5"] {
            assert!(parse_db_index(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn driver_metadata() {
        let d = RedisDriver;
        assert_eq!(d.id(), "redis");
        assert_eq!(d.display_name(), "Redis");
        assert_eq!(d.default_port(), 6379);
    }

    #[test]
    fn split_redis_cli_handles_quotes() {
        assert_eq!(split_redis_cli("GET foo"), vec!["GET", "foo"]);
        assert_eq!(
            split_redis_cli(r#"SET key "hello world""#),
            vec!["SET", "key", "hello world"]
        );
        assert_eq!(split_redis_cli(r#"SET k "a\"b""#), vec!["SET", "k", "a\"b"]);
    }

    #[test]
    fn split_redis_cli_keeps_a_backslash_literal_outside_quotes() {
        assert_eq!(split_redis_cli(r"SET k a\b"), vec!["SET", "k", r"a\b"]);
    }

    #[test]
    fn parse_db_name_accepts_db_n() {
        assert_eq!(parse_db_name("db0").unwrap(), 0);
        assert_eq!(parse_db_name("db15").unwrap(), 15);
        assert!(parse_db_name("users").is_err());
    }

    #[test]
    fn urlencoding_lite_keeps_unreserved_characters_and_encodes_everything_else() {
        assert_eq!(urlencoding_lite("Az09-_.~"), "Az09-_.~");
        assert_eq!(urlencoding_lite("a b"), "a%20b");
        assert_eq!(urlencoding_lite("p@ss/w:rd"), "p%40ss%2Fw%3Ard");
    }

    #[test]
    fn map_redis_error_recognizes_a_connection_refusal() {
        let err = RedisError::from(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        assert!(matches!(map_redis_error(err), DriverError::ConnectionRefused));
    }

    #[test]
    fn a_plain_connection_refused_stays_connection_refused_when_verifying() {
        let err = RedisError::from(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        assert!(matches!(
            map_redis_connect_error(err, true),
            DriverError::ConnectionRefused
        ));
    }

    #[test]
    fn a_certificate_name_mismatch_io_error_maps_to_tls() {
        let err = RedisError::from(std::io::Error::other(
            "invalid peer certificate: certificate not valid for name \"127.0.0.1\"",
        ));
        assert!(matches!(map_redis_error(err), DriverError::Tls(detail) if detail.contains("certificate")));
    }

    #[test]
    fn a_connection_refused_carrying_a_name_mismatch_maps_to_tls() {
        let err = RedisError::from(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "invalid peer certificate: certificate not valid for name \"127.0.0.1\"",
        ));
        let mapped = map_redis_connect_error(err, true);
        assert!(matches!(mapped, DriverError::Tls(detail) if detail.contains("certificate")));
    }

    #[test]
    fn verifying_connect_does_not_report_a_hostname_mismatch_as_a_refusal() {
        let err = RedisError::from(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "invalid peer certificate: NotValidForName",
        ));
        let mapped = map_redis_connect_error(err, true);
        assert!(matches!(
            mapped,
            DriverError::Tls(detail)
                if detail.to_ascii_lowercase().contains("certificate")
                    || detail.to_ascii_lowercase().contains("notvalidforname")
        ));
    }

    #[test]
    fn rustls_invalid_data_name_mismatch_maps_to_tls() {
        let err = RedisError::from(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid peer certificate: NotValidForName",
        ));
        let mapped = map_redis_connect_error(err, true);
        assert!(matches!(
            mapped,
            DriverError::Tls(detail) if detail.to_ascii_lowercase().contains("notvalidforname")
        ));
    }

    #[test]
    fn verifying_connect_maps_a_dropped_handshake_without_tls_text() {
        let err = RedisError::from(std::io::Error::from(std::io::ErrorKind::ConnectionReset));
        assert!(matches!(map_redis_error(err.clone()), DriverError::Query { .. }));
        let mapped = map_redis_connect_error(err, true);
        assert!(
            matches!(mapped, DriverError::Tls(detail) if detail.contains("certificate") && detail.contains("hostname"))
        );
    }

    #[test]
    fn looks_like_tls_failure_reads_rustls_identity_text() {
        assert!(looks_like_tls_failure(
            "invalid peer certificate: certificate not valid for name \"127.0.0.1\""
        ));
        assert!(looks_like_tls_failure("invalid peer certificate: NotValidForName"));
        assert!(!looks_like_tls_failure("Connection refused"));
    }

    #[test]
    fn map_redis_error_recognizes_noauth_and_wrongpass_as_auth_failures() {
        let noauth = RedisError::from((
            redis::ErrorKind::AuthenticationFailed,
            "authentication required",
            "NOAUTH Authentication required.".to_string(),
        ));
        assert!(matches!(map_redis_error(noauth), DriverError::AuthFailed));

        let wrongpass = RedisError::from((
            redis::ErrorKind::AuthenticationFailed,
            "authentication failed",
            "WRONGPASS invalid username-password pair".to_string(),
        ));
        assert!(matches!(map_redis_error(wrongpass), DriverError::AuthFailed));
    }

    #[test]
    fn map_redis_error_falls_back_to_a_query_error() {
        let err = RedisError::from((redis::ErrorKind::UnexpectedReturnType, "unexpected type"));
        assert!(matches!(map_redis_error(err), DriverError::Query { .. }));
    }

    #[test]
    fn redis_value_to_result_maps_nil_okay_and_scalars() {
        assert_eq!(redis_value_to_result(RedisValue::Nil).rows, vec![vec![Value::Null]]);
        assert_eq!(
            redis_value_to_result(RedisValue::Okay).rows,
            vec![vec![Value::Text("OK".into())]]
        );
        assert_eq!(
            redis_value_to_result(RedisValue::Int(42)).rows,
            vec![vec![Value::Int(42)]]
        );
        assert_eq!(
            redis_value_to_result(RedisValue::Boolean(true)).rows,
            vec![vec![Value::Bool(true)]]
        );
    }

    #[test]
    fn redis_value_to_result_decodes_valid_utf8_binary_as_text() {
        let result = redis_value_to_result(RedisValue::BulkString(b"hello".to_vec()));
        assert_eq!(result.rows, vec![vec![Value::Text("hello".into())]]);
    }

    #[test]
    fn redis_value_to_result_replaces_invalid_utf8_binary_instead_of_failing() {
        let result = redis_value_to_result(RedisValue::BulkString(vec![0xFF, 0xFE]));
        let Value::Text(text) = &result.rows[0][0] else {
            panic!("expected a text value");
        };
        assert!(text.contains('\u{FFFD}'));
    }

    #[test]
    fn redis_value_to_result_maps_an_array_to_one_row_per_item() {
        let result = redis_value_to_result(RedisValue::Array(vec![
            RedisValue::Int(1),
            RedisValue::SimpleString("two".into()),
        ]));
        assert_eq!(result.rows, vec![vec![Value::Int(1)], vec![Value::Text("two".into())]]);
        assert!(!result.truncated);
    }

    #[test]
    fn redis_value_to_result_truncates_an_array_past_the_row_cap() {
        let items = (0..MAX_QUERY_ROWS + 1).map(|i| RedisValue::Int(i as i64)).collect();
        let result = redis_value_to_result(RedisValue::Array(items));
        assert_eq!(result.rows.len(), MAX_QUERY_ROWS);
        assert!(result.truncated);
    }

    #[test]
    fn redis_value_to_result_maps_a_map_to_key_value_rows() {
        let result = redis_value_to_result(RedisValue::Map(vec![(
            RedisValue::SimpleString("field".into()),
            RedisValue::Int(7),
        )]));
        assert_eq!(result.rows, vec![vec![Value::Text("field".into()), Value::Int(7)]]);
    }

    #[test]
    fn redis_scalar_maps_nil_and_primitives() {
        assert_eq!(redis_scalar(RedisValue::Nil), Value::Null);
        assert_eq!(redis_scalar(RedisValue::Int(9)), Value::Int(9));
        assert_eq!(redis_scalar(RedisValue::Boolean(false)), Value::Bool(false));
        assert_eq!(redis_scalar(RedisValue::Okay), Value::Text("OK".into()));
    }

    #[test]
    fn redis_scalar_decodes_binary_as_lossy_text() {
        assert_eq!(
            redis_scalar(RedisValue::BulkString(b"abc".to_vec())),
            Value::Text("abc".into())
        );
        assert_eq!(
            redis_scalar(RedisValue::BulkString(vec![0xFF])),
            Value::Text("\u{FFFD}".into())
        );
    }
}
