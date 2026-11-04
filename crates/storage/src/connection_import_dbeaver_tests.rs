use super::*;

const SENTINEL: &str = "not-a-real-secret-0b7f";

fn sources() -> String {
    format!(
        r#"{{
  "folders": {{"Prod": {{}}}},
  "connections": {{
    "pg-1": {{
      "provider": "postgresql", "driver": "postgres-jdbc", "name": "Orders PG", "folder": "Prod",
      "save-password": true,
      "configuration": {{
        "host": "DB.Example.com", "port": "5433", "database": "orders", "user": "alice",
        "url": "jdbc:postgresql://DB.Example.com:5433/orders", "password": "{SENTINEL}"
      }}
    }},
    "my-1": {{
      "provider": "mysql", "driver": "mysql8", "name": "Shop",
      "configuration": {{"host": "10.0.0.5", "database": "shop", "url": "jdbc:mysql://10.0.0.5:3306/shop"}}
    }},
    "ora-1": {{
      "provider": "oracle", "driver": "oracle_thin", "name": "Legacy Oracle",
      "configuration": {{"host": "ora.example.com", "port": 1521, "url": "jdbc:oracle:thin:@ora.example.com:1521:XE"}}
    }}
  }}
}}"#
    )
}

#[test]
fn drafts_carry_host_port_user_and_engine_and_skip_unsupported_engines_by_name() {
    let import = parse_dbeaver_data_sources(&sources()).unwrap();
    let names: Vec<_> = import.drafts.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Orders PG", "Shop"]);
    let pg = &import.drafts[0];
    assert_eq!(
        (
            pg.driver_id.as_str(),
            pg.host.as_str(),
            pg.port,
            pg.database.as_str(),
            pg.username.as_str()
        ),
        ("postgres", "db.example.com", 5433, "orders", "alice")
    );
    let mysql = &import.drafts[1];
    assert_eq!(
        (mysql.driver_id.as_str(), mysql.port, mysql.username.as_str()),
        ("mysql", 3306, "")
    );
    assert_eq!(import.skipped, ["Legacy Oracle"]);
}

#[test]
fn a_remote_draft_is_production_like_a_url_import_and_a_loopback_draft_stays_local() {
    let import = parse_dbeaver_data_sources(&sources()).unwrap();
    assert!(
        import
            .drafts
            .iter()
            .all(|draft| draft.environment == bookie_core::Environment::Prod)
    );
    assert!(
        import
            .drafts
            .iter()
            .all(|draft| draft.effective_tls_mode() == bookie_core::TlsMode::VerifyFull)
    );
    for loopback in ["localhost", "::1", "127.0.0.1"] {
        let local = sources().replace("DB.Example.com", loopback);
        let import = parse_dbeaver_data_sources(&local).unwrap();
        assert_eq!(
            import.drafts[0].environment,
            bookie_core::Environment::Local,
            "{loopback}"
        );
        assert_eq!(
            import.drafts[0].effective_tls_mode(),
            bookie_core::TlsMode::Disabled,
            "{loopback}"
        );
    }
}

#[test]
fn no_password_in_the_source_reaches_the_drafts() {
    let import = parse_dbeaver_data_sources(&sources()).unwrap();
    assert!(!format!("{import:?}").contains(SENTINEL));
    assert!(!serde_json::to_string(&import.drafts).unwrap().contains(SENTINEL));
}

#[test]
fn each_draft_gets_its_own_id() {
    let import = parse_dbeaver_data_sources(&sources()).unwrap();
    assert_ne!(import.drafts[0].id, import.drafts[1].id);
}

#[test]
fn malformed_or_wrongly_shaped_input_is_refused_without_echoing_it() {
    for bad in [
        "",
        "not json",
        "[]",
        r#"{"connections": []}"#,
        r#"{"connections": {"a": 5}}"#,
    ] {
        let error = parse_dbeaver_data_sources(bad).unwrap_err().to_string();
        assert!(!error.contains("not json"), "{error}");
    }
}

#[test]
fn a_file_with_no_connections_is_refused() {
    assert!(parse_dbeaver_data_sources(r#"{"connections": {}}"#).is_err());
}

#[test]
fn a_connection_with_a_bad_host_or_port_is_skipped_not_imported() {
    let json = r#"{"connections": {
        "a": {"provider": "postgresql", "name": "bad host", "configuration": {"host": "evil host;rm", "port": "5432"}},
        "b": {"provider": "postgresql", "name": "bad port", "configuration": {"host": "h", "port": "99999"}},
        "c": {"provider": "postgresql", "name": "ok", "configuration": {"host": "h"}}
    }}"#;
    let import = parse_dbeaver_data_sources(json).unwrap();
    assert_eq!(import.drafts.len(), 1);
    assert_eq!(import.drafts[0].port, 5432);
    assert_eq!(import.skipped.len(), 2);
}

#[test]
fn input_over_the_size_limit_is_refused() {
    let big = format!(r#"{{"connections": {{}}, "pad": "{}"}}"#, "x".repeat(MAX_DBEAVER_BYTES));
    assert!(parse_dbeaver_data_sources(&big).is_err());
}

#[test]
fn a_file_where_no_engine_is_supported_is_refused() {
    let json = r#"{"connections": {"o": {"provider": "oracle", "name": "Oracle", "configuration": {"host": "h"}}}}"#;
    assert!(parse_dbeaver_data_sources(json).is_err());
}

#[test]
fn the_import_body_holds_every_draft_and_no_secrets() {
    let import = parse_dbeaver_data_sources(&sources()).unwrap();
    let ids: Vec<_> = import.drafts.iter().map(|draft| draft.id).collect();
    let body = import.into_body();
    assert_eq!(body.connections.iter().map(|c| c.id).collect::<Vec<_>>(), ids);
    assert!(body.secrets.is_empty());
    assert!(!serde_json::to_string(&body).unwrap().contains(SENTINEL));
}
