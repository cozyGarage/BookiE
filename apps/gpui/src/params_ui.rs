use std::collections::HashMap;

use bookie_app_services::query_parameters::{bind_statement, collect_values, statement_names};
use bookie_core::ParameterKind;

use super::*;
use crate::chrome::tip;
use crate::surfaces::{ButtonKind, button, task_bar};

pub(crate) struct ParamRow {
    pub name: String,
    pub input: Entity<TextInput>,
    pub kind: ParameterKind,
}

#[derive(Default)]
pub(crate) struct ParamsState {
    pub rows: Vec<ParamRow>,
    pub sql: String,
    source: Option<(String, std::ops::Range<usize>)>,
    pub remembered: HashMap<String, String>,
}

impl GridView {
    pub(crate) fn run_with_parameters(
        &mut self,
        sql: String,
        source: Option<(String, std::ops::Range<usize>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(driver_id) = self.live.as_ref().map(|live| live.driver_id.clone()) else {
            self.run_sql_with_source(sql, Vec::new(), source, cx);
            return;
        };
        let names = statement_names(&sql, &driver_id);
        if names.is_empty() {
            self.params.rows.clear();
            self.run_sql_with_source(sql, Vec::new(), source, cx);
            return;
        }
        let waiting = self.params.sql == sql
            && self.params.source == source
            && self.params.rows.iter().map(|row| &row.name).eq(names.iter());
        if !waiting {
            self.params.source = source;
            self.ask_for_parameters(sql, names, cx);
            return;
        }
        self.submit_parameters(cx);
    }

    fn submit_parameters(&mut self, cx: &mut Context<Self>) {
        let Some(driver_id) = self.live.as_ref().map(|live| live.driver_id.clone()) else {
            return;
        };
        if self.params.rows.is_empty() {
            return;
        }
        let sql = self.params.sql.clone();
        let source = self.params.source.clone();
        let entries: Vec<_> = self
            .params
            .rows
            .iter()
            .map(|row| {
                let text = row.input.read(cx).content().to_string();
                (row.name.clone(), row.kind, text)
            })
            .collect();
        for (name, _, text) in &entries {
            self.params.remembered.insert(name.clone(), text.clone());
        }
        let bound = collect_values(&entries).and_then(|values| bind_statement(&sql, &driver_id, &values));
        match bound {
            Ok(bound) => {
                let parsed = bookie_core::extract_named_parameters(&sql, &driver_id);
                self.run_sql_with_mapping(bound.sql, bound.values, source, Some((sql, parsed)), cx);
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn ask_for_parameters(&mut self, sql: String, names: Vec<String>, cx: &mut Context<Self>) {
        let rows = names
            .into_iter()
            .map(|name| {
                let input = cx.new(|cx| TextInput::new("value", cx));
                if let Some(text) = self.params.remembered.get(&name) {
                    input.update(cx, |input, cx| input.set_content(text, cx));
                }
                cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Submit) {
                        this.submit_parameters(cx);
                    }
                    cx.notify();
                })
                .detach();
                ParamRow {
                    name,
                    input,
                    kind: ParameterKind::Auto,
                }
            })
            .collect();
        self.params.rows = rows;
        self.params.sql = sql;
        self.status = "Fill in the query parameters, then run again".into();
    }

    pub(crate) fn cycle_param_kind(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.params.rows.get_mut(index) else {
            return;
        };
        let next = (row.kind.index() as usize + 1) % ParameterKind::ALL.len();
        row.kind = ParameterKind::from_index(next as u32);
        cx.notify();
    }

    pub(crate) fn parameters_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        if self.params.rows.is_empty() {
            return None;
        }
        let fields: Vec<Div> = self
            .params
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_color(rgb(theme.muted_foreground))
                            .child(format!(":{}", row.name)),
                    )
                    .child(
                        button(ButtonKind::Ghost, ("param-kind", index), row.kind.label(), theme)
                            .tooltip(tip("Change how this value is typed"))
                            .on_click(cx.listener(move |this, _, _, cx| this.cycle_param_kind(index, cx))),
                    )
                    .child(div().w(px(size::FORM_CONTROL * 4.0)).child(row.input.clone()))
            })
            .collect();
        Some(
            task_bar("Parameters", theme).flex_wrap().children(fields).child(
                div()
                    .text_size(px(size::TEXT_SMALL))
                    .text_color(rgb(theme.muted_foreground))
                    .child(
                        "Auto reads numbers and true/false; choose Text to keep values like 007. Press Enter to submit the pending statement.",
                    ),
            ),
        )
    }
}
