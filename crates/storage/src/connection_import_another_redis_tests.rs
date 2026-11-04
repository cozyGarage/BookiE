use super::*;

const SENTINEL: &str = "not-a-real-secret-4c1d";

fn bundle() -> String {
    format!(
        r#"{{"connections": [
          {{"name": "cache", "host": "Redis.Example.com", "port": "6380", "username": "ops", "auth": "{SENTINEL}",
            "separator": ":", "groupId": "g1", "connectionReadOnly": true}},
          {{"name": "plain", "host": "10.0.0.5", "port": "", "auth": "{SENTINEL}"}},
          {{"name": "tls", "host": "tls.example.com", "port": 6390, "sslOptions": {{"key": "", "cert": "", "ca": "", "servername": ""}}}},
          {{"name": "via ssh", "host": "10.0.0.6", "sshOptions": {{"host": "bastion", "password": "{SENTINEL}"}}}},
          {{"name": "sentinel", "host": "10.0.0.7", "sentinelOptions": {{"masterName": "m", "nodePassword": "{SENTINEL}"}}}},
          {{"name": "cluster", "host": "10.0.0.8", "cluster": true}},
          {{"name": "bad host", "host": "evil host;rm"}},
          {{"name": "bad port", "host": "h", "port": "99999"}}
        ], "groups": [{{"id": "g1", "name": "Prod"}}]}}"#
    )
}

fn ano(json: &str) -> String {
    STANDARD.encode(json)
}

#[test]
fn drafts_carry_host_port_user_tls_and_read_only_and_skip_indirect_connections_by_name() {
    let import = parse_another_redis_connections(&ano(&bundle())).unwrap();
    let names: Vec<_> = import.drafts.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["cache", "plain", "tls"]);
    let cache = &import.drafts[0];
    assert_eq!(
        (
            cache.driver_id.as_str(),
            cache.host.as_str(),
            cache.port,
            cache.username.as_str(),
            cache.read_only
        ),
        ("redis", "redis.example.com", 6380, "ops", true)
    );
    assert_eq!(import.drafts[1].port, 6379, "an empty port takes the Redis default");
    assert!(!import.drafts[1].use_tls);
    assert!(import.drafts[2].use_tls);
    assert_eq!(import.drafts[2].tls_mode, Some(TlsMode::VerifyFull));
    assert_eq!(
        import.skipped,
        ["bad host", "bad port", "cluster", "sentinel", "via ssh"]
    );
}

#[test]
fn a_plain_json_bundle_and_a_bare_list_are_read_like_the_encoded_file() {
    let list = r#"[{"name": "a", "host": "h"}]"#;
    assert_eq!(parse_another_redis_connections(list).unwrap().drafts.len(), 1);
    assert_eq!(parse_another_redis_connections(&ano(list)).unwrap().drafts.len(), 1);
    assert_eq!(parse_another_redis_connections(&bundle()).unwrap().drafts.len(), 3);
}

#[test]
fn no_password_reaches_the_drafts_or_the_import_body() {
    let import = parse_another_redis_connections(&ano(&bundle())).unwrap();
    let body = import.into_body();
    assert!(body.secrets.is_empty());
    assert!(!serde_json::to_string(&body).unwrap().contains(SENTINEL));
}

#[test]
fn malformed_or_wrongly_shaped_input_is_refused_without_echoing_it() {
    for bad in [
        "",
        "not json at all",
        "[]",
        r#"{"connections": []}"#,
        r#"{"connections": [5]}"#,
        "!!!!",
    ] {
        let error = parse_another_redis_connections(bad).unwrap_err().to_string();
        assert!(!error.contains("not json at all"), "{error}");
    }
}

#[test]
fn a_file_where_nothing_can_be_imported_directly_is_refused() {
    let json = r#"[{"name": "x", "host": "h", "cluster": true}]"#;
    assert!(parse_another_redis_connections(json).is_err());
}

#[test]
fn input_over_the_size_limit_is_refused() {
    let big = format!(r#"[{{"name": "{}"}}]"#, "x".repeat(MAX_ANOTHER_REDIS_BYTES));
    assert!(parse_another_redis_connections(&big).is_err());
}

#[test]
fn each_draft_gets_its_own_id() {
    let import = parse_another_redis_connections(&ano(&bundle())).unwrap();
    assert_ne!(import.drafts[0].id, import.drafts[1].id);
}
