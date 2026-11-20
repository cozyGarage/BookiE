use std::ops::Range;

use bookie_app_services::statement_plan::statement_at_cursor;

use super::*;

pub(crate) fn run_target(text: &str, driver: &str, cursor: usize, selection: Range<usize>) -> Result<String, String> {
    if selection.start < selection.end {
        let chosen = text.get(selection).unwrap_or_default().trim();
        return if chosen.is_empty() {
            Err("The selection is empty; select a statement or place the cursor inside one".into())
        } else {
            Ok(chosen.to_owned())
        };
    }
    if text.trim().is_empty() {
        return Err("Type a statement first".into());
    }
    statement_at_cursor(text, driver, cursor)
        .filter(|statement| !statement.is_empty())
        .ok_or_else(|| "The cursor is not inside a complete statement".into())
}

impl GridView {
    pub(crate) fn run_cursor_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "run-at-cursor",
            label: "Run the selection or the statement at the cursor".to_string(),
            enabled: matches!(self.tabs.current(), Tab::Query),
        }]
    }

    pub(crate) fn run_at_cursor(&mut self, cx: &mut Context<Self>) {
        let driver = self
            .live
            .as_ref()
            .map_or_else(|| "postgres".to_string(), |session| session.driver_id.clone());
        let editor = self.sql_input.read(cx);
        let (text, cursor, selection) = (editor.value().to_string(), editor.cursor(), editor.selected_range());
        match run_target(&text, &driver, cursor, selection) {
            Ok(sql) => self.run_with_parameters(sql, cx),
            Err(message) => {
                self.status = message;
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
#[path = "run_cursor_ui_tests.rs"]
mod tests;
