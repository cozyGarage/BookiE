use std::time::Duration;

pub const MAX_MESSAGES: usize = 200;
const MAX_STATEMENT_CHARS: usize = 120;

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
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MessageLog {
    entries: Vec<Message>,
}

impl MessageLog {
    pub fn push(&mut self, kind: MessageKind, text: impl Into<String>) {
        if self.entries.len() == MAX_MESSAGES {
            self.entries.remove(0);
        }
        self.entries.push(Message {
            kind,
            text: text.into(),
        });
    }

    pub fn entries(&self) -> &[Message] {
        &self.entries
    }

    pub fn statement_ok(&mut self, sql: &str, rows: u64, truncated: bool, elapsed: Duration) {
        let more = if truncated { " (more rows exist)" } else { "" };
        self.push(
            MessageKind::Success,
            format!("{} · {rows} rows{more} · {} ms", one_line(sql), elapsed.as_millis()),
        );
    }

    pub fn statement_failed(&mut self, sql: &str, reason: &str, elapsed: Duration) {
        self.push(
            MessageKind::Error,
            format!("{} · {reason} · {} ms", one_line(sql), elapsed.as_millis()),
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

fn one_line(sql: &str) -> String {
    let flat = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(MAX_STATEMENT_CHARS) {
        Some((end, _)) => format!("{}…", &flat[..end]),
        None => flat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(entries[0].text, "SELECT a, b FROM t · 3 rows · 12 ms");
        assert_eq!(entries[1].text, "SELECT 1 · 100000 rows (more rows exist) · 0 ms");
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
        assert_eq!(log.entries().last().unwrap().text, (MAX_MESSAGES + 4).to_string());
    }
}
