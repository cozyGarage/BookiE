use super::*;
use std::sync::{Arc, Mutex};

#[test]
fn undecodable_cell_cannot_be_bound_as_null() {
    let params = [Value::Undecodable("NUMERIC".into())];
    assert!(matches!(
        bind_pg_params(sqlx::query(sqlx::AssertSqlSafe("SELECT $1")), &params, &[]),
        Err(DriverError::Unsupported(_))
    ));
}

#[test]
fn server_termination_states_are_disconnections_but_query_cancel_is_not() {
    for sqlstate in ["57P01", "57P02", "57P03", "57P04"] {
        assert!(is_server_disconnect_sqlstate(Some(sqlstate)), "{sqlstate}");
    }
    assert!(!is_server_disconnect_sqlstate(Some("57014")));
    assert!(!is_server_disconnect_sqlstate(Some("23505")));
    assert!(!is_server_disconnect_sqlstate(None));
}

#[test]
fn unexpected_io_eof_is_disconnected_and_connection_refusal_stays_distinct() {
    let eof = sqlx::Error::Io(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "server closed the connection",
    ));
    assert!(matches!(map_sqlx_error(eof), DriverError::Disconnected));

    let refused = sqlx::Error::Io(std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        "connection refused",
    ));
    assert!(matches!(map_sqlx_error(refused), DriverError::ConnectionRefused));
}

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl CapturedLogs {
    fn text(&self) -> String {
        match self.0.lock() {
            Ok(buffer) => String::from_utf8_lossy(&buffer).into_owned(),
            Err(poisoned) => String::from_utf8_lossy(&poisoned.into_inner()).into_owned(),
        }
    }
}

impl std::io::Write for CapturedLogs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self.0.lock() {
            Ok(mut buffer) => {
                buffer.extend_from_slice(buf);
                Ok(buf.len())
            }
            Err(_) => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLogs {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn an_undecodable_column_is_reported_to_the_logs_with_its_index_and_type() {
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.clone())
        .with_ansi(false)
        .finish();
    let value = tracing::subscriber::with_default(subscriber, || undecodable(3, "MYDOMAIN"));
    assert!(matches!(&value, Value::Undecodable(name) if name == "MYDOMAIN"));
    let text = logs.text();
    assert!(text.contains("WARN"), "{text}");
    assert!(text.contains("column_index=4"), "{text}");
    assert!(text.contains("type_name=\"MYDOMAIN\""), "{text}");
}

#[test]
fn structure_metadata_declarations_match_the_connection_impl() {
    let d = PgDriver;
    let source = include_str!("lib.rs");
    assert!(d.supports_index_metadata());
    assert!(d.supports_foreign_key_metadata());
    assert!(d.supports_view_metadata());
    assert!(source.contains(&["async fn ", "fetch_indexes("].concat()));
    assert!(source.contains(&["async fn ", "fetch_foreign_keys("].concat()));
    assert!(source.contains(&["async fn ", "list_views("].concat()));
}

#[test]
fn map_certificate_failure_returns_tls_error() {
    let err = sqlx::Error::Io(std::io::Error::other(
        "invalid peer certificate: certificate not valid for name \"127.0.0.1\"",
    ));
    let mapped = map_sqlx_error(err);
    assert!(matches!(mapped, DriverError::Tls(detail) if detail.contains("certificate")));
}

#[test]
fn map_plain_io_failure_is_disconnected() {
    let err = sqlx::Error::Io(std::io::Error::other("connection reset by peer"));
    assert!(matches!(map_sqlx_error(err), DriverError::Disconnected));
}

#[test]
fn map_io_refused_returns_connection_refused() {
    let err = sqlx::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
    assert!(matches!(map_sqlx_error(err), DriverError::ConnectionRefused));
}

#[test]
fn a_pool_startup_timeout_is_not_mapped_as_an_established_disconnect() {
    assert!(matches!(
        map_sqlx_connect_error(sqlx::Error::PoolTimedOut),
        DriverError::ConnectionRefused
    ));
    assert!(matches!(
        map_sqlx_error(sqlx::Error::PoolTimedOut),
        DriverError::Disconnected
    ));
    assert!(matches!(
        map_sqlx_connect_error(sqlx::Error::PoolClosed),
        DriverError::Disconnected
    ));
}

#[test]
fn patched_sqlx_postgres_caps_scram_iterations() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../vendor/sqlx-postgres/src/connection/sasl.rs"
    ));
    assert!(source.contains("const MAX_SASL_ITERATIONS: u32 = 100_000"));
    assert!(source.contains("iter_count > MAX_SASL_ITERATIONS"));
}

#[test]
fn driver_metadata() {
    let d = PgDriver;
    assert_eq!(d.id(), "postgres");
    assert_eq!(d.default_port(), 5432);
    assert_eq!(d.default_database(), "postgres");
    assert_eq!(d.default_username(), "postgres");
}

#[test]
fn quote_ident_doubles_embedded_quotes() {
    assert_eq!(quote_ident("users"), "\"users\"");
    assert_eq!(quote_ident("My Table"), "\"My Table\"");
    assert_eq!(
        quote_ident("evil\"; DROP TABLE x; --"),
        "\"evil\"\"; DROP TABLE x; --\""
    );
}

#[test]
fn normalize_pg_default_strips_only_the_cast_on_a_plain_literal() {
    assert_eq!(normalize_pg_default("'hi'::text".into()), "'hi'");
    assert_eq!(normalize_pg_default("''::text".into()), "''");
    assert_eq!(normalize_pg_default("NULL::character varying".into()), "NULL");
    assert_eq!(normalize_pg_default("42::integer".into()), "42");
    assert_eq!(normalize_pg_default("'-1'::integer".into()), "'-1'");
    assert_eq!(normalize_pg_default("'2024-01-01'::date".into()), "'2024-01-01'");
    assert_eq!(
        normalize_pg_default("'2024-01-01 12:00:00'::timestamp without time zone".into()),
        "'2024-01-01 12:00:00'"
    );
    assert_eq!(normalize_pg_default("'it''s'::text".into()), "'it''s'");
    assert_eq!(normalize_pg_default("'{}'::text[]".into()), "'{}'");
}

#[test]
fn normalize_pg_default_keeps_an_expression_whole() {
    assert_eq!(normalize_pg_default("now()".into()), "now()");
    assert_eq!(normalize_pg_default("CURRENT_TIMESTAMP".into()), "CURRENT_TIMESTAMP");
    assert_eq!(normalize_pg_default("gen_random_uuid()".into()), "gen_random_uuid()");
    assert_eq!(normalize_pg_default("lower('X'::text)".into()), "lower('X'::text)");
    assert_eq!(
        normalize_pg_default("(a::int + b)::numeric".into()),
        "(a::int + b)::numeric"
    );
    assert_eq!(
        normalize_pg_default("'a'::text || 'b'::text".into()),
        "'a'::text || 'b'::text"
    );
    assert_eq!(normalize_pg_default("'unbalanced".into()), "'unbalanced");
}
