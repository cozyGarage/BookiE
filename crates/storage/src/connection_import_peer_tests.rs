use super::*;

fn text(value: &str) -> PeerPort {
    PeerPort::Text(value.to_string())
}

#[test]
fn a_missing_or_blank_port_takes_the_engine_default() {
    assert_eq!(port(None, "postgres"), Some(5432));
    assert_eq!(port(None, "mysql"), Some(3306));
    assert_eq!(port(Some(&text("")), "postgres"), Some(5432));
    assert_eq!(port(Some(&text("   ")), "mysql"), Some(3306));
}

#[test]
fn a_port_is_read_from_a_number_or_padded_text() {
    assert_eq!(port(Some(&PeerPort::Number(5433)), "postgres"), Some(5433));
    assert_eq!(port(Some(&text(" 6543 ")), "postgres"), Some(6543));
    assert_eq!(port(Some(&PeerPort::Number(65535)), "postgres"), Some(65535));
}

#[test]
fn an_unusable_port_refuses_the_connection_instead_of_guessing() {
    assert_eq!(port(Some(&PeerPort::Number(0)), "postgres"), None);
    assert_eq!(port(Some(&PeerPort::Number(65536)), "postgres"), None);
    assert_eq!(port(Some(&text("0")), "postgres"), None);
    assert_eq!(port(Some(&text("99999")), "postgres"), None);
    assert_eq!(port(Some(&text("five")), "postgres"), None);
    assert_eq!(port(Some(&text("-1")), "postgres"), None);
}

#[test]
fn a_field_keeps_ordinary_text_and_refuses_control_characters_and_long_values() {
    assert_eq!(field("alice").as_deref(), Some("alice"));
    assert_eq!(field("").as_deref(), Some(""));
    assert_eq!(field("a\u{7}b"), None);
    assert_eq!(field("line\nbreak"), None);
    assert_eq!(
        field(&"x".repeat(MAX_FIELD_LEN)).as_deref(),
        Some("x".repeat(MAX_FIELD_LEN).as_str())
    );
    assert_eq!(field(&"x".repeat(MAX_FIELD_LEN + 1)), None);
}

#[test]
fn a_skipped_connection_is_named_by_its_name_or_else_its_fallback() {
    assert_eq!(skipped_label("  Orders  ", "key-1"), "Orders");
    assert_eq!(skipped_label("", "key-1"), "key-1");
    assert_eq!(skipped_label("   ", "key-1"), "key-1");
}

#[test]
fn a_skipped_name_is_cleaned_and_bounded_before_it_reaches_the_screen() {
    assert_eq!(skipped_label("a\u{7}b\nc", "k"), "abc");
    let long = "y".repeat(MAX_FIELD_LEN + 50);
    assert_eq!(skipped_label(&long, "k").chars().count(), MAX_FIELD_LEN);
    assert_eq!(
        skipped_label("", &"z".repeat(MAX_FIELD_LEN + 50)).chars().count(),
        MAX_FIELD_LEN
    );
}

#[test]
fn a_refusal_is_a_schema_error_carrying_the_message_only() {
    let error = refused("the file lists no connections").to_string();
    assert!(error.contains("the file lists no connections"), "{error}");
}

#[test]
fn an_import_body_carries_groups_for_the_drafts_that_have_one_and_no_secrets() {
    let grouped =
        crate::connection_url::draft_connection("postgres", "G".into(), "h".into(), 5432, String::new(), String::new());
    let plain =
        crate::connection_url::draft_connection("postgres", "P".into(), "h".into(), 5432, String::new(), String::new());
    let mut groups = BTreeMap::new();
    groups.insert(grouped.id, "Prod / EU".to_string());
    groups.insert(Uuid::new_v4(), "Orphan".to_string());
    let body = PeerImport {
        drafts: vec![grouped.clone(), plain.clone()],
        skipped: Vec::new(),
        groups,
    }
    .into_body();
    assert_eq!(body.connections.len(), 2);
    assert_eq!(
        body.organization
            .get(&grouped.id)
            .and_then(|entry| entry.group.as_deref()),
        Some("Prod / EU")
    );
    assert!(!body.organization.contains_key(&plain.id));
    assert!(body.secrets.is_empty());
}
