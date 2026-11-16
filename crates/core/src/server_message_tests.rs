use super::{
    MAX_SERVER_MESSAGE_TEXT_BYTES, MAX_SERVER_MESSAGES, MAX_SERVER_MESSAGES_TOTAL_BYTES, MessageSeverity,
    ServerMessage, ServerMessages,
};

fn notice(text: &str) -> ServerMessage {
    ServerMessage::new(MessageSeverity::Notice, Some("00000".into()), text)
}

#[test]
fn messages_keep_their_order_severity_and_code() {
    let mut messages = ServerMessages::default();
    messages.push(notice("first"));
    messages.push(ServerMessage::new(MessageSeverity::Warning, None, "second"));
    let items: Vec<_> = messages.iter().collect();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].text, "first");
    assert_eq!(items[0].code.as_deref(), Some("00000"));
    assert_eq!(items[1].severity, MessageSeverity::Warning);
    assert_eq!(items[1].code, None);
    assert_eq!(messages.omitted(), 0);
}

#[test]
fn the_message_count_is_capped_and_the_rest_is_counted() {
    let mut messages = ServerMessages::default();
    for index in 0..MAX_SERVER_MESSAGES + 25 {
        messages.push(notice(&format!("n{index}")));
    }
    assert_eq!(messages.len(), MAX_SERVER_MESSAGES);
    assert_eq!(messages.omitted(), 25);
    assert_eq!(messages.iter().last().map(|message| message.text.as_str()), Some("n99"));
}

#[test]
fn the_total_bytes_are_capped_and_the_rest_is_counted() {
    let mut messages = ServerMessages::default();
    let text = "x".repeat(MAX_SERVER_MESSAGE_TEXT_BYTES);
    let fits = MAX_SERVER_MESSAGES_TOTAL_BYTES / (MAX_SERVER_MESSAGE_TEXT_BYTES + "00000".len());
    for _ in 0..fits + 3 {
        messages.push(notice(&text));
    }
    let kept: usize = messages
        .iter()
        .map(|message| message.text.len() + message.code.as_ref().map_or(0, String::len))
        .sum();
    assert!(kept <= MAX_SERVER_MESSAGES_TOTAL_BYTES);
    assert_eq!(messages.len(), fits);
    assert_eq!(messages.omitted(), 3);
}

#[test]
fn long_text_is_cut_to_the_cap_on_a_character_boundary() {
    let text = "é".repeat(MAX_SERVER_MESSAGE_TEXT_BYTES);
    let message = ServerMessage::new(MessageSeverity::Info, None, &text);
    assert!(message.text.len() <= MAX_SERVER_MESSAGE_TEXT_BYTES);
    assert!(message.text.is_char_boundary(message.text.len()));
    assert!(message.text.starts_with('é'));
    assert!(message.text.ends_with('…'));
}

#[test]
fn short_text_is_kept_whole() {
    let message = ServerMessage::new(MessageSeverity::Info, None, "héllo");
    assert_eq!(message.text, "héllo");
}

#[test]
fn standard_sqlstate_is_kept_and_oversized_code_is_discarded() {
    let standard = ServerMessage::new(MessageSeverity::Notice, Some("01000".into()), "notice");
    let oversized = ServerMessage::new(MessageSeverity::Notice, Some("x".repeat(33)), "notice");
    assert_eq!(standard.code.as_deref(), Some("01000"));
    assert_eq!(oversized.code, None);

    let mut messages = ServerMessages::default();
    messages.push(ServerMessage {
        severity: MessageSeverity::Notice,
        code: Some("x".repeat(33)),
        text: "notice".into(),
    });
    assert_eq!(messages.iter().next().and_then(|message| message.code.as_deref()), None);
}

#[test]
fn total_byte_budget_counts_code_bytes() {
    let mut messages = ServerMessages::default();
    for _ in 0..17 {
        messages.push(ServerMessage::new(
            MessageSeverity::Notice,
            Some("x".repeat(32)),
            &"t".repeat(4070),
        ));
    }
    let retained_bytes: usize = messages
        .iter()
        .map(|message| message.text.len() + message.code.as_ref().map_or(0, String::len))
        .sum();
    assert!(retained_bytes <= MAX_SERVER_MESSAGES_TOTAL_BYTES);
    assert_eq!(messages.len(), 15);
    assert_eq!(messages.omitted(), 2);
}

#[test]
fn an_empty_list_reports_empty_and_a_flood_only_list_is_not_empty() {
    let mut messages = ServerMessages::default();
    assert!(messages.is_empty());
    messages.record_omitted(4);
    assert!(!messages.is_empty());
    assert_eq!(messages.omitted(), 4);
}

#[test]
fn extending_applies_the_same_caps() {
    let mut first = ServerMessages::default();
    for index in 0..MAX_SERVER_MESSAGES {
        first.push(notice(&format!("a{index}")));
    }
    let mut second = ServerMessages::default();
    second.push(notice("late"));
    second.record_omitted(2);
    first.extend(second);
    assert_eq!(first.len(), MAX_SERVER_MESSAGES);
    assert_eq!(first.omitted(), 3);
}
