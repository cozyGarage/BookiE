use bookie_app_services::filter_catalog::{OPERATORS, ValueShape, operator, operator_allowed, rule_from_text};
use bookie_core::{
    ColumnInfo, FilterRule, FilterSet, TableInfo, Value, build_filter_where, filter_value_kind,
    sql_dialect::qualified_table,
};

use super::*;
use crate::chrome::ghost;

pub(crate) fn filter_statement(
    driver_id: &str,
    table: &TableInfo,
    columns: &[ColumnInfo],
    rule: &FilterRule,
) -> Result<(String, Vec<Value>), String> {
    if let Some(column) = columns.iter().find(|column| column.name == rule.column)
        && !operator_allowed(filter_value_kind(column), rule.op)
    {
        return Err(format!(
            "\"{}\" does not apply to the {} column {}",
            operator(rule.op).label,
            column.data_type,
            column.name
        ));
    }
    let set = FilterSet {
        rules: vec![rule.clone()],
        ..FilterSet::default()
    };
    let (clause, params) = build_filter_where(driver_id, columns, &set)
        .map_err(|error| error.to_string())?
        .ok_or("The rule is empty")?;
    let target = qualified_table(driver_id, table.schema.as_deref(), &table.name);
    Ok((format!("SELECT * FROM {target} WHERE {clause}"), params))
}

pub(crate) struct FilterRuleState {
    pub open: bool,
    pub focus: bool,
    pub operator: usize,
    pub column: Option<Entity<TextInput>>,
    pub value: Option<Entity<TextInput>>,
}

impl Default for FilterRuleState {
    fn default() -> Self {
        Self {
            open: false,
            focus: false,
            operator: 0,
            column: None,
            value: None,
        }
    }
}

impl GridView {
    pub(crate) fn filter_rule_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "filter-rule",
            label: "Filter this table by a column, operator and value…".to_string(),
            enabled: self.live.is_some() && !self.grid_is_query,
        }]
    }

    pub(crate) fn focus_filter_rule_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.search.rule.focus) {
            return;
        }
        if let Some(input) = &self.search.rule.value {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
    }

    fn rule_input(&mut self, hint: &str, cx: &mut Context<Self>) -> Entity<TextInput> {
        let input = cx.new(|cx| TextInput::new(hint, cx));
        cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Submit) {
                this.run_filter_rule(cx);
            }
        })
        .detach();
        input
    }

    pub(crate) fn open_filter_rule_bar(&mut self, cx: &mut Context<Self>) {
        if self.live.is_none() || self.grid_is_query {
            self.status = "Open a table to filter it by a rule".into();
            return;
        }
        if self.search.rule.column.is_none() {
            let column = self.rule_input("Column", cx);
            let value = self.rule_input("Value (commas separate values for between and in)", cx);
            self.search.rule.column = Some(column);
            self.search.rule.value = Some(value);
        }
        let active = self
            .cell_cursor
            .and_then(|cursor| self.source.columns().get(cursor.column))
            .map(|column| column.name.clone())
            .unwrap_or_default();
        if let (Some(column), Some(value)) = (self.search.rule.column.clone(), self.search.rule.value.clone()) {
            column.update(cx, |input, cx| input.set_content(&active, cx));
            value.update(cx, |input, cx| input.set_content("", cx));
        }
        self.search.rule.open = true;
        self.search.rule.focus = true;
        self.status = "Choose the operator, type a value and press Enter".into();
        cx.notify();
    }

    pub(crate) fn run_filter_rule(&mut self, cx: &mut Context<Self>) {
        let (Some(session), Some(column), Some(value)) = (
            self.live.clone(),
            self.search.rule.column.clone(),
            self.search.rule.value.clone(),
        ) else {
            return;
        };
        let op = OPERATORS[self.search.rule.operator % OPERATORS.len()].op;
        let rule = match rule_from_text(column.read(cx).content().trim(), op, value.read(cx).content()) {
            Ok(rule) => rule,
            Err(message) => {
                self.status = message;
                cx.notify();
                return;
            }
        };
        let table = self.catalog.grid_table.clone();
        self.search.rule.open = false;
        self.tabs.open_query();
        self.sync_grid_owner();
        self.status = "Filtering…".into();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                let started = Instant::now();
                let columns = session.table_columns(&table)?;
                let (sql, params) = filter_statement(&session.driver_id, &table, &columns, &rule)?;
                let result = session.run_query_bound(&sql, &params, bookie_gpui::live::ROW_CAP, &cancel);
                result.map(|loaded| (started.elapsed(), loaded))
            },
            move |this, result| match result {
                Ok((elapsed, table)) => {
                    this.add_run(Some(std::sync::Arc::new(table)));
                    if let Some(slot) = this.run_tabs.slots.last_mut() {
                        slot.run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
                    }
                    this.status = "Filtered rows are shown as a result".into();
                }
                Err(message) => this.status = format!("Could not filter: {message}"),
            },
        );
    }

    pub(crate) fn filter_rule_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let (column, value) = (self.search.rule.column.clone()?, self.search.rule.value.clone()?);
        if !self.search.rule.open {
            return None;
        }
        let entry = operator(OPERATORS[self.search.rule.operator % OPERATORS.len()].op);
        let needs_value = entry.shape != ValueShape::None;
        let mut bar = div()
            .flex()
            .items_center()
            .gap_3()
            .p_2()
            .flex_none()
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(div().font_weight(gpui::FontWeight::MEDIUM).child("Filter by rule"))
            .child(div().w(px(size::FORM_CONTROL * 3.0)).child(column))
            .child(
                ghost("filter-rule-operator", entry.label, theme).on_click(cx.listener(|this, _, _, cx| {
                    this.search.rule.operator = (this.search.rule.operator + 1) % OPERATORS.len();
                    cx.notify();
                })),
            );
        if needs_value {
            bar = bar.child(div().flex_1().min_w_0().child(value));
        } else {
            bar = bar.child(div().flex_1());
        }
        Some(
            bar.child(
                ghost("run-filter-rule", "Filter", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.run_filter_rule(cx))),
            )
            .child(
                ghost("cancel-filter-rule", "Cancel", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.search.rule.open = false;
                    cx.notify();
                })),
            ),
        )
    }
}

#[cfg(test)]
#[path = "filter_rule_ui_tests.rs"]
mod tests;
