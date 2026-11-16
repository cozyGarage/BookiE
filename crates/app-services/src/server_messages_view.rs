use bookie_core::{MessageSeverity, ServerMessages};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    pub severity: MessageSeverity,
    pub severity_label: String,
    pub code: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessagesView {
    rows: Vec<MessageRow>,
    omitted: u64,
}

impl MessagesView {
    pub fn new(messages: &ServerMessages) -> Self {
        Self {
            rows: messages.iter().map(row_for).collect(),
            omitted: messages.omitted(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.omitted == 0
    }

    pub fn rows(&self) -> &[MessageRow] {
        &self.rows
    }

    pub fn total(&self) -> u64 {
        (self.rows.len() as u64).saturating_add(self.omitted)
    }

    pub fn title(&self) -> String {
        crate::tr!("Messages ({n})").replace("{n}", &self.total().to_string())
    }

    pub fn status_suffix(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let total = self.total();
        Some(crate::trn!("{n} message", "{n} messages", total).replace("{n}", &total.to_string()))
    }

    pub fn omitted_notice(&self) -> Option<String> {
        (self.omitted > 0).then(|| {
            crate::trn!(
                "{n} more message not shown",
                "{n} more messages not shown",
                self.omitted
            )
            .replace("{n}", &self.omitted.to_string())
        })
    }

    pub fn copy_text(&self) -> String {
        let mut lines: Vec<String> = self.rows.iter().map(copy_line).collect();
        lines.extend(self.omitted_notice());
        lines.join("\n")
    }
}

pub fn status_with_messages(status: &str, view: &MessagesView) -> String {
    match view.status_suffix() {
        Some(suffix) => format!("{status} · {suffix}"),
        None => status.to_string(),
    }
}

fn row_for(message: &bookie_core::ServerMessage) -> MessageRow {
    MessageRow {
        severity: message.severity,
        severity_label: severity_label(message.severity),
        code: message.code.clone(),
        text: message.text.clone(),
    }
}

fn copy_line(row: &MessageRow) -> String {
    match &row.code {
        Some(code) => format!("{} [{}] {}", row.severity_label, code, row.text),
        None => format!("{} {}", row.severity_label, row.text),
    }
}

fn severity_label(severity: MessageSeverity) -> String {
    match severity {
        MessageSeverity::Debug => crate::tr!("Debug"),
        MessageSeverity::Info => crate::tr!("Info"),
        MessageSeverity::Notice => crate::tr!("Notice"),
        MessageSeverity::Warning => crate::tr!("Warning"),
        MessageSeverity::Error => crate::tr!("Error"),
    }
}

#[cfg(test)]
#[path = "server_messages_view_tests.rs"]
mod tests;
