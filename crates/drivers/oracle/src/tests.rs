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
fn the_driver_declares_itself_experimental_on_the_oracle_port() {
    let capabilities = OracleDriver.capabilities();
    assert_eq!(capabilities.maturity, DriverMaturity::Experimental);
    assert_eq!(OracleDriver.default_port(), 1521);
    assert!(!OracleDriver.id().is_empty());
}
