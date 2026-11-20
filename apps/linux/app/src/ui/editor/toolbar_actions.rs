use crate::ui::spacing;
use std::cell::Cell;
use std::rc::Rc;

use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};
use uuid::Uuid;

use super::sql_text::text_to_run;
use super::{RunMessage, SqlEditor, SqlEditorInput, StatementOutcomeKind};

pub(super) struct TransactionActions {
    pub(super) row: gtk::FlowBox,
    id: Rc<Cell<Option<Uuid>>>,
}

impl TransactionActions {
    pub(super) fn install(toolbar: &adw::ToolbarView, sender: &ComponentSender<SqlEditor>) -> Self {
        let row = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(1)
            .max_children_per_line(3)
            .column_spacing(spacing::grid(spacing::SM))
            .row_spacing(spacing::grid(spacing::XS))
            .margin_start(spacing::SM)
            .margin_end(spacing::SM)
            .visible(false)
            .build();
        row.set_widget_name("editor-transaction-actions");
        let label = gtk::Label::new(Some(&bookie_app_services::tr!("Transaction active")));
        label.add_css_class("warning");
        row.insert(&label, -1);
        let id = Rc::new(Cell::new(None));
        for (label, commit) in [
            (bookie_app_services::tr!("Commit & close session"), true),
            (bookie_app_services::tr!("Roll back & close session"), false),
        ] {
            let button = gtk::Button::with_label(&label);
            let active = id.clone();
            let sender = sender.clone();
            button.connect_clicked(move |_| {
                if let Some(session_id) = active.get() {
                    sender.input(SqlEditorInput::Run(RunMessage::SessionAction { session_id, commit }));
                }
            });
            row.insert(&button, -1);
        }
        toolbar.add_top_bar(&row);
        Self { row, id }
    }
}

impl SqlEditor {
    pub(super) fn on_run_message(&mut self, message: RunMessage, sender: ComponentSender<Self>) {
        match message {
            RunMessage::Run => self.run_all(sender),
            RunMessage::SessionAction { session_id, commit } => {
                self.on_session_toolbar_action(session_id, commit, &sender);
            }
            RunMessage::RunWithParameters {
                sql,
                anchor,
                planned,
                values,
            } => {
                self.execute_sql(sql, anchor, planned, values, sender);
            }
            RunMessage::Cancel => self.cancel_run(),
            RunMessage::RunWithLongerTimeout => self.retry_with_longer_timeout(sender),
            RunMessage::RunAtCursor => self.run_scope(true, sender),
            RunMessage::Explain => self.explain_at_cursor(),
            RunMessage::ShowTemporaryTable { schema, name } => {
                let driver_id = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
                self.begin_run(
                    temporary_table_query(&driver_id, schema.as_deref(), &name),
                    None,
                    true,
                    sender,
                );
            }
        }
    }

    fn run_all(&mut self, sender: ComponentSender<Self>) {
        self.timeout_retry.clear_override();
        let sql = text_to_run(&self.source_view.buffer());
        let trimmed = sql.trim().to_string();
        if trimmed.is_empty() {
            self.status.set_label(&bookie_app_services::tr!("empty query"));
            return;
        }
        let buffer = self.source_view.buffer();
        let start = buffer
            .selection_bounds()
            .filter(|(start, end)| !buffer.text(start, end, true).trim().is_empty())
            .map_or(0, |(start, _)| start.offset() as usize);
        let anchor = super::SourceAnchor {
            revision: self.drop_generation.revision(),
            start: start + sql.chars().take_while(|character| character.is_whitespace()).count(),
        };
        self.begin_run(trimmed, Some(anchor), false, sender);
    }

    fn cancel_run(&mut self) {
        if self.cancel_button.is_visible()
            && let Some(token) = self.cancel_token.take()
        {
            token.cancel();
        }
    }

    fn retry_with_longer_timeout(&mut self, sender: ComponentSender<Self>) {
        let Some(sql) = self.timeout_retry.accept() else {
            return;
        };
        self.longer_timeout_button.set_visible(false);
        self.begin_run(sql, None, false, sender);
    }

    fn explain_at_cursor(&self) {
        let buffer = self.source_view.buffer();
        let text = if let Some((a, b)) = buffer.selection_bounds() {
            buffer.text(&a, &b, true).to_string()
        } else {
            let (start, end) = buffer.bounds();
            let all = buffer.text(&start, &end, true).to_string();
            let cursor_chars = buffer.iter_at_mark(&buffer.get_insert()).offset() as usize;
            let driver_id = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
            let cursor = super::cursor_byte_offset(&all, cursor_chars);
            super::statement_at_cursor(&all, self.lexing(&driver_id), cursor).unwrap_or(all)
        };
        if let Some(window) = self
            .source_view
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok())
        {
            crate::ui::explain_dialog::present(&window, self.connection_id, &text, &self.database, &self.preferences);
        }
    }

    pub(super) fn run_scope(&mut self, current: bool, sender: ComponentSender<Self>) {
        self.timeout_retry.clear_override();
        let buffer = self.source_view.buffer();
        let (start, end) = buffer.bounds();
        let sql = buffer.text(&start, &end, true).to_string();
        if current
            && self
                .metadata()
                .is_some_and(|metadata| super::grammar_for(&metadata.driver_id).is_none())
        {
            let cursor = super::cursor_byte_offset(&sql, buffer.cursor_position() as usize);
            let driver = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
            if let Some(text) = super::statement_at_cursor(&sql, self.lexing(&driver), cursor) {
                self.begin_run(text, None, true, sender);
            } else {
                self.status.set_label(&bookie_app_services::tr!("No statement to run"));
            }
            return;
        }
        let range = if current {
            let cursor = super::cursor_byte_offset(&sql, buffer.iter_at_mark(&buffer.get_insert()).offset() as usize);
            let driver = self.metadata().map(|metadata| metadata.driver_id).unwrap_or_default();
            super::statement_cursor::statement_range_at(&sql, self.lexing(&driver), cursor)
        } else {
            let start = sql.len() - sql.trim_start().len();
            (!sql.trim().is_empty()).then_some(start..start + sql.trim().len())
        };
        let Some(range) = range else {
            self.status.set_label(&bookie_app_services::tr!("No statement to run"));
            return;
        };
        let anchor = super::SourceAnchor {
            revision: self.drop_generation.revision(),
            start: sql[..range.start].chars().count(),
        };
        self.begin_run(sql[range].to_string(), Some(anchor), current, sender);
    }

    pub(super) fn session_action_idle(&self) -> bool {
        self.run_generation.active.is_empty()
            && !self.reading_lexing
            && !self.session_open_in_flight
            && self.ending_session_id.is_none()
            && !self.session_teardown_active
    }

    pub(super) fn sync_session_controls(&self) {
        let id = self.toolbar_transaction_id();
        let clock = self.transaction_clock.label_at(std::time::Instant::now());
        let label = self.sync_schema_entry(&clock);
        self.session_button.set_label(&label);
        self.session_button
            .update_property(&[gtk::accessible::Property::Label(&label)]);
        self.transaction_actions.id.set(id);
        self.transaction_actions.row.set_visible(id.is_some());
        self.transaction_actions.row.set_sensitive(self.session_action_idle());
        self.run_options.set_sensitive(self.session_action_idle());
    }

    pub(super) fn refresh_run_options(&self, sender: &ComponentSender<Self>) {
        let group = gtk::gio::SimpleActionGroup::new();
        for (name, current) in [("all", false), ("current", true)] {
            let action = gtk::gio::SimpleAction::new(name, None);
            let sender = sender.clone();
            action.connect_activate(move |_, _| {
                sender.input(if current {
                    SqlEditorInput::Run(RunMessage::RunAtCursor)
                } else {
                    SqlEditorInput::Run(RunMessage::Run)
                })
            });
            group.add_action(&action);
        }
        if let Some(parent) = self.run_options.parent() {
            parent.insert_action_group("editor-run", Some(&group));
        }
        self.source_view.insert_action_group("editor-run", Some(&group));
        if let Some(root) = self.source_view.ancestor(adw::BreakpointBin::static_type()) {
            root.insert_action_group("editor-run", Some(&group));
        }
        self.refresh_result_exports();
    }

    pub(super) fn refresh_result_exports(&self) {
        let menu = gtk::gio::Menu::new();
        for (label, action, accelerator) in run_menu_items() {
            let item = gtk::gio::MenuItem::new(Some(&label), Some(action));
            item.set_attribute_value("accel", Some(&accelerator.to_variant()));
            menu.append_item(&item);
        }
        let exports = gtk::gio::Menu::new();
        let actions = gtk::gio::SimpleActionGroup::new();
        for (statement, outcome) in self.pinned.latest().iter().enumerate() {
            match &outcome.kind {
                StatementOutcomeKind::Rows(results) => {
                    for (result_set, result) in results.iter().enumerate() {
                        if result.columns.is_empty() {
                            continue;
                        }
                        let name = format!("result-{statement}-{result_set}");
                        let label =
                            bookie_app_services::tr!("Export loaded rows: statement {statement}, result {result}")
                                .replace("{statement}", &(statement + 1).to_string())
                                .replace("{result}", &(result_set + 1).to_string());
                        let action = gtk::gio::SimpleAction::new(&name, None);
                        let result = result.clone();
                        let sender = self.grid_sender.clone();
                        action.connect_activate(move |_, _| {
                            let _ = sender.send(super::GridMsg::ExportResults(result.clone()));
                        });
                        actions.add_action(&action);
                        exports.append(Some(&label), Some(&format!("editor-export.{name}")));
                    }
                }
                StatementOutcomeKind::Streamed(result)
                    if matches!(result.terminal, super::StreamTerminal::Complete) =>
                {
                    if let Some(request) = result.export.clone().filter(|request| request.spool.is_some()) {
                        let action = gtk::gio::SimpleAction::new("all-query-rows", None);
                        let sender = self.grid_sender.clone();
                        action.connect_activate(move |_, _| {
                            let _ = sender.send(super::GridMsg::ExportStreamedResults(request.clone()));
                        });
                        actions.add_action(&action);
                        exports.append(
                            Some(&bookie_app_services::tr!("Export all rows from this result…")),
                            Some("editor-export.all-query-rows"),
                        );
                    }
                }
                _ => {}
            }
        }
        self.run_options.insert_action_group("editor-export", Some(&actions));
        if exports.n_items() > 0 {
            menu.append_section(Some(&bookie_app_services::tr!("File output")), &exports);
        }
        self.run_options.set_menu_model(Some(&menu));
    }
}

pub(super) fn run_menu_items() -> [(String, &'static str, &'static str); 2] {
    [
        (
            bookie_app_services::tr!("Run selection or all statements"),
            "editor-run.all",
            "<Primary>Return",
        ),
        (
            bookie_app_services::tr!("Run current statement"),
            "editor-run.current",
            "<Primary><Shift>Return",
        ),
    ]
}

fn temporary_table_query(driver_id: &str, schema: Option<&str>, name: &str) -> String {
    let table = bookie_core::sql_dialect::quote_ident(driver_id, name);
    let relation = schema.map_or(table.clone(), |schema| {
        format!("{}.{}", bookie_core::sql_dialect::quote_ident(driver_id, schema), table)
    });
    format!("SELECT * FROM {relation}")
}

#[cfg(test)]
mod tests {
    use super::temporary_table_query;

    #[test]
    fn a_temporary_table_query_quotes_each_identifier() {
        assert_eq!(
            temporary_table_query("postgres", Some("pg_temp_4"), "x\"; DROP TABLE users;--"),
            "SELECT * FROM \"pg_temp_4\".\"x\"\"; DROP TABLE users;--\""
        );
    }

    #[test]
    fn a_mariadb_temporary_table_query_uses_mysql_identifiers() {
        assert_eq!(
            temporary_table_query("mysql", Some("test"), "scratch"),
            "SELECT * FROM `test`.`scratch`"
        );
    }
}
