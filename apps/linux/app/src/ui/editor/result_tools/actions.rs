use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bookie_app_services::request_generation::replace_in_flight;
use bookie_app_services::result_find::{FindError, FindReport, find_matches};
use bookie_app_services::result_predicate::{ColumnScope, ResultPredicate};
use bookie_app_services::result_scan::{ScanError, ScanProgress};
use bookie_app_services::text_match::TextMatchOptions;
use relm4::gtk;
use relm4::gtk::glib::WeakRef;
use relm4::gtk::prelude::*;

use super::cursor::SearchSpec;
use super::editor::FilterRequest;
use super::jump::{JumpTarget, go_to};
use super::ticket::FilterTicket;
use super::widget::Tools;
use crate::ui::count_format::format_thousands;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

fn with<T: IsA<gtk::glib::Object>>(weak: &WeakRef<T>, apply: impl FnOnce(&T)) {
    if let Some(widget) = weak.upgrade() {
        apply(&widget);
    }
}

pub(super) fn connect(
    tools: &Rc<Tools>,
    row: &gtk::Box,
    entry: &gtk::SearchEntry,
    previous: &gtk::Button,
    next: &gtk::Button,
    filter: &gtk::Button,
    stop: &gtk::Button,
    match_case: &gtk::ToggleButton,
    whole_words: &gtk::ToggleButton,
    regex: &gtk::ToggleButton,
    columns: &gtk::DropDown,
) {
    entry.connect_activate({
        let tools = tools.clone();
        move |_| tools.search(true)
    });
    entry.connect_search_changed({
        let tools = tools.clone();
        move |_| tools.entry_changed()
    });
    previous.connect_clicked({
        let tools = tools.clone();
        move |_| tools.search(false)
    });
    next.connect_clicked({
        let tools = tools.clone();
        move |_| tools.search(true)
    });
    filter.connect_clicked({
        let tools = tools.clone();
        move |_| tools.filter()
    });
    stop.connect_clicked({
        let tools = tools.clone();
        move |_| tools.stop()
    });
    for toggle in [match_case, whole_words, regex] {
        toggle.connect_toggled({
            let tools = tools.clone();
            move |_| tools.entry_changed()
        });
    }
    columns.connect_selected_notify({
        let tools = tools.clone();
        move |_| tools.entry_changed()
    });
    row.connect_destroy({
        let tools = tools.clone();
        move |_| tools.cancel_search()
    });
}

impl Tools {
    fn search_spec(&self) -> SearchSpec {
        let needle = self
            .controls
            .entry
            .upgrade()
            .map(|entry| entry.text().to_string())
            .unwrap_or_default();
        let options = TextMatchOptions {
            match_case: self
                .controls
                .match_case
                .upgrade()
                .is_some_and(|toggle| toggle.is_active()),
            whole_words: self
                .controls
                .whole_words
                .upgrade()
                .is_some_and(|toggle| toggle.is_active()),
            regex: self.controls.regex.upgrade().is_some_and(|toggle| toggle.is_active()),
        };
        let selected = self.controls.columns.upgrade().map(|columns| columns.selected()).unwrap_or(0);
        let column = (selected > 0).then(|| selected as usize - 1);
        SearchSpec {
            needle,
            options,
            column,
        }
    }

    pub(super) fn set_status(&self, text: &str) {
        with(&self.controls.status, |label| label.set_label(text));
    }

    fn set_busy(&self, busy: bool) {
        self.busy.set(busy);
        for button in [&self.controls.previous, &self.controls.next, &self.controls.filter] {
            with(button, |button| button.set_sensitive(!busy));
        }
        with(&self.controls.stop, |button| button.set_visible(busy));
    }

    fn cancel_search(&self) {
        self.cancel.borrow().cancel();
    }

    fn stop(&self) {
        self.cancel_search();
        if let Some(ticket) = self.filter_ticket.borrow().as_ref() {
            ticket.cancel().cancel();
        }
        super::ticket::cancel(self.identity);
    }

    fn filter_running(&self) -> bool {
        self.filter_ticket
            .borrow()
            .as_ref()
            .is_some_and(|ticket| !ticket.is_finished())
            || super::ticket::running(self.identity)
    }

    fn entry_changed(&self) {
        let spec = self.search_spec();
        if self.cursor.borrow().keeps_for(&spec) {
            return;
        }
        self.cancel.borrow().cancel();
        self.cursor.borrow_mut().invalidate();
        if self.filter_running() {
            return;
        }
        self.set_busy(false);
        self.set_status("");
    }

    pub(super) fn search(self: &Rc<Self>, forward: bool) {
        if self.filter_running() {
            return;
        }
        let spec = self.search_spec();
        if spec.needle.is_empty() {
            self.set_status("");
            return;
        }
        if self.cursor.borrow().has_results_for(&spec) {
            self.step(forward);
            return;
        }
        if self.cursor.borrow().is_searching_for(&spec) {
            self.cursor.borrow_mut().request_direction(forward);
            return;
        }
        self.start_search(spec, forward);
    }

    fn start_search(self: &Rc<Self>, spec: SearchSpec, forward: bool) {
        let cancel = replace_in_flight(&self.cancel);
        let generation = self.cursor.borrow_mut().begin(spec.clone());
        let progress = ScanProgress::default();
        let (send, receive) = async_channel::bounded(1);
        let worker = {
            let source = self.source.clone();
            let progress = progress.clone();
            std::thread::Builder::new().name("result-find".into()).spawn(move || {
                let scope = spec
                    .column
                    .map_or(ColumnScope::Any, ColumnScope::Column);
                let predicate = ResultPredicate::Contains {
                    scope,
                    needle: spec.needle,
                    options: spec.options,
                };
                let _ = send.send_blocking(find_matches(&source, predicate, &cancel, &progress));
            })
        };
        if worker.is_err() {
            self.cursor.borrow_mut().invalidate();
            self.set_status(&crate::tr!("Could not start the search"));
            return;
        }
        self.set_busy(true);
        self.poll_search(generation, progress);
        let tools = Rc::downgrade(self);
        gtk::glib::spawn_future_local(async move {
            let result = received(receive.recv().await);
            if let Some(tools) = tools.upgrade() {
                tools.finish_search(generation, result, forward);
            }
        });
    }

    fn poll_search(self: &Rc<Self>, generation: u64, progress: ScanProgress) {
        let tools = Rc::downgrade(self);
        let total = self.source.expected_rows();
        gtk::glib::timeout_add_local(POLL_INTERVAL, move || {
            let Some(tools) = tools.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            if !tools.busy.get() || !tools.cursor.borrow().accepts(generation) {
                return gtk::glib::ControlFlow::Break;
            }
            tools.set_status(&searching_text(progress.rows_done(), total));
            gtk::glib::ControlFlow::Continue
        });
    }

    fn finish_search(self: &Rc<Self>, generation: u64, result: Result<FindReport, FindError>, forward: bool) {
        if !self.cursor.borrow().accepts(generation) {
            return;
        }
        if self.filter_running() {
            self.cursor.borrow_mut().end_search(generation);
            return;
        }
        self.set_busy(false);
        match result {
            Ok(report) => {
                let forward = self.cursor.borrow_mut().take_direction().unwrap_or(forward);
                self.cursor.borrow_mut().complete(generation, report);
                self.step(forward);
            }
            Err(FindError::Scan(ScanError::Cancelled)) => {
                self.cursor.borrow_mut().end_search(generation);
                self.set_status(&crate::tr!("Search cancelled"));
            }
            Err(FindError::Predicate(_)) => {
                self.cursor.borrow_mut().end_search(generation);
                self.set_status(&crate::tr!("Invalid regular expression"));
            }
            Err(_) => {
                self.cursor.borrow_mut().end_search(generation);
                self.set_status(&crate::tr!("The search could not finish"));
            }
        }
    }

    fn step(&self, forward: bool) {
        let found = self.cursor.borrow_mut().step(forward);
        let mut status = self.cursor.borrow().status();
        let status_text = status.clone();
        if let Some(found) = found
            && let Some(target) = self.target_for(found.page, found.row)
        {
            if let (Some(holder), Some(pages)) = (self.holder.upgrade(), self.pages.upgrade()) {
                let status = self.controls.status.clone();
                let base = status_text.clone();
                let failed: Rc<dyn Fn()> = Rc::new(move || {
                    with(&status, |label| {
                        label.set_label(&crate::tr!("{status} · could not show this row").replace("{status}", &base));
                    });
                });
                go_to(&holder, &pages, target, &self.jump, failed);
            }
            status = crate::tr!("{status} · row {row}")
                .replace("{status}", &status)
                .replace("{row}", &format_thousands(target.global_row as u128 + 1));
        }
        self.set_status(&status);
    }

    fn target_for(&self, page: usize, row: usize) -> Option<JumpTarget> {
        let start = *self.result.page_row_starts.get(page)?;
        let page_rows = *self.result.page_row_counts.get(page)?;
        Some(JumpTarget {
            generation: self.result.generation,
            page,
            row,
            global_row: start.saturating_sub(1).saturating_add(row),
            page_rows,
        })
    }

    fn filter(self: &Rc<Self>) {
        if self.filter_running() {
            return;
        }
        let spec = self.search_spec();
        if spec.needle.is_empty() {
            self.set_status(&crate::tr!("Enter text to filter by"));
            return;
        }
        let ticket = Arc::new(FilterTicket::default());
        super::ticket::register(self.identity, &ticket);
        self.adopt_filter(ticket.clone());
        let scope = spec.column.map_or(ColumnScope::Any, ColumnScope::Column);
        let sent = self
            .sender
            .send(super::super::SqlEditorInput::FilterResults(Box::new(FilterRequest {
                source: self.result.clone(),
                identity: self.identity,
                predicate: ResultPredicate::Contains {
                    scope,
                    needle: spec.needle.clone(),
                    options: spec.options,
                },
                label: spec.needle,
                ticket: ticket.clone(),
            })));
        if sent.is_err() {
            ticket.finish(crate::tr!("The filter could not start"));
        }
    }

    pub(super) fn adopt_filter(self: &Rc<Self>, ticket: Arc<FilterTicket>) {
        self.cancel_search();
        self.cursor.borrow_mut().invalidate();
        *self.filter_ticket.borrow_mut() = Some(ticket.clone());
        self.set_busy(true);
        self.poll_filter(ticket);
    }

    fn poll_filter(self: &Rc<Self>, ticket: Arc<FilterTicket>) {
        let tools = Rc::downgrade(self);
        let total = self.source.expected_rows();
        gtk::glib::timeout_add_local(POLL_INTERVAL, move || {
            let Some(tools) = tools.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            if ticket.is_finished() {
                tools.set_busy(false);
                tools.set_status(&ticket.message());
                return gtk::glib::ControlFlow::Break;
            }
            tools.set_status(&filtering_text(ticket.progress().rows_done(), total));
            gtk::glib::ControlFlow::Continue
        });
    }
}

pub(super) fn received(
    result: Result<Result<FindReport, FindError>, async_channel::RecvError>,
) -> Result<FindReport, FindError> {
    result.unwrap_or(Err(FindError::Scan(ScanError::Incomplete)))
}

fn searching_text(done: u64, total: u64) -> String {
    crate::tr!("Searching… {done} of {total} rows")
        .replace("{done}", &format_thousands(u128::from(done)))
        .replace("{total}", &format_thousands(u128::from(total)))
}

pub(super) fn filtering_text(done: u64, total: u64) -> String {
    crate::tr!("Filtering… {done} of {total} rows")
        .replace("{done}", &format_thousands(u128::from(done)))
        .replace("{total}", &format_thousands(u128::from(total)))
}
