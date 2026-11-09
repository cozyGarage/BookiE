use super::*;
use bookie_core::TlsConfig;
use secrecy::SecretString;

fn options() -> ConnectOptions {
    ConnectOptions {
        host: "db.example".into(),
        port: 1521,
        database: "FREEPDB1".into(),
        username: "u".into(),
        password: SecretString::new("p".to_string().into()),
        tls: TlsConfig::disabled(),
        ..ConnectOptions::default()
    }
}

#[test]
fn plaintext_options_build_a_config_and_a_tls_request_is_refused_not_downgraded() {
    assert!(connect_config(&options()).is_ok());
    for mode in [
        TlsMode::Prefer,
        TlsMode::Require,
        TlsMode::VerifyCa,
        TlsMode::VerifyFull,
    ] {
        let mut secured = options();
        secured.tls.mode = mode;
        assert!(
            matches!(connect_config(&secured), Err(DriverError::Unsupported(_))),
            "{mode:?}"
        );
    }
}

#[test]
fn a_missing_service_name_is_refused() {
    let mut missing = options();
    missing.database = "  ".into();
    assert!(matches!(connect_config(&missing), Err(DriverError::Unsupported(_))));
}

#[test]
fn the_connect_deadline_is_applied_to_the_oracle_transport() {
    let mut options = options();
    options.connect_timeout_secs = Some(3);
    assert_eq!(
        connect_config(&options)
            .expect("valid Oracle options")
            .transport_connect_timeout(),
        Some(std::time::Duration::from_secs(3))
    );
}

#[test]
fn a_missing_or_zero_connect_deadline_uses_the_shared_transport_default() {
    for connect_timeout_secs in [None, Some(0)] {
        let mut options = options();
        options.connect_timeout_secs = connect_timeout_secs;
        assert_eq!(
            connect_config(&options)
                .expect("valid Oracle options")
                .transport_connect_timeout(),
            Some(std::time::Duration::from_secs(30))
        );
    }
}

#[test]
fn the_driver_declares_itself_experimental_on_the_oracle_port() {
    let capabilities = OracleDriver.capabilities();
    assert_eq!(capabilities.maturity, DriverMaturity::Experimental);
    assert_eq!(OracleDriver.default_port(), 1521);
    assert!(!OracleDriver.id().is_empty());
}
