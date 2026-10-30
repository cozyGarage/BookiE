use super::*;

#[test]
fn undecodable_cell_cannot_be_bound_as_null() {
    let params = [Value::Undecodable("GEOMETRY".into())];
    assert!(matches!(
        bind_mysql_params(sqlx::query(sqlx::AssertSqlSafe("SELECT ?")), &params),
        Err(DriverError::Unsupported(_))
    ));
}

#[test]
fn unexpected_io_eof_is_reported_as_disconnected() {
    let error = sqlx::Error::Io(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "server closed the connection",
    ));
    assert!(matches!(map_sqlx_error(error), DriverError::Disconnected));
}

#[test]
fn a_tls_certificate_failure_wrapped_in_io_remains_a_tls_error() {
    #[derive(Debug)]
    struct CertificateNameMismatch;

    impl std::fmt::Display for CertificateNameMismatch {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("invalid peer certificate: NotValidForName")
        }
    }

    impl std::error::Error for CertificateNameMismatch {}

    #[derive(Debug)]
    struct ClosedTlsHandshake(CertificateNameMismatch);

    impl std::fmt::Display for ClosedTlsHandshake {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("connection closed unexpectedly")
        }
    }

    impl std::error::Error for ClosedTlsHandshake {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    let error = sqlx::Error::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        ClosedTlsHandshake(CertificateNameMismatch),
    ));
    assert!(matches!(map_sqlx_error(error), DriverError::Tls(message) if message.contains("certificate")));
}

#[test]
fn connection_refusal_remains_distinct_from_disconnection() {
    let error = sqlx::Error::Io(std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        "connection refused",
    ));
    assert!(matches!(map_sqlx_error(error), DriverError::ConnectionRefused));
}

#[test]
fn structure_metadata_declarations_match_the_connection_impl() {
    let d = MysqlDriver;
    let source = include_str!("lib.rs");
    assert!(d.supports_index_metadata());
    assert!(d.supports_foreign_key_metadata());
    assert!(source.contains(&["async fn ", "fetch_indexes("].concat()));
    assert!(source.contains(&["async fn ", "fetch_foreign_keys("].concat()));
}

#[test]
fn driver_metadata() {
    let d = MysqlDriver;
    assert_eq!(d.id(), "mysql");
    assert_eq!(d.display_name(), "MySQL");
    assert_eq!(d.default_port(), 3306);
    assert_eq!(d.default_database(), "mysql");
    assert_eq!(d.default_username(), "root");
    assert!(d.supports_client_tls_auth());
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
fn quote_ident_doubles_embedded_backticks() {
    assert_eq!(quote_ident("users"), "`users`");
    assert_eq!(quote_ident("My Table"), "`My Table`");
    assert_eq!(quote_ident("evil`; DROP TABLE x; --"), "`evil``; DROP TABLE x; --`");
}

fn connect_opts_with_client_tls(mode: tablepro_core::TlsMode) -> ConnectOptions {
    ConnectOptions {
        host: "db.corp.example".into(),
        port: 3306,
        tls: tablepro_core::TlsConfig {
            mode,
            client_cert: Some(std::path::PathBuf::from("/tmp/tablepro-client.crt")),
            client_key: Some(std::path::PathBuf::from("/tmp/tablepro-client.key")),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn mysql_url_query(mysql_opts: &MySqlConnectOptions) -> Vec<(String, String)> {
    use sqlx::ConnectOptions as _;
    mysql_opts
        .to_url_lossy()
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn mysql_url_has_client_tls_material(mysql_opts: &MySqlConnectOptions) -> bool {
    mysql_url_query(mysql_opts)
        .iter()
        .any(|(key, _)| key == "ssl-cert" || key == "ssl-key")
}

#[test]
fn disabled_tls_ignores_saved_client_cert_and_key() {
    let mysql_opts = mysql_connect_options(&connect_opts_with_client_tls(tablepro_core::TlsMode::Disabled));
    let query = mysql_url_query(&mysql_opts);
    assert!(
        query
            .iter()
            .any(|(key, value)| key == "ssl-mode" && value == "DISABLED")
    );
    assert!(!mysql_url_has_client_tls_material(&mysql_opts));
}

#[test]
fn prefer_and_require_tls_still_apply_saved_client_cert_and_key() {
    for mode in [tablepro_core::TlsMode::Prefer, tablepro_core::TlsMode::Require] {
        let mysql_opts = mysql_connect_options(&connect_opts_with_client_tls(mode));
        assert!(mysql_url_has_client_tls_material(&mysql_opts));
    }
}

/// H1: a tunnelled connection must dial the forwarded local socket but
/// keep verifying TLS against the real service hostname, not 127.0.0.1
/// -- exactly the pattern PostgreSQL's driver already uses.
#[test]
fn a_tunnelled_connection_dials_the_forwarded_socket_and_verifies_the_service_hostname() {
    let directory = std::path::PathBuf::from("/tmp/tablepro-ssh-abc123");
    let opts = ConnectOptions {
        host: "127.0.0.1".into(),
        port: 54321,
        service_endpoint: Some(("db.corp.example".into(), 3306)),
        forwarded_socket_dir: Some(directory.clone()),
        ..Default::default()
    };
    let mysql_opts = mysql_connect_options(&opts);
    assert_eq!(mysql_opts.get_host(), "db.corp.example");
    assert_eq!(mysql_opts.get_port(), 3306);
    assert_eq!(mysql_opts.get_socket(), Some(&directory.join(FORWARDED_SOCKET_NAME)));
}
