use std::time::Duration;

pub const MAX_MESSAGES: usize = 200;
pub const MAX_STATEMENT_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub kind: MessageKind,
    pub text: String,
    pub id: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MessageLog {
    entries: Vec<Message>,
    next_id: u64,
}

impl MessageLog {
    pub fn push(&mut self, kind: MessageKind, text: impl Into<String>) {
        if self.entries.len() == MAX_MESSAGES {
            self.entries.remove(0);
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.entries.push(Message {
            kind,
            text: text.into(),
            id,
        });
    }

    pub fn server_messages(&mut self, messages: &bookie_core::ServerMessages) {
        let view = bookie_app_services::server_messages_view::MessagesView::new(messages);
        for row in view.rows() {
            let code = row.code.as_ref().map(|code| format!(" [{code}]")).unwrap_or_default();
            let kind = if row.severity == bookie_core::MessageSeverity::Error {
                MessageKind::Error
            } else {
                MessageKind::Info
            };
            self.push(kind, format!("{}{code}: {}", row.severity_label, row.text));
        }
        if let Some(omitted) = view.omitted_notice() {
            self.push(MessageKind::Info, omitted);
        }
    }

    pub fn entries(&self) -> &[Message] {
        &self.entries
    }

    pub fn statement_ok(&mut self, sql: &str, rows: u64, truncated: bool, elapsed: Duration) {
        let more = if truncated { " (result may be incomplete)" } else { "" };
        self.push(
            MessageKind::Success,
            format!(
                "{} · {rows} rows{more} · {} ms",
                one_line_sql(sql, MAX_STATEMENT_CHARS),
                elapsed.as_millis()
            ),
        );
    }

    pub fn statement_cancelled(&mut self, sql: &str, elapsed: Duration) {
        self.push(
            MessageKind::Info,
            format!(
                "{} · Cancelled · {} ms",
                one_line_sql(sql, MAX_STATEMENT_CHARS),
                elapsed.as_millis()
            ),
        );
    }

    pub fn statement_failed(&mut self, sql: &str, reason: &str, elapsed: Duration) {
        self.push(
            MessageKind::Error,
            format!(
                "{} · {reason} · {} ms",
                one_line_sql(sql, MAX_STATEMENT_CHARS),
                elapsed.as_millis()
            ),
        );
    }
}

pub fn format_elapsed(elapsed: Duration) -> String {
    if elapsed < Duration::from_secs(1) {
        format!("{} ms", elapsed.as_millis())
    } else {
        format!("{:.2} s", elapsed.as_secs_f64())
    }
}

pub fn noun<'a, N: PartialEq + From<u8>>(count: N, one: &'a str, many: &'a str) -> &'a str {
    if count == N::from(1) { one } else { many }
}

pub fn plural<N: Copy + std::fmt::Display + PartialEq + From<u8>>(count: N, one: &str, many: &str) -> String {
    format!("{count} {}", noun(count, one, many))
}

pub fn one_line_sql(sql: &str, max_chars: usize) -> String {
    let flat = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", &flat[..end]),
        None => flat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_takes_the_singular_only_for_one() {
        assert_eq!(plural(1_usize, "row", "rows"), "1 row");
        assert_eq!(plural(0_usize, "row", "rows"), "0 rows");
        assert_eq!(plural(2_u64, "change", "changes"), "2 changes");
        assert_eq!(noun(1_u64, "table", "tables"), "table");
    }

    #[test]
    fn one_line_sql_flattens_whitespace_and_cuts_at_the_given_length() {
        assert_eq!(one_line_sql("SELECT  1\n FROM t", 40), "SELECT 1 FROM t");
        assert_eq!(one_line_sql("abcdef", 3), "abc…");
        assert_eq!(one_line_sql("abc", 3), "abc");
    }

    #[test]
    fn server_messages_keep_severity_code_and_omission_notice() {
        let mut messages = bookie_core::ServerMessages::default();
        messages.push(bookie_core::ServerMessage::new(
            bookie_core::MessageSeverity::Warning,
            Some("01000".into()),
            "careful",
        ));
        messages.push(bookie_core::ServerMessage::new(
            bookie_core::MessageSeverity::Error,
            None,
            "failed",
        ));
        messages.record_omitted(2);
        let mut log = MessageLog::default();
        log.server_messages(&messages);
        assert_eq!(log.entries()[0].text, "Warning [01000]: careful");
        assert_eq!(log.entries()[1].kind, MessageKind::Error);
        assert!(log.entries()[2].text.contains('2'));
        let before = log.entries().len();
        log.server_messages(&bookie_core::ServerMessages::default());
        assert_eq!(log.entries().len(), before);
    }

    #[test]
    fn a_duration_reads_in_milliseconds_below_a_second_and_seconds_above() {
        assert_eq!(format_elapsed(Duration::from_millis(0)), "0 ms");
        assert_eq!(format_elapsed(Duration::from_millis(999)), "999 ms");
        assert_eq!(format_elapsed(Duration::from_millis(1240)), "1.24 s");
    }

    #[test]
    fn a_statement_is_flattened_to_one_line_with_its_outcome_and_time() {
        let mut log = MessageLog::default();
        log.statement_ok("SELECT a,\n   b\nFROM t", 3, false, Duration::from_millis(12));
        log.statement_ok("SELECT 1", 100_000, true, Duration::from_millis(0));
        log.statement_failed("DELETE FROM t", "ReadOnly", Duration::from_millis(4));
        let entries = log.entries();
        assert_ne!(entries[0].id, entries[1].id);
        assert_ne!(entries[1].id, entries[2].id);
        assert_eq!(entries[0].text, "SELECT a, b FROM t · 3 rows · 12 ms");
        assert_eq!(
            entries[1].text,
            "SELECT 1 · 100000 rows (result may be incomplete) · 0 ms"
        );
        assert_eq!(
            (entries[2].kind, entries[2].text.as_str()),
            (MessageKind::Error, "DELETE FROM t · ReadOnly · 4 ms")
        );
    }

    #[test]
    fn a_long_statement_is_cut_on_a_character_boundary() {
        let mut log = MessageLog::default();
        log.statement_ok(&"é".repeat(300), 1, false, Duration::ZERO);
        let text = &log.entries()[0].text;
        assert!(text.starts_with(&"é".repeat(MAX_STATEMENT_CHARS)));
        assert!(text.contains('…'));
    }

    #[test]
    fn the_log_keeps_only_the_newest_entries() {
        let mut log = MessageLog::default();
        for number in 0..(MAX_MESSAGES + 5) {
            log.push(MessageKind::Info, number.to_string());
        }
        assert_eq!(log.entries().len(), MAX_MESSAGES);
        assert_eq!(log.entries()[0].text, "5");
        assert_eq!(log.entries()[0].id, 5);
        assert_eq!(log.entries().last().unwrap().id, (MAX_MESSAGES + 4) as u64);
        assert_eq!(log.entries().last().unwrap().text, (MAX_MESSAGES + 4).to_string());
    }
}
