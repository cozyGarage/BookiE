use bookie_core::{MessageSeverity, ServerMessage, ServerMessages};
use tiberius::ServerInfoMessage;

use crate::MssqlClient;

pub(crate) fn discard(client: &mut MssqlClient) {
    let _ = client.take_server_info();
}

pub(crate) fn drain(client: &mut MssqlClient) -> ServerMessages {
    let (infos, dropped) = client.take_server_info();
    to_messages(&infos, dropped)
}

fn to_messages(infos: &[ServerInfoMessage], dropped: u64) -> ServerMessages {
    let mut messages = ServerMessages::default();
    for info in infos {
        let code = (info.number != 0).then(|| info.number.to_string());
        messages.push(ServerMessage::new(MessageSeverity::Info, code, &info.message));
    }
    messages.record_omitted(dropped);
    messages
}

#[cfg(test)]
mod tests {
    use bookie_core::MessageSeverity;
    use tiberius::ServerInfoMessage;

    use super::to_messages;

    fn info(number: u32, text: &str) -> ServerInfoMessage {
        ServerInfoMessage {
            number,
            state: 1,
            class: 0,
            message: text.into(),
            server: String::new(),
            procedure: String::new(),
            line: 1,
        }
    }

    #[test]
    fn print_has_no_code_and_a_numbered_message_keeps_its_number() {
        let messages = to_messages(&[info(0, "printed"), info(50000, "raised")], 3);
        let items: Vec<_> = messages.iter().collect();
        assert_eq!(items[0].code, None);
        assert_eq!(items[0].severity, MessageSeverity::Info);
        assert_eq!(items[1].code.as_deref(), Some("50000"));
        assert_eq!(messages.omitted(), 3);
    }
}
