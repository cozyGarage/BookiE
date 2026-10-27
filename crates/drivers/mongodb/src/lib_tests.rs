use super::*;

#[tokio::test]
async fn sampling_infers_types_one_document_at_a_time_and_marks_a_late_difference() {
    let docs = (0..200).map(|index| {
        let value = if index == 199 {
            mongodb::bson::Bson::String("late".into())
        } else {
            mongodb::bson::Bson::Int32(index)
        };
        Ok(doc! { "n": value, "name": "x" })
    });
    let types = observe_sample(futures::stream::iter(docs)).await.unwrap();
    assert_eq!(types.get("n").map(String::as_str), Some("mixed"));
    assert_eq!(types.get("name").map(String::as_str), Some("string"));
}

#[tokio::test]
async fn sampling_stops_reading_after_an_error() {
    let pulled = std::sync::atomic::AtomicUsize::new(0);
    let stream = futures::stream::unfold(0usize, |index| {
        let pulled = &pulled;
        async move {
            pulled.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let item = if index == 2 {
                Err(mongodb::error::Error::custom("boom"))
            } else {
                Ok(doc! { "n": 1 })
            };
            Some((item, index + 1))
        }
    });
    assert!(observe_sample(Box::pin(stream)).await.is_err());
    assert_eq!(pulled.load(std::sync::atomic::Ordering::SeqCst), 3);
}

#[tokio::test]
async fn open_session_is_refused() {
    let client_opts = ClientOptions::parse("mongodb://127.0.0.1:27017").await.unwrap();
    let client = Client::with_options(client_opts).unwrap();
    let conn = MongodbConnection {
        client,
        database_name: "test".into(),
    };

    match conn.open_session().await {
        Err(DriverError::Unsupported(_)) => {}
        Ok(_) => panic!("mongodb must refuse open_session"),
        Err(other) => panic!("expected Unsupported, got {other:?}"),
    }
}

#[tokio::test]
async fn a_timed_out_read_reports_an_unknown_outcome() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { while listener.accept().await.is_ok() {} });
    let opts = ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        database: "test".into(),
        ..Default::default()
    };
    let client_opts = build_client_options(&opts).await.unwrap();
    let client = Client::with_options(client_opts).unwrap();
    let conn = MongodbConnection {
        client,
        database_name: "test".into(),
    };
    let control = tablepro_core::OperationControl::with_timeout(std::time::Duration::from_millis(50));

    let result = conn.list_tables_controlled(&control).await;

    match result {
        Err(DriverError::OperationOutcomeUnknown { .. }) => {}
        other => panic!("expected OperationOutcomeUnknown, got {other:?}"),
    }
}

#[tokio::test]
async fn value_contract_mongodb_cancelled_inflight_read_is_unknown_not_disconnected() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let opts = ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        database: "test".into(),
        ..Default::default()
    };
    let client_opts = build_client_options(&opts).await.unwrap();
    let client = Client::with_options(client_opts).unwrap();
    let conn = MongodbConnection {
        client,
        database_name: "test".into(),
    };
    let token = tokio_util::sync::CancellationToken::new();
    let control = tablepro_core::OperationControl::new(token.clone(), None);
    let operation = tokio::spawn(async move { conn.list_tables_controlled(&control).await });

    // Hold the accepted socket open so this is an in-flight operation,
    // rather than a connect failure or a server disconnect.
    let (_stream, _) = listener.accept().await.unwrap();
    token.cancel();

    match operation.await.unwrap() {
        Err(DriverError::OperationOutcomeUnknown { source }) => {
            assert!(matches!(*source, DriverError::Cancelled), "{source:?}");
        }
        other => panic!("expected unknown cancelled outcome, not disconnection: {other:?}"),
    }
}

#[test]
fn structure_metadata_is_not_declared_without_a_fetch() {
    let d = MongodbDriver;
    let source = include_str!("lib.rs");
    assert!(!d.supports_index_metadata());
    assert!(!d.supports_foreign_key_metadata());
    assert!(!source.contains(&["async fn ", "fetch_indexes("].concat()));
    assert!(!source.contains(&["async fn ", "fetch_foreign_keys("].concat()));
}

#[test]
fn driver_metadata() {
    let d = MongodbDriver;
    assert_eq!(d.id(), "mongodb");
    assert_eq!(d.display_name(), "MongoDB");
    assert_eq!(d.default_port(), 27017);
    assert_eq!(d.default_database(), "test");
    assert_eq!(d.default_username(), "");
}

#[test]
fn mongodb_host_rejects_uri_delimiters_and_brackets_ipv6() {
    for host in [
        "",
        "db/name",
        "db?x",
        "db#x",
        "user@db",
        "db%2fadmin",
        "db,other",
        "db\\name",
        "db name",
        "[bad-ip]",
    ] {
        assert!(uri_host(host).is_err(), "host must be refused: {host:?}");
    }
    assert_eq!(uri_host("db.internal").unwrap(), "db.internal");
    assert_eq!(uri_host("127.0.0.1").unwrap(), "127.0.0.1");
    assert_eq!(uri_host("::1").unwrap(), "[::1]");
    assert_eq!(uri_host("[::1]").unwrap(), "[::1]");
}

#[tokio::test]
async fn reserved_credentials_remain_encoded_and_keep_the_database_auth_source() {
    let opts = ConnectOptions {
        host: "127.0.0.1".into(),
        port: 27017,
        username: "user@name".into(),
        password: secrecy::SecretString::new("p:/?#@ss".into()),
        database: "appdb".into(),
        ..Default::default()
    };
    let client_opts = build_client_options(&opts).await.unwrap();
    let credential = client_opts.credential.unwrap();
    assert_eq!(credential.username.as_deref(), Some("user@name"));
    assert_eq!(credential.password.as_deref(), Some("p:/?#@ss"));
    assert_eq!(credential.source.as_deref(), Some("appdb"));
}

#[test]
fn decimal128_edit_parser_preserves_native_precision_and_refuses_rounding() {
    for value in [
        "1234567890123456789012345678901234",
        "0.1234567890123456789012345678901234",
        "1.2300",
        "NaN",
        "Infinity",
        "-Infinity",
    ] {
        assert_eq!(canonical_decimal128_text(value).unwrap(), value, "{value}");
    }
    assert!(canonical_decimal128_text("12345678901234567890123456789012345").is_err());
    assert!(canonical_decimal128_text("1.2.3").is_err());
}

#[test]
fn patched_mongodb_caps_scram_iterations() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../vendor/mongodb/src/client/auth/scram.rs"
    ));
    assert!(source.contains("const MAX_ITERATION_COUNT: u32 = 100_000"));
    assert!(source.contains("self.i > MAX_ITERATION_COUNT"));
    assert!(source.contains("self.nonce.starts_with(nonce)"));
}

#[tokio::test]
async fn a_single_host_connection_always_requests_a_direct_connection() {
    let opts = ConnectOptions {
        host: "db.internal".into(),
        port: 27017,
        ..Default::default()
    };
    let client_opts = build_client_options(&opts).await.expect("build client options");
    assert_eq!(client_opts.direct_connection, Some(true));
}

#[tokio::test]
async fn the_auth_source_defaults_to_the_database_and_can_be_overridden() {
    let mut opts = ConnectOptions {
        host: "db.internal".into(),
        port: 27017,
        database: "app".into(),
        username: "root".into(),
        ..Default::default()
    };
    let source = |options: mongodb::options::ClientOptions| options.credential.and_then(|c| c.source);
    assert_eq!(
        source(build_client_options(&opts).await.unwrap()).as_deref(),
        Some("app")
    );

    opts.auth_source = Some("ad min/&".into());
    let overridden = source(build_client_options(&opts).await.unwrap());
    assert_eq!(overridden.as_deref(), Some("ad min/&"));
    opts.database.clear();
    assert!(build_client_options(&opts).await.is_ok());
}

#[tokio::test]
async fn a_tunneled_connection_verifies_tls_against_the_service_hostname() {
    let opts = ConnectOptions {
        host: "127.0.0.1".into(),
        port: 54321,
        service_endpoint: Some(("mongo.internal.example".into(), 27017)),
        tls: tablepro_core::TlsConfig {
            mode: tablepro_core::TlsMode::VerifyFull,
            ..Default::default()
        },
        ..Default::default()
    };
    let client_opts = build_client_options(&opts).await.expect("build client options");
    let Some(Tls::Enabled(tls_options)) = client_opts.tls else {
        panic!("TLS must be enabled when verifying");
    };
    assert_eq!(tls_options.verify_hostname.as_deref(), Some("mongo.internal.example"));
}

const PASSWORD_CANARY: &str = "canary-p@ss/w0rd#1";

fn hostile_options(host: &str, port: u16, database: &str) -> ConnectOptions {
    ConnectOptions {
        host: host.into(),
        port,
        database: database.into(),
        username: "app".into(),
        password: secrecy::SecretString::from(PASSWORD_CANARY),
        ..Default::default()
    }
}

#[tokio::test]
async fn connection_option_errors_never_carry_the_password() {
    let cases = [
        ("", 27017, "test"),
        ("bad host", 27017, "test"),
        ("exa@mple", 27017, "test"),
        ("127.0.0.1", 0, "test"),
        ("127.0.0.1", 27017, "a/b?x=1#y"),
        ("127.0.0.1", 27017, "\u{0}"),
    ];
    let mut failures = 0;
    for (host, port, database) in cases {
        let opts = hostile_options(host, port, database);
        if let Err(error) = build_client_options(&opts).await {
            failures += 1;
            let text = format!("{error} {error:?}");
            assert!(!text.contains(PASSWORD_CANARY), "{host:?}:{port}/{database:?}: {text}");
            assert!(!text.contains("p%40ss"), "{host:?}:{port}/{database:?}: {text}");
        }
    }
    assert!(failures >= 5, "the hostile cases must actually fail, got {failures}");
}
