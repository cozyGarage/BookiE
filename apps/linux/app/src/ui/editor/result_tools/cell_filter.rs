use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use relm4::gtk;
use relm4::gtk::prelude::*;

use bookie_app_services::result_predicate::ResultPredicate;
use bookie_core::{ColumnInfo, Value};
use relm4::ComponentSender;

use super::super::stream_pages::StreamedResult;
use super::super::{SqlEditor, SqlEditorInput, StatementIdentity, StatementOutcomeKind};
use super::actions::filtering_text;
use super::editor::FilterRequest;
use super::ticket::{FilterTicket, register, running};
use crate::ui::grid::{GridMsg, value_to_display_text};

const LABEL_VALUE_CHARS: usize = 24;

pub(super) fn cell_predicate(
    columns: &[ColumnInfo],
    column: usize,
    value: Value,
    exclude: bool,
) -> Option<(ResultPredicate, String)> {
    if matches!(value, Value::Undecodable(_)) {
        return None;
    }
    let name = &columns.get(column)?.name;
    let text: String = value_to_display_text(&value).chars().take(LABEL_VALUE_CHARS).collect();
    let operator = if exclude { "≠" } else { "=" };
    let label = format!("{name} {operator} {text}");
    let predicate = if exclude {
        ResultPredicate::NotEquals { column, value }
    } else {
        ResultPredicate::Equals { column, value }
    };
    Some((predicate, label))
}

impl SqlEditor {
    fn streamed_result_for(&self, identity: StatementIdentity) -> Option<StreamedResult> {
        self.pinned
            .displayed()
            .into_iter()
            .filter(|outcome| outcome.identity == identity)
            .find_map(|outcome| match outcome.kind {
                StatementOutcomeKind::Streamed(result) => Some(*result),
                _ => None,
            })
    }

    pub(in crate::ui::editor) fn on_result_grid_message(&mut self, message: GridMsg, sender: &ComponentSender<Self>) {
        let GridMsg::FilterWholeResult {
            identity,
            column,
            value,
            exclude,
        } = message
        else {
            return;
        };
        let Some(source) = self.streamed_result_for(identity) else {
            self.status.set_label(&crate::tr!("This result is no longer available"));
            return;
        };
        let Some((predicate, label)) = cell_predicate(&source.columns, column, value, exclude) else {
            self.status
                .set_label(&crate::tr!("Values of this type can't be used as a filter"));
            return;
        };
        if running(identity) {
            self.status
                .set_label(&crate::tr!("A filter is already running on this result"));
            return;
        }
        let ticket = Arc::new(FilterTicket::default());
        register(identity, &ticket);
        self.status.set_label(&filtering_text(0, source.total_rows as u64));
        watch_cell_filter(&self.results_holder, &self.status, &ticket, source.total_rows as u64);
        sender.input(SqlEditorInput::FilterResults(Box::new(FilterRequest {
            source,
            identity,
            predicate,
            label,
            ticket,
        })));
    }
}

fn find_named(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_named(&widget, name) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn watch_cell_filter(holder: &gtk::Box, status: &gtk::Label, ticket: &Arc<FilterTicket>, total: u64) {
    let stop =
        find_named(holder.upcast_ref(), "tp-result-find-stop").and_then(|widget| widget.downcast::<gtk::Button>().ok());
    if let Some(stop) = stop {
        show_stop(&stop, ticket);
    }
    let status = status.downgrade();
    let ticket = ticket.clone();
    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        let (Some(status), false) = (status.upgrade(), ticket.is_finished()) else {
            return gtk::glib::ControlFlow::Break;
        };
        status.set_label(&filtering_text(ticket.progress().rows_done(), total));
        gtk::glib::ControlFlow::Continue
    });
}

fn show_stop(stop: &gtk::Button, ticket: &Arc<FilterTicket>) {
    stop.set_visible(true);
    let cancelling = ticket.clone();
    let handler = Cell::new(Some(stop.connect_clicked(move |_| cancelling.cancel().cancel())));
    let stop = stop.downgrade();
    let ticket = ticket.clone();
    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        if !ticket.is_finished() {
            return gtk::glib::ControlFlow::Continue;
        }
        if let (Some(stop), Some(handler)) = (stop.upgrade(), handler.take()) {
            stop.disconnect(handler);
            stop.set_visible(false);
        }
        gtk::glib::ControlFlow::Break
    });
}
