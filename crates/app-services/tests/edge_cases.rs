#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use tablepro_app_services::enum_input::{EnumInputError, declared_member_labels, parse_enum_input};
use tablepro_app_services::reconnect::next_delay;
use tablepro_app_services::sql_text::SqlTextLimits;
use tablepro_app_services::stream_result_spool::StreamResultSpoolManager;
use tablepro_core::{ColumnInfo, EnumInputStyle, QueryResult, Value};

fn column(name: &str, data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[tokio::test]
async fn a_spooled_page_larger_than_one_encryption_chunk_round_trips() {
    let manager = StreamResultSpoolManager::with_limit(16 * 1024 * 1024);
    let spool = manager.create().unwrap();
    let columns = vec![column("body", "text")];
    let big = "x".repeat(200_000);
    let page = Arc::new(QueryResult {
        columns: columns.clone(),
        rows: vec![vec![Value::Text(big.clone())], vec![Value::Text("é".repeat(70_000))]],
        truncated: false,
    });

    spool.append_page(0, page.clone()).await.unwrap();
    let loaded = spool.load_page(0, columns).await.unwrap();

    assert_eq!(loaded.rows, page.rows);
    assert_eq!(spool.page_count(), 1);
}

#[tokio::test]
async fn an_empty_spooled_page_round_trips() {
    let manager = StreamResultSpoolManager::with_limit(1024 * 1024);
    let spool = manager.create().unwrap();
    let columns = vec![column("a", "int")];
    let page = Arc::new(QueryResult {
        columns: columns.clone(),
        rows: Vec::new(),
        truncated: true,
    });

    spool.append_page(0, page).await.unwrap();
    let loaded = spool.load_page(0, columns).await.unwrap();

    assert!(loaded.rows.is_empty());
    assert!(loaded.truncated);
}

#[tokio::test]
async fn a_page_appended_out_of_order_is_refused_and_the_spool_stays_usable() {
    let manager = StreamResultSpoolManager::with_limit(1024 * 1024);
    let spool = manager.create().unwrap();
    let columns = vec![column("a", "int")];
    let page = Arc::new(QueryResult {
        columns: columns.clone(),
        rows: vec![vec![Value::Int(1)]],
        truncated: false,
    });

    assert!(spool.append_page(1, page.clone()).await.is_err());
    spool.append_page(0, page.clone()).await.unwrap();
    assert!(spool.append_page(0, page).await.is_err());
    assert_eq!(spool.page_count(), 1);
}

#[test]
fn enum_labels_are_read_from_a_declaration_with_surrounding_space() {
    assert_eq!(
        declared_member_labels("enum('a','b')"),
        Some(vec!["a".to_string(), "b".to_string()])
    );
    assert_eq!(
        declared_member_labels("ENUM( 'a' , 'b' )"),
        Some(vec!["a".to_string(), "b".to_string()])
    );
    assert_eq!(declared_member_labels("varchar(10)"), None);
}

#[test]
fn a_set_member_is_accepted_only_when_declared() {
    let set = column("tags", "set('a','b','c')");
    let parse = |text: &str| parse_enum_input(text, Some(&set), EnumInputStyle::DeclaredMemberList);

    assert_eq!(parse("a,c"), Some(Ok(Value::Text("a,c".into()))));
    assert_eq!(parse(""), Some(Ok(Value::Text(String::new()))));
    assert_eq!(parse("a,z"), Some(Err(EnumInputError::UndeclaredMember)));
}

#[test]
fn a_reconnect_delay_grows_and_stops_at_its_ceiling() {
    let mut delay = Duration::from_millis(500);
    let mut previous = delay;
    for _ in 0..40 {
        delay = next_delay(delay);
        assert!(delay >= previous);
        previous = delay;
    }
    assert_eq!(next_delay(delay), delay);
}

#[test]
fn line_limits_count_bytes_not_characters_and_agree_between_document_and_insertion() {
    let limits = SqlTextLimits::EDITOR;
    let wide = "é".repeat(131_073);
    assert!(limits.validate_document(&wide).is_err());
    assert!(limits.validate_insertion(0, 0, 0, 1, &wide).is_err());

    let at_limit = "a".repeat(256 * 1024);
    assert!(limits.validate_document(&at_limit).is_ok());
    assert!(limits.validate_insertion(0, 0, 0, 1, &at_limit).is_ok());
    let over = "a".repeat(256 * 1024 + 1);
    assert!(limits.validate_document(&over).is_err());
    assert!(limits.validate_insertion(0, 0, 0, 1, &over).is_err());
}

#[test]
fn a_document_over_the_size_limit_is_refused_before_its_lines_are_checked() {
    let limits = SqlTextLimits::EDITOR;
    let text = "a\n".repeat(4 * 1024 * 1024 + 1);
    assert!(limits.validate_document(&text).is_err());
}
