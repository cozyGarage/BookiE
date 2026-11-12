use bookie_core::{MAX_SERVER_MESSAGES, MessageSeverity, ServerMessage, ServerMessages, Value};
use futures::TryStreamExt;
use sqlx::mysql::MySql;
use sqlx::pool::PoolConnection;

use crate::decode::extract_value;

pub(crate) fn discard(connection: &mut PoolConnection<MySql>) {
    let _ = connection.take_warning_count();
}

pub(crate) async fn collect(connection: &mut PoolConnection<MySql>) -> ServerMessages {
    let count = connection.take_warning_count();
    if count == 0 {
        return ServerMessages::default();
    }
    let rows = match show_warnings(connection).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "the server warnings could not be read");
            Vec::new()
        }
    };
    to_messages(&rows, u64::from(count))
}

async fn show_warnings(connection: &mut PoolConnection<MySql>) -> Result<Vec<Vec<Value>>, sqlx::Error> {
    let sql = format!("SHOW WARNINGS LIMIT {MAX_SERVER_MESSAGES}");
    let mut stream = sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).fetch(&mut **connection);
    let mut rows = Vec::new();
    while let Some(row) = stream.try_next().await? {
        rows.push((0..3).map(|index| extract_value(&row, index)).collect());
    }
    Ok(rows)
}

fn to_messages(rows: &[Vec<Value>], reported: u64) -> ServerMessages {
    let mut messages = ServerMessages::default();
    for row in rows {
        messages.push(message_from_row(row));
    }
    messages.record_omitted(reported.saturating_sub(rows.len() as u64));
    messages
}

fn message_from_row(row: &[Value]) -> ServerMessage {
    let severity = match row.first() {
        Some(Value::Text(level)) => severity(level),
        _ => MessageSeverity::Warning,
    };
    let code = match row.get(1) {
        Some(Value::Int(code)) => Some(code.to_string()),
        Some(Value::Text(code)) if !code.is_empty() => Some(code.clone()),
        _ => None,
    };
    let text = match row.get(2) {
        Some(Value::Text(text)) => text.as_str(),
        _ => "",
    };
    ServerMessage::new(severity, code, text)
}

fn severity(level: &str) -> MessageSeverity {
    match level.to_ascii_lowercase().as_str() {
        "note" => MessageSeverity::Notice,
        "error" => MessageSeverity::Error,
        _ => MessageSeverity::Warning,
    }
}

#[cfg(test)]
mod tests {
    use bookie_core::{MessageSeverity, Value};

    use super::to_messages;

    fn row(level: &str, code: i64, text: &str) -> Vec<Value> {
        vec![Value::Text(level.into()), Value::Int(code), Value::Text(text.into())]
    }

    #[test]
    fn show_warnings_rows_become_messages_with_levels_and_codes() {
        let rows = [
            row("Warning", 1292, "Truncated incorrect INTEGER value"),
            row("Note", 1051, "gone"),
        ];
        let messages = to_messages(&rows, 2);
        let items: Vec<_> = messages.iter().collect();
        assert_eq!(items[0].severity, MessageSeverity::Warning);
        assert_eq!(items[0].code.as_deref(), Some("1292"));
        assert_eq!(items[1].severity, MessageSeverity::Notice);
        assert_eq!(messages.omitted(), 0);
    }

    #[test]
    fn warnings_the_server_counted_but_did_not_list_are_reported_as_omitted() {
        let messages = to_messages(&[row("Warning", 1265, "Data truncated")], 5);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages.omitted(), 4);
    }

    #[test]
    fn a_count_without_rows_still_reports_that_warnings_exist() {
        let messages = to_messages(&[], 3);
        assert!(!messages.is_empty());
        assert_eq!(messages.omitted(), 3);
    }
}
