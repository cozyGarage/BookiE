use bookie_core::{MAX_SERVER_MESSAGES, MessageSeverity, ServerMessage, ServerMessages};

use super::{MessagesView, status_with_messages};

fn messages(items: &[(MessageSeverity, Option<&str>, &str)]) -> ServerMessages {
    let mut messages = ServerMessages::default();
    for (severity, code, text) in items {
        messages.push(ServerMessage::new(*severity, code.map(str::to_string), text));
    }
    messages
}

#[test]
fn an_empty_list_has_no_view_and_no_status_suffix() {
    let view = MessagesView::new(&ServerMessages::default());
    assert!(view.is_empty());
    assert_eq!(view.status_suffix(), None);
    assert_eq!(status_with_messages("done in 5 ms", &view), "done in 5 ms");
}

#[test]
fn rows_keep_severity_code_and_text_in_order() {
    let view = MessagesView::new(&messages(&[
        (MessageSeverity::Notice, Some("00000"), "first"),
        (MessageSeverity::Warning, None, "second"),
    ]));
    let rows = view.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].severity, MessageSeverity::Notice);
    assert_eq!(rows[0].code.as_deref(), Some("00000"));
    assert_eq!(rows[0].text, "first");
    assert_eq!(rows[1].code, None);
    assert_eq!(view.total(), 2);
}

#[test]
fn the_status_line_shows_the_count() {
    let view = MessagesView::new(&messages(&[
        (MessageSeverity::Info, None, "a"),
        (MessageSeverity::Info, None, "b"),
    ]));
    let status = status_with_messages("done in 5 ms", &view);
    assert!(status.starts_with("done in 5 ms"));
    assert!(status.contains('2'), "{status}");
}

#[test]
fn a_flood_shows_a_marker_with_the_hidden_count_and_the_true_total() {
    let mut flood = ServerMessages::default();
    for index in 0..MAX_SERVER_MESSAGES + 7 {
        flood.push(ServerMessage::new(MessageSeverity::Notice, None, &format!("n{index}")));
    }
    let view = MessagesView::new(&flood);
    assert_eq!(view.rows().len(), MAX_SERVER_MESSAGES);
    assert_eq!(view.total(), (MAX_SERVER_MESSAGES + 7) as u64);
    let notice = view.omitted_notice().expect("a marker");
    assert!(notice.contains('7'), "{notice}");
    assert!(view.copy_text().ends_with(&notice));
}

#[test]
fn copy_text_has_one_line_per_message_with_code_and_text() {
    let view = MessagesView::new(&messages(&[
        (MessageSeverity::Warning, Some("1292"), "Truncated"),
        (MessageSeverity::Info, None, "printed"),
    ]));
    let copied = view.copy_text();
    let lines: Vec<&str> = copied.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("1292") && lines[0].contains("Truncated"), "{copied}");
    assert!(lines[1].contains("printed"), "{copied}");
    assert!(view.omitted_notice().is_none());
}

#[test]
fn a_list_with_only_an_omitted_count_is_not_empty() {
    let mut only_omitted = ServerMessages::default();
    only_omitted.record_omitted(3);
    let view = MessagesView::new(&only_omitted);
    assert!(!view.is_empty());
    assert_eq!(view.total(), 3);
}
