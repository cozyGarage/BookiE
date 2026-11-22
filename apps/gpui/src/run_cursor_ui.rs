use std::ops::Range;

use bookie_app_services::statement_plan::{statement_at_cursor, statement_range_at};

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

pub(crate) fn run_source_range(text: &str, driver: &str, cursor: usize, selection: Range<usize>) -> Option<Range<usize>> {
    if selection.start < selection.end {
        let selected = text.get(selection.clone())?;
        let leading = selected.len() - selected.trim_start().len();
        let length = selected.trim().len();
        return (length > 0).then_some(selection.start + leading..selection.start + leading + length);
    }
    statement_range_at(text, driver, cursor)
}

pub(crate) fn error_location(editor: &str, submitted: &str, position: usize) -> Option<(u32, u32)> {
    if editor != submitted || position == 0 {
        return None;
    }
    let byte = submitted.char_indices().nth(position - 1)?.0;
    let prefix = &submitted[..byte];
    let line = prefix.bytes().filter(|&byte| byte == b'\n').count();
    let column = prefix.rsplit('\n').next()?.chars().count();
    Some((u32::try_from(line).ok()?, u32::try_from(column).ok()?))
}

pub(crate) fn mapped_error_location(editor: &str, sql: &str, position: usize, source: Option<&(String, Range<usize>)>) -> Option<((u32, u32), Range<usize>)> {
    let start = match source {
        Some((snapshot, range)) if editor == snapshot && snapshot.get(range.clone()) == Some(sql) => range.start,
        None if editor == sql => 0,
        _ => return None,
    };
    sql.chars().nth(position.checked_sub(1)?)?;
    let absolute = editor[..start].chars().count().checked_add(position)?;
    let location = error_location(editor, editor, absolute)?;
    let token = error_token(sql, position)?;
    Some((location, start + token.start..start + token.end))
}

pub(crate) fn error_token(sql: &str, position: usize) -> Option<Range<usize>> {
    let (offset, ch) = sql.char_indices().nth(position.checked_sub(1)?)?;
    let identifier = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    if !identifier(ch) {
        return Some(offset..offset + ch.len_utf8());
    }
    let start = sql[..offset]
        .char_indices()
        .rev()
        .find(|(_, c)| !identifier(*c))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let end = sql[offset..]
        .char_indices()
        .find(|(_, c)| !identifier(*c))
        .map_or(sql.len(), |(index, _)| offset + index);
    Some(start..end)
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
        let range = run_source_range(&text, &driver, cursor, selection.clone());
        match run_target(&text, &driver, cursor, selection) {
            Ok(sql) if bookie_app_services::query_parameters::statement_names(&sql, &driver).is_empty() => {
                let source = range.filter(|range| text.get(range.clone()) == Some(sql.as_str())).map(|range| (text, range));
                self.run_sql_with_source(sql, Vec::new(), source, cx);
            }
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
