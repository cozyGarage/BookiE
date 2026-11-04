use super::*;

const SENTINEL: &str = "not-a-real-secret-5d90";

fn export() -> String {
    format!(
        r#"[
  {{"auth": "{SENTINEL}", "host": "Redis.Example.com", "keys_pattern": "*", "name": "cache", "namespace_separator": ":",
    "port": 6380, "ssh_port": 22, "timeout_connect": 60000, "timeout_execute": 60000, "username": "ops", "ssl": true}},
  {{"host": "10.0.0.5", "name": "plain", "namespace_separator": ":", "port": 6379}},
  {{"host": "10.0.0.6", "name": "via ssh", "namespace_separator": ":", "ssh_host": "bastion", "ssh_user": "u",
    "ssh_password": "{SENTINEL}", "ssh_private_key_path": "/k"}},
  {{"host": "evil host;rm", "name": "bad host", "namespace_separator": ":"}},
  {{"host": "h", "name": "bad port", "port": 99999, "namespace_separator": ":"}},
  {{"type": "group", "name": "folder", "connections": []}}
]"#
    )
}

#[test]
fn connections_carry_host_port_user_and_tls() {
    let import = parse_resp_connections(&export()).unwrap();
    let names: Vec<_> = import.drafts.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["cache", "plain"]);
    let cache = &import.drafts[0];
    assert_eq!(
        (cache.driver_id.as_str(), cache.port, cache.username.as_str()),
        ("redis", 6380, "ops")
    );
    assert_eq!(cache.host, "redis.example.com");
    assert_eq!(cache.tls_mode, Some(TlsMode::VerifyFull));
    let plain = &import.drafts[1];
    assert_eq!((plain.port, plain.use_tls), (6379, false));
}

#[test]
fn ssh_bad_host_bad_port_and_group_entries_are_skipped_by_name() {
    let import = parse_resp_connections(&export()).unwrap();
    assert_eq!(import.skipped, ["bad host", "bad port", "folder", "via ssh"]);
}

#[test]
fn no_password_reaches_the_drafts_or_the_import_body() {
    let import = parse_resp_connections(&export()).unwrap();
    assert!(!format!("{:?}", import.drafts).contains(SENTINEL));
    assert!(!format!("{:?}", import.into_body()).contains(SENTINEL));
}

#[test]
fn only_files_with_resp_keys_are_recognised() {
    assert!(looks_like_resp_export(&export()));
    assert!(!looks_like_resp_export(r#"[{"name": "r", "host": "h"}]"#));
    assert!(!looks_like_resp_export(r#"{"connections": []}"#));
}

#[test]
fn malformed_empty_or_oversized_input_is_refused_without_echoing_it() {
    for bad in ["", "not json at all", "[]", r#"{"a": 1}"#] {
        let error = parse_resp_connections(bad).unwrap_err().to_string();
        assert!(!error.contains("not json at all"), "{error}");
    }
    let big = format!("[{}]", " ".repeat(MAX_RESP_BYTES));
    assert!(parse_resp_connections(&big).is_err());
}

#[test]
fn a_file_with_nothing_importable_is_refused() {
    let only_ssh = r#"[{"host": "h", "name": "n", "namespace_separator": ":", "ssh_host": "b"}]"#;
    assert!(parse_resp_connections(only_ssh).is_err());
}
