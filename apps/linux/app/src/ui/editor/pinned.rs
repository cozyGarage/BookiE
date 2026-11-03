use super::stream_pages::StreamTerminal;
use super::{SqlEditor, StatementOutcome, StatementOutcomeKind, clear_results_box, render_outcomes};
use relm4::gtk;
use relm4::gtk::prelude::*;

pub(super) const MAX_PINNED: usize = 4;

fn has_grid_result(outcome: &StatementOutcome) -> bool {
    match &outcome.kind {
        StatementOutcomeKind::Rows(_) => true,
        StatementOutcomeKind::Streamed(result) => !matches!(result.terminal, StreamTerminal::Loading),
        _ => false,
    }
}

pub(super) struct AddedPin {
    pub(super) index: usize,
    pub(super) evicted: usize,
}

#[derive(Default)]
pub(super) struct PinnedResults {
    pins: Vec<StatementOutcome>,
    last: Vec<StatementOutcome>,
}

impl PinnedResults {
    pub(super) fn latest(&self) -> &[StatementOutcome] {
        &self.last
    }

    pub(super) fn is_pinned(&self) -> bool {
        !self.pins.is_empty()
    }

    pub(super) fn count(&self) -> usize {
        self.pins.len()
    }

    pub(super) fn has_results(&self) -> bool {
        self.last.iter().any(has_grid_result)
    }

    pub(super) fn set_latest(&mut self, outcome: StatementOutcome) {
        self.last = vec![outcome];
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &StatementOutcome> {
        self.pins.iter().chain(self.last.iter())
    }

    pub(super) fn displayed(&self) -> Vec<StatementOutcome> {
        let mut shown = self.pins.clone();
        shown.extend(self.last.iter().cloned());
        shown
    }

    pub(super) fn show(&mut self, fresh: Vec<StatementOutcome>) -> Vec<StatementOutcome> {
        self.last = fresh;
        self.displayed()
    }

    pub(super) fn pin_last(&mut self) -> usize {
        let pinnable = self
            .last
            .iter()
            .filter(|outcome| has_grid_result(outcome))
            .cloned()
            .map(|outcome| StatementOutcome {
                pinned: true,
                ..outcome
            });
        self.pins.extend(pinnable);
        let excess = self.pins.len().saturating_sub(MAX_PINNED);
        self.pins.drain(..excess);
        self.pins.len()
    }

    pub(super) fn add_pin(&mut self, outcome: StatementOutcome) -> AddedPin {
        self.pins.push(StatementOutcome {
            pinned: true,
            ..outcome
        });
        let evicted = self.pins.len().saturating_sub(MAX_PINNED);
        self.pins.drain(..evicted);
        AddedPin {
            index: self.pins.len() - 1,
            evicted,
        }
    }

    pub(super) fn unpin_all(&mut self) -> Vec<StatementOutcome> {
        self.pins.clear();
        self.last.clone()
    }
}

impl SqlEditor {
    pub(super) fn refresh_pin_button(&self) {
        let label = if self.pinned.is_pinned() {
            crate::tr!("Unpin results ({n})").replace("{n}", &self.pinned.count().to_string())
        } else {
            crate::tr!("Pin results")
        };
        self.pin_button.set_label(&label);
        self.pin_button
            .set_sensitive(self.pinned.is_pinned() || self.pinned.has_results());
        self.compare_button
            .set_sensitive(super::result_compare::can_compare(self.pinned.iter()));
        self.refresh_result_exports();
        self.panel.refresh_errors(self.pinned.latest());
    }

    pub(super) fn pin_and_show(
        &mut self,
        outcome: StatementOutcome,
        sender: &relm4::ComponentSender<Self>,
    ) -> (AddedPin, bool) {
        let added = self.pinned.add_pin(outcome);
        let rendered = self.run_generation.active.is_empty();
        if rendered {
            clear_results_box(&self.results_holder);
            render_outcomes(
                &self.results_holder,
                &self.pinned.displayed(),
                &self.grid_sender,
                self.connection_id,
                self.database.clone(),
                self.result_view_settings.clone(),
                Some(sender),
            );
        }
        self.refresh_pin_button();
        (added, rendered)
    }

    pub(super) fn show_pins_above(&mut self, page: &impl IsA<gtk::Widget>, sender: &relm4::ComponentSender<Self>) {
        clear_results_box(&self.results_holder);
        let pins = self.pinned.show(Vec::new());
        if !pins.is_empty() {
            render_outcomes(
                &self.results_holder,
                &pins,
                &self.grid_sender,
                self.connection_id,
                self.database.clone(),
                self.result_view_settings.clone(),
                Some(sender),
            );
        }
        self.results_holder.append(page);
        self.refresh_pin_button();
    }

    pub(super) fn toggle_pin(&mut self, sender: &relm4::ComponentSender<Self>) {
        if self.pinned.is_pinned() {
            let remaining = self.pinned.unpin_all();
            self.refresh_pin_button();
            clear_results_box(&self.results_holder);
            render_outcomes(
                &self.results_holder,
                &remaining,
                &self.grid_sender,
                self.connection_id,
                self.database.clone(),
                self.result_view_settings.clone(),
                Some(sender),
            );
            return;
        }
        let kept = self.pinned.pin_last();
        self.refresh_pin_button();
        let kept = kept.to_string();
        self.status
            .set_label(&crate::tr!("{n} pinned; they stay above the next run").replace("{n}", &kept));
    }
}

pub(super) fn with_pending_note(message: String, rendered: bool) -> String {
    if rendered {
        return message;
    }
    format!(
        "{message} · {}",
        crate::tr!("Pinned; it shows when the running query finishes")
    )
}

pub(super) fn with_eviction_note(message: String, evicted: usize) -> String {
    if evicted == 0 {
        return message;
    }
    format!("{message} · {}", crate::tr!("The oldest pinned result was closed"))
}

#[cfg(test)]
#[path = "../../../tests/support/pinned_tests.rs"]
mod tests;
