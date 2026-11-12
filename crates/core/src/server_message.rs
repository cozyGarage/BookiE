pub const MAX_SERVER_MESSAGES: usize = 100;
pub const MAX_SERVER_MESSAGE_TEXT_BYTES: usize = 4096;
pub const MAX_SERVER_MESSAGES_TOTAL_BYTES: usize = 64 * 1024;

const TRUNCATION_MARK: char = '…';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSeverity {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMessage {
    pub severity: MessageSeverity,
    pub code: Option<String>,
    pub text: String,
}

impl ServerMessage {
    pub fn new(severity: MessageSeverity, code: Option<String>, text: &str) -> Self {
        Self {
            severity,
            code,
            text: cap_text(text),
        }
    }
}

fn cap_text(text: &str) -> String {
    if text.len() <= MAX_SERVER_MESSAGE_TEXT_BYTES {
        return text.to_string();
    }
    let mut end = MAX_SERVER_MESSAGE_TEXT_BYTES.saturating_sub(TRUNCATION_MARK.len_utf8());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut capped = String::with_capacity(end + TRUNCATION_MARK.len_utf8());
    capped.push_str(&text[..end]);
    capped.push(TRUNCATION_MARK);
    capped
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerMessages {
    items: Vec<ServerMessage>,
    bytes: usize,
    omitted: u64,
}

impl ServerMessages {
    pub fn push(&mut self, message: ServerMessage) {
        let size = message.text.len();
        let fits = self.items.len() < MAX_SERVER_MESSAGES
            && self.bytes.saturating_add(size) <= MAX_SERVER_MESSAGES_TOTAL_BYTES;
        if !fits {
            self.omitted = self.omitted.saturating_add(1);
            return;
        }
        self.bytes += size;
        self.items.push(message);
    }

    pub fn record_omitted(&mut self, count: u64) {
        self.omitted = self.omitted.saturating_add(count);
    }

    pub fn extend(&mut self, other: ServerMessages) {
        self.omitted = self.omitted.saturating_add(other.omitted);
        for message in other.items {
            self.push(message);
        }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, ServerMessage> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.omitted == 0
    }

    pub fn omitted(&self) -> u64 {
        self.omitted
    }

    pub fn total(&self) -> u64 {
        (self.items.len() as u64).saturating_add(self.omitted)
    }
}

impl<'a> IntoIterator for &'a ServerMessages {
    type Item = &'a ServerMessage;
    type IntoIter = std::slice::Iter<'a, ServerMessage>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

#[cfg(test)]
#[path = "server_message_tests.rs"]
mod tests;
