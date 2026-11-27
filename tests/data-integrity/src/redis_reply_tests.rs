use super::*;

#[test]
fn redis_cli_escapes_decode_to_the_stored_bytes() {
    assert_eq!(unescape_repr(r#"a\xffb\\c\""#).unwrap(), b"a\xffb\\c\"".to_vec());
    assert_eq!(unescape_repr("\u{e9}").unwrap(), "\u{e9}".as_bytes().to_vec());
}

#[test]
fn bare_infinities_from_redis_cli_become_score_text() {
    let node = parse_cli(r#"[["f",-inf],["e",inf],["inf",1]]"#).unwrap();
    let Node::List(items) = node else { panic!("list") };
    assert_eq!(
        items[0],
        Node::List(vec![Node::Bytes(b"f".to_vec()), Node::Bytes(b"-inf".to_vec())])
    );
    assert_eq!(
        items[2],
        Node::List(vec![Node::Bytes(b"inf".to_vec()), Node::Int("1".into())])
    );
}

#[test]
fn integers_past_two_to_the_53_stay_exact() {
    assert_eq!(
        parse_cli("9007199254740993").unwrap(),
        Node::Int("9007199254740993".into())
    );
}

#[test]
fn a_resp3_map_equals_the_resp2_alternating_list() {
    let server = parse_cli(r#"{"f":"v","e":""}"#).unwrap();
    let ours = Node::List(
        ["f", "v", "e", ""]
            .iter()
            .map(|text| Node::Bytes(text.as_bytes().to_vec()))
            .collect(),
    );
    assert!(compare(&server, &ours, Shape::default()).is_empty());
}

#[test]
fn a_score_may_differ_in_spelling_only_at_score_positions() {
    let server = parse_cli(r#"[["a",1.5]]"#).unwrap();
    let ours = Node::List(vec![Node::Bytes(b"a".to_vec()), Node::Bytes(b"1.5".to_vec())]);
    let scores = Shape {
        scores: Scores::WithScores,
        unordered: false,
    };
    assert!(compare(&server, &ours, scores).is_empty());
    assert!(!compare(&server, &ours, Shape::default()).is_empty());
    let member_as_number = parse_cli(r#"[[1.5,"x"]]"#).unwrap();
    let ours = Node::List(vec![Node::Bytes(b"1.5".to_vec()), Node::Bytes(b"x".to_vec())]);
    assert!(!compare(&member_as_number, &ours, scores).is_empty());
}

#[test]
fn unordered_replies_compare_as_multisets() {
    let server = parse_cli(r#"["a","b"]"#).unwrap();
    let ours = Node::List(vec![Node::Bytes(b"b".to_vec()), Node::Bytes(b"a".to_vec())]);
    let unordered = Shape {
        scores: Scores::None,
        unordered: true,
    };
    assert!(compare(&server, &ours, unordered).is_empty());
    assert!(!compare(&server, &ours, Shape::default()).is_empty());
}

#[test]
fn nested_structure_must_match_not_just_the_flattened_elements() {
    let server = parse_cli(r#"[["1-1",["f","v"]]]"#).unwrap();
    let flat = Node::List(
        ["1-1", "f", "v"]
            .iter()
            .map(|text| Node::Bytes(text.as_bytes().to_vec()))
            .collect(),
    );
    assert!(!compare(&server, &flat, Shape::default()).is_empty());
}
