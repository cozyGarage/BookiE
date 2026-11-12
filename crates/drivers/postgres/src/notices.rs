use bookie_core::{MessageSeverity, ServerMessage, ServerMessages};
use sqlx::Postgres;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgServerNotice, PgSeverity};

pub(crate) fn discard(connection: &mut PoolConnection<Postgres>) {
    let _ = connection.take_server_notices();
}

pub(crate) fn drain(connection: &mut PoolConnection<Postgres>) -> ServerMessages {
    let (notices, dropped) = connection.take_server_notices();
    to_messages(&notices, dropped)
}

fn to_messages(notices: &[PgServerNotice], dropped: u64) -> ServerMessages {
    let mut messages = ServerMessages::default();
    for notice in notices {
        let code = (!notice.code.is_empty()).then(|| notice.code.clone());
        messages.push(ServerMessage::new(severity(notice.severity), code, &notice.message));
    }
    messages.record_omitted(dropped);
    messages
}

fn severity(severity: PgSeverity) -> MessageSeverity {
    match severity {
        PgSeverity::Debug => MessageSeverity::Debug,
        PgSeverity::Info | PgSeverity::Log => MessageSeverity::Info,
        PgSeverity::Notice => MessageSeverity::Notice,
        PgSeverity::Warning => MessageSeverity::Warning,
        PgSeverity::Error | PgSeverity::Fatal | PgSeverity::Panic => MessageSeverity::Error,
    }
}

#[cfg(test)]
mod tests {
    use bookie_core::MessageSeverity;
    use sqlx::postgres::{PgServerNotice, PgSeverity};

    use super::to_messages;

    fn notice(severity: PgSeverity, code: &str, message: &str) -> PgServerNotice {
        PgServerNotice {
            severity,
            code: code.into(),
            message: message.into(),
        }
    }

    #[test]
    fn notices_convert_with_severity_code_and_the_dropped_count() {
        let messages = to_messages(
            &[
                notice(PgSeverity::Warning, "01000", "careful"),
                notice(PgSeverity::Log, "", "logged"),
            ],
            7,
        );
        let items: Vec<_> = messages.iter().collect();
        assert_eq!(items[0].severity, MessageSeverity::Warning);
        assert_eq!(items[0].code.as_deref(), Some("01000"));
        assert_eq!(items[1].severity, MessageSeverity::Info);
        assert_eq!(items[1].code, None);
        assert_eq!(messages.omitted(), 7);
    }
}
