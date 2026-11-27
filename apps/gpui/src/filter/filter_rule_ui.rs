use bookie_app_services::filter_catalog::{OPERATORS, ValueShape, operator, operator_allowed, rule_from_text};
use bookie_core::{
    ColumnInfo, FilterRule, FilterSet, TableInfo, Value, build_filter_where, filter_value_kind,
    sql_dialect::qualified_table,
};

use crate::surfaces::{ButtonKind, button, task_bar};
use crate::*;

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

const OPERATOR_GROUPS: [std::ops::Range<usize>; 3] = [0..6, 6..13, 13..OPERATORS.len()];

impl GridView {
    pub(crate) fn filter_rule_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "filter-rule",
            label: "Filter this table by a column, operator and value…".to_string(),
            enabled: self.shows_live_table(),
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
        if !self.shows_live_table() {
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
        self.tabs.open_query();
        self.sync_grid_owner();
        if !self.grid_is_query {
            return;
        }
        self.search.rule.open = false;
        self.status = "Filtering…".into();
        self.begin_background(
            Work::Tool,
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
                Err(message) => this.report_failure("filter", &message),
            },
        );
    }

    pub(crate) fn set_filter_operator(&mut self, index: usize) {
        self.search.rule.operator = index % OPERATORS.len();
    }

    fn operator_menu(&self, label: &str, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::DropdownMenu as _;
        let selected = self.search.rule.operator % OPERATORS.len();
        let owner = cx.entity().downgrade();
        crate::surfaces::select_button(
            "filter-rule-operator",
            Some(label),
            "Operator",
            crate::surfaces::Bar::Small,
            theme,
        )
        .dropdown_menu(move |menu, _, _| {
            OPERATOR_GROUPS.iter().enumerate().fold(menu, |menu, (group, range)| {
                let options: Vec<(gpui::SharedString, usize)> = range
                    .clone()
                    .map(|index| (OPERATORS[index].label.into(), index))
                    .collect();
                let owner = owner.clone();
                let menu = if group > 0 { menu.separator() } else { menu };
                crate::menus::select(menu, &options, Some(&selected), move |index, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.set_filter_operator(index);
                        cx.notify();
                    });
                })
            })
        })
    }

    pub(crate) fn filter_rule_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let (column, value) = (self.search.rule.column.clone()?, self.search.rule.value.clone()?);
        if !self.search.rule.open {
            return None;
        }
        let entry = operator(OPERATORS[self.search.rule.operator % OPERATORS.len()].op);
        let needs_value = entry.shape != ValueShape::None;
        let mut bar = task_bar("Filter by rule", theme)
            .child(div().w(px(size::FORM_CONTROL * 3.0)).child(column))
            .child(self.operator_menu(entry.label, theme, cx));
        if needs_value {
            bar = bar.child(div().flex_1().min_w_0().child(value));
        } else {
            bar = bar.child(div().flex_1());
        }
        Some(
            bar.child(
                button(ButtonKind::Primary, "run-filter-rule", "Filter", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.run_filter_rule(cx))),
            )
            .child(
                button(ButtonKind::Ghost, "cancel-filter-rule", "Cancel", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.search.rule.open = false;
                        cx.notify();
                    },
                )),
            ),
        )
    }
}

#[cfg(test)]
#[path = "filter_rule_ui_tests.rs"]
mod tests;
