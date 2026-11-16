//! Bounded buffer for the informational messages (INFO tokens) a server sends
//! while a statement runs: `PRINT` output and `RAISERROR` at severity 10 or
//! below.

use crate::tds::codec::TokenInfo;

/// The most messages kept per connection between two calls to
/// [`Client::take_server_info`](crate::Client::take_server_info).
pub const MAX_BUFFERED_SERVER_INFO: usize = 256;

/// The longest message text kept, in bytes.
pub const MAX_SERVER_INFO_BYTES: usize = 8192;

/// One informational message from the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfoMessage {
    /// The message number (`PRINT` uses 0).
    pub number: u32,
    /// The error state.
    pub state: u8,
    /// The severity class (10 or below for informational messages).
    pub class: u8,
    /// The message text, cut to [`MAX_SERVER_INFO_BYTES`].
    pub message: String,
    /// The server name.
    pub server: String,
    /// The procedure that raised the message, empty outside a procedure.
    pub procedure: String,
    /// The line number in the batch or procedure.
    pub line: u32,
}

#[derive(Debug, Default)]
pub(crate) struct ServerInfoBuffer {
    messages: Vec<ServerInfoMessage>,
    dropped: u64,
}

impl ServerInfoBuffer {
    pub(crate) fn push(&mut self, info: &TokenInfo) {
        self.push_message(ServerInfoMessage {
            number: info.number,
            state: info.state,
            class: info.class,
            message: info.message.clone(),
            server: info.server.clone(),
            procedure: info.procedure.clone(),
            line: info.line,
        });
    }

    pub(crate) fn push_message(&mut self, mut message: ServerInfoMessage) {
        if self.messages.len() >= MAX_BUFFERED_SERVER_INFO {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        if message.message.len() > MAX_SERVER_INFO_BYTES {
            let mut end = MAX_SERVER_INFO_BYTES;
            while !message.message.is_char_boundary(end) {
                end -= 1;
            }
            message.message.truncate(end);
        }
        self.messages.push(message);
    }

    pub(crate) fn take(&mut self) -> (Vec<ServerInfoMessage>, u64) {
        (std::mem::take(&mut self.messages), std::mem::take(&mut self.dropped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(text: &str) -> ServerInfoMessage {
        ServerInfoMessage {
            number: 0,
            state: 1,
            class: 0,
            message: text.to_owned(),
            server: String::new(),
            procedure: String::new(),
            line: 1,
        }
    }

    #[test]
    fn take_returns_messages_in_order_and_empties_the_buffer() {
        let mut buffer = ServerInfoBuffer::default();
        buffer.push_message(message("first"));
        buffer.push_message(message("second"));
        let (messages, dropped) = buffer.take();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].message, "first");
        assert_eq!(messages[1].message, "second");
        assert_eq!(dropped, 0);
        let (again, dropped_again) = buffer.take();
        assert!(again.is_empty());
        assert_eq!(dropped_again, 0);
    }

    #[test]
    fn a_flood_keeps_the_cap_and_counts_the_rest() {
        let mut buffer = ServerInfoBuffer::default();
        for index in 0..MAX_BUFFERED_SERVER_INFO + 10 {
            buffer.push_message(message(&index.to_string()));
        }
        let (messages, dropped) = buffer.take();
        assert_eq!(messages.len(), MAX_BUFFERED_SERVER_INFO);
        assert_eq!(dropped, 10);
    }

    #[test]
    fn long_text_is_cut_on_a_character_boundary() {
        let mut buffer = ServerInfoBuffer::default();
        buffer.push_message(message(&"é".repeat(MAX_SERVER_INFO_BYTES)));
        let (messages, _) = buffer.take();
        assert!(messages[0].message.len() <= MAX_SERVER_INFO_BYTES);
        assert!(messages[0].message.starts_with('é'));
    }
}
