use bookie_app_services::value_display::copy_text;
use bookie_app_services::value_search::{SearchOptions, SearchReport, TableOutcome};
use bookie_core::SearchMode;
use bookie_gpui::messages::MessageKind;

use super::*;
use crate::chrome::ghost;

fn first_match(outcome: &TableOutcome) -> Option<String> {
    let hit = outcome.hits.first()?;
    let cells: Vec<String> = hit
        .shown
        .iter()
        .map(|(column, value)| format!("{column} = {}", copy_text(value).into_string()))
        .collect();
    Some(format!("    first match: {}", cells.join(", ")))
}

pub(crate) fn summarize(report: &SearchReport, term: &str) -> Vec<(MessageKind, String)> {
    let mut lines = Vec::new();
    for outcome in report.with_hits() {
        let count = outcome.hits.len();
        let more = if outcome.more_rows_exist { "at least " } else { "" };
        lines.push((
            MessageKind::Success,
            format!(
                "{}: {more}{count} matching row(s) in {}",
                outcome.target.label(),
                outcome.matched_columns.join(", ")
            ),
        ));
        lines.extend(first_match(outcome).map(|line| (MessageKind::Info, line)));
    }
    if lines.is_empty() {
        lines.push((
            MessageKind::Info,
            format!("No match for \"{term}\" in {} tables", report.tables.len()),
        ));
    }
    let notes = report.notes();
    let stopped = report.stopped.is_some();
    lines.extend(notes.into_iter().map(|note| {
        let kind = if stopped && note.starts_with("The search") {
            MessageKind::Error
        } else {
            MessageKind::Info
        };
        (kind, note)
    }));
    lines
}

#[derive(Default)]
pub(crate) struct ValueSearchState {
    pub open: bool,
    pub focus: bool,
    pub exact: bool,
    pub input: Option<Entity<TextInput>>,
}

impl GridView {
    pub(crate) fn value_search_actions(&self) -> Vec<Action> {
        let enabled = self.live.as_ref().is_some_and(|session| !session.tables.is_empty());
        vec![Action {
            id: "search-values",
            label: "Search database values…".to_string(),
            enabled,
        }]
    }

    pub(crate) fn focus_value_search_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.search.values.focus) {
            return;
        }
        if let Some(input) = &self.search.values.input {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
    }

    pub(crate) fn open_value_search_bar(&mut self, cx: &mut Context<Self>) {
        if self.live.is_none() {
            self.status = "Connect to a database to search its values".into();
            return;
        }
        let input = self.search.values.input.get_or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new("Text or value to look for in every table", cx));
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submit) {
                    this.run_value_search(cx);
                }
            })
            .detach();
            input
        });
        input.update(cx, |input, cx| input.set_content("", cx));
        self.search.values.open = true;
        self.search.values.focus = true;
        self.status = "Type a value, then press Enter to search every table".into();
        cx.notify();
    }

    pub(crate) fn run_value_search(&mut self, cx: &mut Context<Self>) {
        let (Some(session), Some(input)) = (self.live.clone(), self.search.values.input.clone()) else {
            return;
        };
        let term = input.read(cx).content().trim().to_string();
        let mode = if self.search.values.exact {
            SearchMode::Exact
        } else {
            SearchMode::Contains
        };
        let options = SearchOptions {
            mode,
            include_non_text: false,
        };
        self.search.values.open = false;
        self.tabs.open_query();
        self.sync_grid_owner();
        self.status = format!("Searching every table for \"{term}\"…");
        let searched = term.clone();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| session.search_values(&term, options, &cancel),
            move |this, result| match result {
                Ok(report) => {
                    for (kind, line) in summarize(&report, &searched).into_iter().rev() {
                        this.panel.messages.push(kind, line);
                    }
                    this.panel.view = ResultView::Messages;
                    this.panel.open = true;
                    this.status = format!("Searched {} tables for \"{searched}\"", report.tables.len());
                }
                Err(message) => this.status = format!("Could not search: {message}"),
            },
        );
    }

    pub(crate) fn late_bars(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        self.export_file_bar(theme, cx)
            .or_else(|| self.set_cells_bar(theme, cx))
            .or_else(|| self.value_search_bar(theme, cx))
            .or_else(|| self.schema_diagram_bar(theme, cx))
    }

    pub(crate) fn value_search_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.search.values.input.clone()?;
        if !self.search.values.open {
            return None;
        }
        let mode = if self.search.values.exact {
            "Exact match"
        } else {
            "Contains"
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .p_2()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child("Search database values"),
                )
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    ghost("value-search-mode", mode, theme).on_click(cx.listener(|this, _, _, cx| {
                        this.search.values.exact = !this.search.values.exact;
                        cx.notify();
                    })),
                )
                .child(
                    ghost("run-value-search", "Search", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.run_value_search(cx))),
                )
                .child(
                    ghost("cancel-value-search", "Cancel", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.search.values.open = false;
                        cx.notify();
                    })),
                ),
        )
    }
}

#[cfg(test)]
#[path = "value_search_ui_tests.rs"]
mod tests;
