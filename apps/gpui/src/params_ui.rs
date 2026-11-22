use std::collections::HashMap;

use bookie_app_services::query_parameters::{bind_statement, collect_values, statement_names};
use bookie_core::ParameterKind;

use super::*;
use crate::chrome::{ghost, tip};

pub(crate) struct ParamRow {
    pub name: String,
    pub input: Entity<TextInput>,
    pub kind: ParameterKind,
}

#[derive(Default)]
pub(crate) struct ParamsState {
    pub rows: Vec<ParamRow>,
    pub sql: String,
    pub remembered: HashMap<String, String>,
}

impl GridView {
    pub(crate) fn run_with_parameters(&mut self, sql: String, cx: &mut Context<Self>) {
        let Some(driver_id) = self.live.as_ref().map(|live| live.driver_id.clone()) else {
            self.run_sql(sql, cx);
            return;
        };
        let names = statement_names(&sql, &driver_id);
        if names.is_empty() {
            self.params.rows.clear();
            self.run_sql(sql, cx);
            return;
        }
        let waiting = self.params.sql == sql && self.params.rows.iter().map(|row| &row.name).eq(names.iter());
        if !waiting {
            self.ask_for_parameters(sql, names, cx);
            return;
        }
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
                let editor = self.sql_input.read(cx).value().to_string();
                let source = (editor == sql).then(|| (editor.clone(), 0..editor.len()));
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
                        this.run_query(cx);
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
                        ghost(("param-kind", index), row.kind.label(), theme)
                            .tooltip(tip("Change how this value is typed"))
                            .on_click(cx.listener(move |this, _, _, cx| this.cycle_param_kind(index, cx))),
                    )
                    .child(div().w(px(size::FORM_CONTROL * 4.0)).child(row.input.clone()))
            })
            .collect();
        Some(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_3()
                .px_2()
                .py_1()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(div().font_weight(gpui::FontWeight::MEDIUM).child("Parameters"))
                .children(fields)
                .child(
                    div()
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child("Auto reads numbers and true/false; choose Text to keep values like 007. Press Enter or Run."),
                ),
        )
    }
}
