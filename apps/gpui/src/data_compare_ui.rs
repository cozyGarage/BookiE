use bookie_app_services::data_compare::{DataCompare, DataCompareError};
use bookie_app_services::driver_error_presentation::classify;
use bookie_app_services::driver_failure_message::driver_failure_message;
use bookie_core::{DriverError, TableInfo};
use gpui_component::checkbox::Checkbox;

use super::*;
use crate::palette_state::PaletteMode;
use crate::surfaces::{ButtonKind, ListState, button, list_state, note, section_header, task_bar, unavailable_button};

const SHOWN_SAMPLES: usize = 20;

#[derive(Default)]
pub(crate) struct DataCompareState {
    pub request: crate::Ticket,
    pub open: bool,
    pub source: Option<TableInfo>,
    pub target: Option<TableInfo>,
    pub outcome: Option<Result<DataCompare, String>>,
    pub include_deletes: bool,
}

pub(crate) fn failure_message(error: &DataCompareError) -> String {
    match error {
        DataCompareError::NoKey => {
            "Both tables need the same primary key to compare rows; add one and compare again".into()
        }
        DataCompareError::BadKey(column) => {
            format!("Key column {column} must be NOT NULL and have the same type in both tables")
        }
        DataCompareError::Unsupported => "This database does not support data compare".into(),
        DataCompareError::Driver(DriverError::Cancelled) => "Canceled the data compare".into(),
        DataCompareError::Driver(error) => driver_failure_message(&classify(error)),
        other => format!("Could not compare the tables: {other}"),
    }
}

pub(crate) fn summary_lines(result: &DataCompare, source: &str, target: &str) -> Vec<String> {
    let counts = result.counts;
    let mut lines = vec![
        format!("Only in {source}: {}", counts.insert),
        format!("Only in {target}: {}", counts.delete),
        format!("Changed: {}", counts.update),
        format!("Same: {}", counts.same),
    ];
    if !result.layout.not_compared.is_empty() {
        lines.push(format!("Not compared: {}", result.layout.not_compared.join(", ")));
    }
    if result.unknown_cells > 0 {
        lines.push(format!("{} values could not be compared", result.unknown_cells));
    }
    lines
}

impl GridView {
    pub(crate) fn data_compare_actions(&self) -> Vec<Action> {
        let enabled = self.live.is_some() && matches!(self.tabs.current(), Tab::Table(_));
        vec![Action {
            id: "compare-table-data",
            label: "Compare table data…".to_string(),
            enabled,
        }]
    }

    pub(crate) fn open_data_compare_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Tab::Table(source) = self.tabs.current().clone() else {
            self.status = "Open a table to compare its data".into();
            return;
        };
        if self.live.is_none() {
            self.status = "Connect to a database to compare table data".into();
            return;
        }
        self.search.data_compare.source = Some(source);
        self.open_picker(PaletteMode::CompareTarget, window, cx);
    }

    pub(crate) fn data_compare_targets(&self) -> Vec<TableInfo> {
        let source = self.search.data_compare.source.as_ref();
        self.catalog
            .tables
            .iter()
            .filter(|table| Some(*table) != source)
            .cloned()
            .collect()
    }

    pub(crate) fn start_data_compare(&mut self, target: TableInfo, cx: &mut Context<Self>) {
        let (Some(session), Some(source)) = (self.live.clone(), self.search.data_compare.source.clone()) else {
            return;
        };
        let state = &mut self.search.data_compare;
        state.request.invalidate();
        state.request.cancel = CancellationToken::new();
        state.open = true;
        state.outcome = None;
        state.include_deletes = false;
        state.target = Some(target.clone());
        let (generation, cancel) = (state.request.generation, state.request.cancel.clone());
        self.status = "Comparing table data…".into();
        cx.spawn(async move |this, cx| {
            let live = session.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { live.compare_table_data(&source, &target, &cancel) })
                .await
                .map_err(|error| failure_message(&error));
            let _ = this.update(cx, |this, cx| {
                this.finish_data_compare(&session, generation, outcome, cx)
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn finish_data_compare(
        &mut self,
        session: &std::sync::Arc<bookie_gpui::live::LiveSession>,
        generation: u64,
        outcome: Result<DataCompare, String>,
        cx: &mut Context<Self>,
    ) {
        if !self.is_current_session(session) || self.search.data_compare.request.generation != generation {
            return;
        }
        self.status = match &outcome {
            Ok(result) if result.is_identical() => "The tables hold the same rows".into(),
            Ok(_) => "Data compare finished".into(),
            Err(message) => message.clone(),
        };
        self.search.data_compare.outcome = Some(outcome);
        cx.notify();
    }

    pub(crate) fn close_data_compare(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.search.data_compare;
        state.request.invalidate();
        state.open = false;
        state.outcome = None;
        cx.notify();
    }

    pub(crate) fn data_compare_script(&self) -> Result<String, String> {
        let state = &self.search.data_compare;
        let (Some(Ok(result)), Some(target), Some(session)) = (&state.outcome, &state.target, &self.live) else {
            return Err("Run a data compare first".into());
        };
        result
            .script(
                &session.driver_id,
                target.schema.as_deref(),
                &target.name,
                state.include_deletes,
            )
            .map_err(|error| failure_message(&error))
    }

    pub(crate) fn copy_data_compare_script(&mut self, cx: &mut Context<Self>) {
        match self.data_compare_script() {
            Ok(script) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(script));
                self.status = "Copied the sync script; review it before running".into();
            }
            Err(message) => self.status = message,
        }
        cx.notify();
    }

    pub(crate) fn data_compare_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let state = &self.search.data_compare;
        if !state.open {
            return None;
        }
        let (source, target) = (state.source.as_ref()?, state.target.as_ref()?);
        let copy = if matches!(state.outcome, Some(Ok(_))) {
            button(
                ButtonKind::Primary,
                "copy-data-compare-script",
                "Copy sync script",
                theme,
            )
            .on_click(cx.listener(|this, _, _, cx| this.copy_data_compare_script(cx)))
        } else {
            unavailable_button(
                ButtonKind::Primary,
                "copy-data-compare-script",
                "Copy sync script",
                "Available when the compare finishes",
                theme,
            )
        };
        let bar = task_bar("Compare table data", theme)
            .child(div().flex_1().min_w_0())
            .child(copy)
            .child(
                button(ButtonKind::Ghost, "cancel-data-compare", "Cancel", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.close_data_compare(cx))),
            );
        let (source, target) = (table_label(source), table_label(target));
        Some(
            div()
                .flex()
                .flex_col()
                .child(bar)
                .child(self.data_compare_body(&source, &target, theme, cx)),
        )
    }

    fn data_compare_body(&self, source: &str, target: &str, theme: Theme, cx: &mut Context<Self>) -> Div {
        let state = &self.search.data_compare;
        let body = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(note(format!("Changes {target} to match {source}"), theme));
        let result = match &state.outcome {
            None => return body.child(list_state(ListState::Loading("Comparing rows…"), theme)),
            Some(Err(message)) => return body.child(list_state(ListState::Failed(message), theme)),
            Some(Ok(result)) => result,
        };
        let driver = self.live.as_ref().map_or("", |session| session.driver_id.as_str());
        let samples = result.sample_lines(driver).into_iter().take(SHOWN_SAMPLES);
        body.children(
            summary_lines(result, source, target)
                .into_iter()
                .map(|line| div().child(line)),
        )
        .when(result.counts.delete > 0, |body| {
            body.child(
                Checkbox::new("data-compare-deletes")
                    .label("Include deletes in the script")
                    .checked(state.include_deletes)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.search.data_compare.include_deletes = *checked;
                        cx.notify();
                    })),
            )
        })
        .when(!result.sample.is_empty(), |body| {
            body.child(section_header("Sample", theme))
                .children(samples.map(|line| div().child(line)))
        })
    }
}

#[cfg(test)]
#[path = "data_compare_ui_tests.rs"]
mod tests;
