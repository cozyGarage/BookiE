use super::*;
use secrecy::ExposeSecret;

#[test]
fn a_postgres_url_becomes_a_saved_connection() {
    let parsed = parse_connection_url("postgres://alice@db.example.com:5433/sales").unwrap();
    assert_eq!(parsed.connection.driver_id, "postgres");
    assert_eq!(parsed.connection.host, "db.example.com");
    assert_eq!(parsed.connection.port, 5433);
    assert_eq!(parsed.connection.database, "sales");
    assert_eq!(parsed.connection.username, "alice");
    assert_eq!(parsed.connection.name, "sales on db.example.com");
    assert!(parsed.password.is_none());
}

#[test]
fn an_omitted_port_falls_back_to_the_scheme_default() {
    assert_eq!(
        parse_connection_url("mysql://root@localhost/app")
            .unwrap()
            .connection
            .port,
        3306
    );
    assert_eq!(
        parse_connection_url("postgresql://localhost").unwrap().connection.port,
        5432
    );
    assert_eq!(
        parse_connection_url("mariadb://localhost")
            .unwrap()
            .connection
            .driver_id,
        "mysql"
    );
}

#[test]
fn a_password_is_kept_out_of_the_saved_record() {
    let parsed = parse_connection_url("postgres://alice:s3cr3t@db.example.com/sales").unwrap();
    assert_eq!(parsed.password.as_ref().map(|p| p.expose_secret()), Some("s3cr3t"));
    let json = serde_json::to_string(&parsed.connection).unwrap();
    assert!(
        !json.contains("s3cr3t"),
        "the serialized record must not carry the password"
    );
}

#[test]
fn percent_escapes_are_decoded_in_credentials_and_database() {
    let parsed = parse_connection_url("postgres://a%40corp:p%2Fw%3A1@db.example.com/my%20db").unwrap();
    assert_eq!(parsed.connection.username, "a@corp");
    assert_eq!(parsed.connection.database, "my db");
    assert_eq!(parsed.password.as_ref().map(|p| p.expose_secret()), Some("p/w:1"));
}

#[test]
fn a_bracketed_ipv6_host_keeps_its_address() {
    let parsed = parse_connection_url("postgres://u@[2001:db8::1]:6000/app").unwrap();
    assert_eq!(parsed.connection.host, "2001:db8::1");
    assert_eq!(parsed.connection.port, 6000);
}

#[test]
fn a_query_string_never_becomes_the_database_name() {
    let parsed = parse_connection_url("postgres://u@db.example.com/app?sslmode=require").unwrap();
    assert_eq!(parsed.connection.database, "app");
}

#[test]
fn imported_urls_default_to_a_local_read_write_password_connection() {
    let parsed = parse_connection_url("postgres://u@db.example.com/app").unwrap();
    assert_eq!(parsed.connection.environment, Environment::Local);
    assert_eq!(parsed.connection.auth_mode, AuthMode::Password);
    assert_eq!(parsed.connection.effective_tls_mode(), TlsMode::Disabled);
    assert!(!parsed.connection.read_only);
    assert!(parsed.connection.ssh.is_none());
}

#[test]
fn hostile_and_malformed_urls_are_refused() {
    for input in [
        "",
        "   ",
        "db.example.com/app",
        "file:///etc/passwd",
        "javascript://db/app",
        "postgres://",
        "postgres:///app",
        "postgres://db.example.com:0/app",
        "postgres://db.example.com:99999/app",
        "postgres://db.example.com:abc/app",
        "postgres://db.example.com/a/b",
        "postgres://db.exa mple.com/app",
        "postgres://db.example.com/ap%2p",
        "postgres://db.example.com/ap%",
        "postgres://[2001:db8::1/app",
        "postgres://db;DROP TABLE t/app",
    ] {
        assert!(
            parse_connection_url(input).is_err(),
            "{input:?} must be refused, not imported"
        );
    }
}

#[test]
fn an_over_long_url_is_refused_before_decoding() {
    let long = format!("postgres://u@db.example.com/{}", "a".repeat(MAX_URL_LEN));
    assert!(parse_connection_url(&long).is_err());
}

#[test]
fn explicit_url_tls_modes_survive_import() {
    for (url, mode) in [
        ("postgres://u@db/app?sslmode=disable", TlsMode::Disabled),
        ("postgres://u@db/app?sslmode=prefer", TlsMode::Prefer),
        ("postgres://u@db/app?sslmode=require", TlsMode::Require),
        ("postgres://u@db/app?sslmode=verify-ca", TlsMode::VerifyCa),
        ("postgres://u@db/app?sslmode=verify-full", TlsMode::VerifyFull),
        ("postgres://u@db/app?sslmode=verify%2Dfull", TlsMode::VerifyFull),
        ("mysql://u@db/app?ssl-mode=DISABLED", TlsMode::Disabled),
        ("mysql://u@db/app?ssl-mode=PREFERRED", TlsMode::Prefer),
        ("mysql://u@db/app?ssl-mode=REQUIRED", TlsMode::Require),
        ("mysql://u@db/app?ssl-mode=VERIFY_CA", TlsMode::VerifyCa),
        ("mysql://u@db/app?ssl-mode=VERIFY_IDENTITY", TlsMode::VerifyFull),
        ("mongodb://u@db/app?tls=true", TlsMode::VerifyFull),
        ("mongodb://u@db/app?tls=false", TlsMode::Disabled),
    ] {
        let parsed = parse_connection_url(url).expect("supported TLS mode");
        assert_eq!(parsed.connection.effective_tls_mode(), mode, "{url}");
        assert_eq!(parsed.connection.use_tls, mode.encrypts(), "{url}");
    }
}

#[test]
fn unsupported_or_ambiguous_url_options_refuse_import_without_echoing_secrets() {
    for options in [
        "sslmode=allow",
        "sslmode=invalid",
        "sslmode=",
        "sslmode",
        "sslmode=require&sslmode=disable",
        "sslrootcert=/secret/root.pem",
        "sslmode=require&password=secret",
        "sslmode=require&",
        "sslmode=%ZZ",
    ] {
        let url = format!("postgres://u:secret@db/app?{options}");
        let error = parse_connection_url(&url).err().expect("refuse unsupported option");
        assert!(!error.to_string().contains("secret"));
    }
    for url in [
        "mongodb://u@db/app?authSource=admin",
        "mongodb://u@db/app?tlsAllowInvalidCertificates=true",
        "mongodb://u@db/app?tls=invalid",
        "mysql://u@db/app?ssl-mode=invalid",
        "redis://u@db/app?tls=true",
        "postgres://u@db/app?sslmode=require#sslmode=disable",
    ] {
        assert!(parse_connection_url(url).is_err(), "{url}");
    }
}

#[test]
fn a_mysql_socket_url_imports_a_local_socket_endpoint() {
    for url in [
        "mysql://app@/shop?socket=/run/mysqld/mysqld.sock",
        "mariadb://app@localhost/shop?socket=%2Frun%2Fmysqld%2Fmysqld.sock",
    ] {
        let parsed = parse_connection_url(url).unwrap();
        let connection = parsed.connection;
        assert_eq!(connection.driver_id, "mysql", "{url}");
        assert_eq!(
            connection.socket_path.as_deref(),
            Some(std::path::Path::new("/run/mysqld/mysqld.sock"))
        );
        assert_eq!(connection.socket_dir, None);
        assert_eq!(connection.host, "localhost");
        assert_eq!(connection.database, "shop");
        assert_eq!(connection.username, "app");
        assert_eq!(connection.tls_mode, Some(TlsMode::Disabled));
        assert_eq!(connection.name, "shop on /run/mysqld/mysqld.sock");
    }
}

#[test]
fn a_postgres_host_directory_imports_a_socket_directory_and_keeps_its_port() {
    let parsed = parse_connection_url("postgresql://app:secret@:6543/shop?host=/run/postgresql").unwrap();
    let connection = parsed.connection;
    assert_eq!(
        connection.socket_dir.as_deref(),
        Some(std::path::Path::new("/run/postgresql"))
    );
    assert_eq!(connection.socket_path, None);
    assert_eq!(connection.port, 6543);
    assert!(parsed.password.is_some());
    assert!(!serde_json::to_string(&connection).unwrap().contains("secret"));
}

#[test]
fn a_socket_url_must_name_an_absolute_local_path_and_no_network_host() {
    for url in [
        "mysql://app@/shop?socket=mysqld.sock",
        "mysql://app@/shop?socket=/run/../tmp/mysqld.sock",
        "mysql://app@/shop?socket=/",
        "mysql://app@/shop?socket=",
        "mysql://app@db.example.com/shop?socket=/run/mysqld/mysqld.sock",
        "mysql://app@/shop?socket=/run/mysqld/mysqld.sock&ssl-mode=REQUIRED",
        "postgres://app@/shop?socket=/run/postgresql",
        "mysql://app@/shop?host=/run/mysqld",
        "redis://app@/0?socket=/run/redis.sock",
    ] {
        assert!(parse_connection_url(url).is_err(), "{url}");
    }
}

#[test]
fn a_semicolon_or_quote_in_the_host_cannot_reach_a_saved_record() {
    assert!(parse_connection_url("postgres://u@db'--/app").is_err());
    assert!(parse_connection_url("postgres://u@db\"/app").is_err());
}
