use std::sync::Arc;

use bookie_app_services::result_filter::{FilterError, FilteredResult, filter_into_spool};
use bookie_app_services::result_predicate::ResultPredicate;
use bookie_app_services::result_scan::{ScanError, ScanSource};
use bookie_app_services::stream_result_spool::StreamResultSpoolError;
use relm4::ComponentSender;
use relm4::gtk;
use relm4::gtk::prelude::*;

use super::super::pinned::with_eviction_note;
use super::super::stream_pages::{StreamTerminal, StreamedResult};
use super::super::{SqlEditor, SqlEditorInput, StatementIdentity};
use super::derived::derived_outcome;
use super::jump::find_descendant;
use super::ticket::FilterTicket;
use crate::ui::count_format::format_thousands;

#[derive(Debug)]
pub(in crate::ui) struct FilterRequest {
    pub(in crate::ui) source: StreamedResult,
    pub(in crate::ui::editor) identity: StatementIdentity,
    pub(in crate::ui::editor) predicate: ResultPredicate,
    pub(in crate::ui::editor) label: String,
    pub(in crate::ui::editor) ticket: Arc<FilterTicket>,
}

pub(in crate::ui) struct FilterDone {
    request: FilterRequest,
    outcome: Result<FilteredResult, FilterError>,
}

impl std::fmt::Debug for FilterDone {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("filter result")
    }
}

pub(super) fn failure_message(error: &FilterError) -> String {
    match error {
        FilterError::Spool(StreamResultSpoolError::QuotaExceeded | StreamResultSpoolError::PageLimit) => {
            bookie_app_services::tr!("Not enough temporary storage; close results to free space")
        }
        FilterError::Scan(bookie_app_services::result_scan::ScanError::Cancelled) => {
            bookie_app_services::tr!("Filter cancelled")
        }
        FilterError::Scan(bookie_app_services::result_scan::ScanError::BudgetExceeded) => {
            bookie_app_services::tr!("Not enough memory to scan this result; close other large results and retry")
        }
        _ => bookie_app_services::tr!("The filter could not finish"),
    }
}

pub(super) fn success_message(rows: u64, unsearchable: u64, rendered: bool) -> String {
    let shown = if rendered {
        bookie_app_services::trn!(
            "Showing {n} matching row in a new result",
            "Showing {n} matching rows in a new result",
            rows as usize
        )
    } else {
        bookie_app_services::trn!(
            "{n} matching row pinned; it shows when the running query finishes",
            "{n} matching rows pinned; they show when the running query finishes",
            rows as usize
        )
    }
    .replace("{n}", &format_thousands(u128::from(rows)));
    if unsearchable == 0 {
        return shown;
    }
    let skipped = bookie_app_services::trn!(
        "{n} cell could not be searched",
        "{n} cells could not be searched",
        unsearchable as usize
    )
    .replace("{n}", &format_thousands(u128::from(unsearchable)));
    format!("{shown} · {skipped}")
}

pub(super) fn can_filter(source: &StreamedResult) -> bool {
    matches!(source.terminal, StreamTerminal::Complete)
}

pub(super) fn guarded<T>(work: impl FnOnce() -> Result<T, FilterError>) -> Result<T, FilterError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or(Err(FilterError::Scan(ScanError::Incomplete)))
}

impl SqlEditor {
    fn refuse_filter(&self, request: &FilterRequest, message: String) {
        self.status.set_label(&message);
        request.ticket.finish(message);
    }

    pub(in crate::ui::editor) fn on_filter_results(&mut self, request: FilterRequest, sender: &ComponentSender<Self>) {
        if !can_filter(&request.source) {
            self.refuse_filter(
                &request,
                bookie_app_services::tr!("Only a complete result can be filtered"),
            );
            return;
        }
        let Some(spool) = request.source.spool.clone() else {
            self.refuse_filter(&request, bookie_app_services::tr!("This result is no longer available"));
            return;
        };
        let manager = self.database.stream_result_spool_manager();
        let source = ScanSource::new(spool, request.source.columns.clone(), request.source.total_rows as u64);
        let input = sender.input_sender().clone();
        let ticket = request.ticket.clone();
        let spawned = std::thread::Builder::new().name("result-filter".into()).spawn(move || {
            let predicate = request.predicate.clone();
            let outcome = guarded(|| {
                filter_into_spool(
                    &source,
                    predicate,
                    &manager,
                    request.ticket.cancel(),
                    request.ticket.progress(),
                )
            });
            let _ = input.send(SqlEditorInput::FilterFinished(Box::new(FilterDone {
                request,
                outcome,
            })));
        });
        if spawned.is_err() {
            let message = bookie_app_services::tr!("The filter could not start");
            self.status.set_label(&message);
            ticket.finish(message);
        }
    }

    pub(in crate::ui::editor) fn on_filter_finished(&mut self, done: FilterDone, sender: &ComponentSender<Self>) {
        let FilterDone { request, outcome } = done;
        let message = match outcome {
            Ok(filtered) if filtered.total_rows == 0 => bookie_app_services::tr!("No rows match"),
            Ok(filtered) => self.show_derived_result(&request, filtered, sender),
            Err(error) => failure_message(&error),
        };
        self.status.set_label(&message);
        request.ticket.finish(message);
    }

    fn show_derived_result(
        &mut self,
        request: &FilterRequest,
        filtered: FilteredResult,
        sender: &ComponentSender<Self>,
    ) -> String {
        let (total, unsearchable) = (filtered.total_rows, filtered.unsearchable_cells);
        let outcome = derived_outcome(request.identity, &request.source, filtered, &request.label);
        let (added, rendered) = self.pin_and_show(outcome, sender);
        if rendered {
            select_result_tab(&self.results_holder, added.index);
        }
        with_eviction_note(success_message(total, unsearchable, rendered), added.evicted)
    }
}

fn select_result_tab(holder: &gtk::Box, index: usize) {
    let root = holder.clone().upcast::<gtk::Widget>();
    if let Some(stack) = find_descendant::<relm4::adw::ViewStack>(&root, |widget| widget.is::<relm4::adw::ViewStack>())
    {
        stack.set_visible_child_name(&format!("r{index}"));
    }
}
