use std::sync::Arc;

use bookie_app_services::result_predicate::ResultPredicate;
use bookie_core::{ColumnInfo, Value};
use relm4::ComponentSender;
use sha2::{Digest, Sha256};

use super::super::stream_pages::StreamedResult;
use super::super::{SqlEditor, SqlEditorInput, StatementIdentity, StatementOutcome, StatementOutcomeKind};
use super::actions::filtering_text;
use super::editor::FilterRequest;
use super::ticket::{FilterTicket, register, running};
use super::widget::tools_for;
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

pub(in crate::ui::editor) fn stream_identity(identity: StatementIdentity, generation: u64) -> StatementIdentity {
    let mut digest = Sha256::new();
    digest.update(identity.digest);
    digest.update(generation.to_be_bytes());
    StatementIdentity {
        statement_index: identity.statement_index,
        digest: digest.finalize().into(),
    }
}

pub(super) fn find_streamed<'a>(
    mut outcomes: impl Iterator<Item = &'a StatementOutcome>,
    key: StatementIdentity,
) -> Option<StreamedResult> {
    outcomes.find_map(|outcome| match &outcome.kind {
        StatementOutcomeKind::Streamed(result) if stream_identity(outcome.identity, result.generation) == key => {
            Some((**result).clone())
        }
        _ => None,
    })
}

impl SqlEditor {
    fn streamed_result_for(&self, identity: StatementIdentity) -> Option<StreamedResult> {
        find_streamed(self.pinned.iter(), identity)
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
            self.status
                .set_label(&bookie_app_services::tr!("This result is no longer available"));
            return;
        };
        let Some((predicate, label)) = cell_predicate(&source.columns, column, value, exclude) else {
            self.status.set_label(&bookie_app_services::tr!(
                "Values of this type can't be used as a filter"
            ));
            return;
        };
        if running(identity) {
            self.status
                .set_label(&bookie_app_services::tr!("A filter is already running on this result"));
            return;
        }
        let ticket = Arc::new(FilterTicket::default());
        register(identity, &ticket);
        self.status.set_label(&filtering_text(0, source.total_rows as u64));
        if let Some(tools) = tools_for(identity) {
            tools.adopt_filter(ticket.clone());
        }
        sender.input(SqlEditorInput::FilterResults(Box::new(FilterRequest {
            source,
            identity,
            predicate,
            label,
            ticket,
        })));
    }
}
