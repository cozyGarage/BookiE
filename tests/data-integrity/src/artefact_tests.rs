use super::*;

#[test]
fn artefact_text_is_read_with_the_case_carrier_and_rule() {
    assert!(check_text(Rule::Exact, Carrier::Decimal, Some("1.2300"), Some("1.2300"), None).is_ok());
    assert!(check_text(Rule::Exact, Carrier::Decimal, Some("1.23"), Some("1.2300"), None).is_err());
    assert!(check_text(Rule::Float, Carrier::Float, Some("-0"), Some("-0"), None).is_ok());
    assert!(check_text(Rule::Bytes, Carrier::Bytes, Some("\\xdead"), Some("DEAD"), None).is_ok());
    assert!(check_text(Rule::Exact, Carrier::Text, None, None, None).is_ok());
    assert!(check_text(Rule::Exact, Carrier::Text, Some(""), None, None).is_err());
}

#[test]
fn a_spelling_the_harness_cannot_parse_must_equal_the_server_text() {
    let epoch = Some("-210863520000");
    let text = Some("4713-01-01 BC");
    assert!(check_text(Rule::Instant, Carrier::Date, Some("4713-01-01 BC"), epoch, text).is_ok());
    assert!(check_text(Rule::Instant, Carrier::Date, Some("4713-01-01 B.C."), epoch, text).is_err());
}

#[test]
fn delimited_artefacts_keep_null_and_empty_apart_with_a_marker() {
    let cells = delimited_cells("id,v\n1,\\N\n2,\"\"\n3,x\n", b',', true, Some("\\N")).unwrap();
    assert_eq!(cells[&1], None);
    assert_eq!(cells[&2], Some(String::new()));
    assert_eq!(cells[&3], Some("x".into()));
}

#[test]
fn json_artefacts_keep_number_spelling() {
    let rows: Vec<RawRow> =
        serde_json::from_str(r#"[{"id":1,"v":1.2300},{"id":2,"v":null},{"id":3,"v":"t"},{"id":4,"v":{"a":1,"a":2}}]"#)
            .unwrap();
    let cells = json_cells(&rows, false).unwrap();
    assert_eq!(cells[&1], Some("1.2300".into()));
    assert_eq!(cells[&2], None);
    assert_eq!(cells[&3], Some("t".into()));
    assert_eq!(cells[&4], Some(r#"{"a":1,"a":2}"#.into()));
}
