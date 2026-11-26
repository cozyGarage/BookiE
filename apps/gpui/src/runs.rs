use super::*;

const MAX_RESULT_TABS: usize = 64;
const MAX_RETAINED_RESULT_BYTES: usize = 256 * 1024 * 1024;

pub(crate) struct RunSlot {
    pub run: Run,
    pub edits: grid_edit::Edits,
    pub source: Option<Source>,
    pub failure: Option<String>,
    pub estimated_bytes: usize,
    pub cancel_process: bool,
}

impl RunSlot {
    pub fn new(run: Run, source: Option<Source>) -> Self {
        Self {
            run,
            estimated_bytes: source.as_ref().map_or(0, |source| source.estimated_retained_bytes()),
            edits: grid_edit::Edits::new(),
            source,
            failure: None,
            cancel_process: false,
        }
    }
}

fn result_evictions(tabs: &panel_state::RunTabs, incoming_bytes: usize) -> Option<Vec<usize>> {
    let mut count = tabs.slots.len() + 1;
    let mut bytes = tabs
        .slots
        .iter()
        .fold(incoming_bytes, |total, slot| total.saturating_add(slot.estimated_bytes));
    let mut evictions = Vec::new();
    for (index, slot) in tabs.slots.iter().enumerate() {
        if count <= MAX_RESULT_TABS && bytes <= MAX_RETAINED_RESULT_BYTES {
            break;
        }
        if index == tabs.active
            || !slot.edits.is_empty()
            || slot
                .source
                .as_ref()
                .is_some_and(|source| std::sync::Arc::strong_count(source) > 1)
        {
            continue;
        }
        count -= 1;
        bytes -= slot.estimated_bytes;
        evictions.push(index);
    }
    (count <= MAX_RESULT_TABS && bytes <= MAX_RETAINED_RESULT_BYTES).then_some(evictions)
}

fn original_error_location(
    mapping: Option<(String, bookie_core::NamedParameters)>,
    sql: &str,
    position: Option<usize>,
) -> (String, Option<usize>) {
    match mapping {
        Some((original, map)) if map.sql == sql => {
            let position = position
                .and_then(|position| map.source_byte_for_position(&original, position))
                .map(|byte| original[..byte].chars().count() + 1);
            (original, position)
        }
        Some((original, _)) => (original, None),
        None => (sql.to_string(), position),
    }
}

impl GridView {
    pub(crate) fn run_query(&mut self, cx: &mut Context<Self>) {
        let sql = self.sql_input.read(cx).value().to_string();
        if self.live.is_some() && sql.trim().is_empty() {
            self.status = "Type a statement first".into();
            return;
        }
        let source = Some((sql.clone(), 0..sql.len()));
        self.run_with_parameters(sql, source, cx);
    }

    pub(crate) fn explain_query(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.live.clone() else {
            self.status = "Connect to a database to explain a query".into();
            return;
        };
        let sql = self.sql_input.read(cx).value().to_string();
        if sql.trim().is_empty() {
            self.status = "Type a statement first".into();
            return;
        }
        if !bookie_app_services::query_parameters::statement_names(&sql, &session.driver_id).is_empty() {
            self.status = "Explain does not take parameters yet; put the values in the statement".into();
            return;
        }
        match bookie_core::sql_dialect::explain_statement(&session.driver_id, &sql) {
            Some(statement) => self.run_sql(statement, cx),
            None => self.status = "This database cannot explain a query".into(),
        }
    }

    pub(crate) fn run_sql(&mut self, sql: String, cx: &mut Context<Self>) {
        self.run_sql_with(sql, Vec::new(), cx);
    }

    pub(crate) fn run_sql_with(&mut self, sql: String, params: Vec<bookie_core::Value>, cx: &mut Context<Self>) {
        self.run_sql_with_source(sql, params, None, cx);
    }

    pub(crate) fn run_sql_with_source(
        &mut self,
        sql: String,
        params: Vec<bookie_core::Value>,
        source: Option<(String, std::ops::Range<usize>)>,
        cx: &mut Context<Self>,
    ) {
        self.run_sql_with_mapping(sql, params, source, None, cx);
    }

    fn run_sql_in_open_transaction(&mut self, sql: String, unparameterized: bool, cx: &mut Context<Self>) {
        if !unparameterized {
            self.status = "Parameters are not supported inside a transaction yet".into();
            return;
        }
        if self
            .schema_context
            .as_deref()
            .is_some_and(|schema| !schema.trim().is_empty())
        {
            self.status =
                "Clear the schema context before running in a transaction; it is not applied inside a transaction yet"
                    .into();
            return;
        }
        self.run_in_transaction(sql, cx);
    }

    pub(crate) fn run_sql_with_mapping(
        &mut self,
        sql: String,
        params: Vec<bookie_core::Value>,
        source: Option<(String, std::ops::Range<usize>)>,
        mapping: Option<(String, bookie_core::NamedParameters)>,
        cx: &mut Context<Self>,
    ) {
        let cancel_process = std::mem::take(&mut self.pending_cancel_process);
        if self.save.busy {
            self.status = "A save is running; wait for it to finish first".into();
            return;
        }
        if self.transaction_open() {
            self.run_sql_in_open_transaction(sql, params.is_empty() && mapping.is_none(), cx);
            return;
        }
        if let Some(session) = self.live.clone() {
            let execution = match crate::query_execution::Execution::plan(
                &sql,
                params,
                &session.driver_id,
                self.schema_context.as_deref().unwrap_or_default(),
            ) {
                Ok(execution) => execution,
                Err(message) => {
                    self.status = message;
                    cx.notify();
                    return;
                }
            };
            if !matches!(self.tabs.current(), Tab::Query) {
                self.tabs.open_query();
                self.sync_grid_owner();
                if !matches!(self.tabs.current(), Tab::Query) {
                    return;
                }
            }
            self.panel.error_location = None;
            self.sql_input.update(cx, Self::clear_sql_diagnostics);
            self.status = "Running…".into();
            let explaining = crate::plan_view::typed_explain(&sql);
            let current_session = session.clone();
            let selected = self
                .run_tabs
                .slots
                .get(self.run_tabs.active)
                .map(|slot| slot.run.number);
            self.begin_background_with_context(
                Work::Query,
                cx,
                move |cancel| execution.run(&session, cancel),
                move |this, outcomes, cx| {
                    if !this.is_current_session(&current_session) {
                        this.invalidate_query();
                        return;
                    }
                    this.panel.running = None;
                    this.panel.explaining = false;
                    let activate = this.can_activate_query_result(selected) && this.cell_edit.active.is_none();
                    let original = (sql, source, mapping);
                    let error_status = outcomes
                        .last()
                        .and_then(|outcome| outcome.result.as_ref().err())
                        .map(|error| error.message.clone());
                    let first_new_number = this.run_tabs.next_number;
                    for outcome in outcomes {
                        this.apply_query_outcome(outcome, &original, cancel_process, activate, cx);
                    }
                    if activate && this.run_tabs.next_number != first_new_number {
                        this.activate_run(this.run_tabs.slots.len() - 1);
                        if let Some(message) = error_status {
                            this.status = message;
                        }
                    }
                },
            );
            self.panel.explaining = explaining;
            self.track_running_query(cx);
            return;
        }
        self.add_run(None);
    }

    fn apply_query_outcome(
        &mut self,
        outcome: crate::query_execution::Outcome,
        (submitted, source, mapping): &(
            String,
            Option<(String, std::ops::Range<usize>)>,
            Option<(String, bookie_core::NamedParameters)>,
        ),
        cancel_process: bool,
        activate: bool,
        cx: &mut Context<Self>,
    ) {
        let crate::query_execution::Outcome { sql, elapsed, result } = outcome;
        match result {
            Ok(tables) => {
                for table in tables {
                    let rows = bookie_gpui::source::ResultSource::total(&table);
                    let truncated = bookie_gpui::source::ResultSource::is_partial(&table);
                    self.panel.messages.statement_ok(&sql, rows, truncated, elapsed);
                    self.panel.messages.server_messages(&table.messages);
                    if let Some(slot) = self.append_run(Some(std::sync::Arc::new(table)), cancel_process, false) {
                        slot.run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
                    }
                }
            }
            Err(error) => {
                let detail = match error.hint.as_deref() {
                    Some(hint) if !hint.is_empty() => format!("{} · Hint: {hint}", error.message),
                    _ => error.message.clone(),
                };
                self.panel.messages.statement_failed(&sql, &detail, elapsed);
                self.append_failed_run(detail, elapsed);
                if activate {
                    let matches = &sql == submitted;
                    let (original, position) =
                        original_error_location(if matches { mapping.clone() } else { None }, &sql, error.position);
                    self.panel.error_location =
                        position
                            .zip(self.panel.messages.entries().last())
                            .map(|(position, message)| {
                                (
                                    original,
                                    position,
                                    message.id,
                                    if matches { source.clone() } else { None },
                                )
                            });
                    self.show_sql_diagnostic(error.message.clone(), cx);
                    self.status = error.message;
                }
            }
        }
    }

    fn track_running_query(&mut self, cx: &mut Context<Self>) {
        let running = (self.queries.generation, Instant::now());
        self.panel.running = Some(running);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let still_running = this
                    .update(cx, |view, cx| {
                        let still_running = view.panel.running == Some(running);
                        if still_running && matches!(view.tabs.current(), Tab::Query) {
                            cx.notify();
                        }
                        still_running
                    })
                    .unwrap_or(false);
                if !still_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn stop_query(&mut self) {
        if self.stop_transaction_statement() {
            return;
        }
        if self
            .panel
            .running
            .is_some_and(|(generation, _)| generation == self.queries.generation)
            && !self.queries.cancel.is_cancelled()
        {
            self.queries.cancel.cancel();
            self.status = "Cancellation requested…".into();
        }
    }

    pub(crate) fn show_data_dictionary(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.live.clone() else {
            self.status = "Connect to a database to read its data dictionary".into();
            return;
        };
        let Some(driver) = drivers_registry::default_registry().get(&session.driver_id) else {
            self.status = "This database has no data dictionary".into();
            return;
        };
        let capabilities = driver.capabilities();
        self.tabs.open_query();
        self.sync_grid_owner();
        if !self.grid_is_query {
            return;
        }
        self.status = "Reading the data dictionary…".into();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                let started = Instant::now();
                let result = session.data_dictionary_controlled(capabilities, &cancel);
                (started.elapsed(), result)
            },
            |this, (elapsed, result)| match result {
                Ok(table) => {
                    let activate = this.grid_is_query;
                    if let Some(slot) = this.append_run(Some(std::sync::Arc::new(table)), false, activate) {
                        slot.run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
                    }
                }
                Err(message) => this.status = message,
            },
        );
    }

    pub(crate) fn add_run(&mut self, source: Option<Source>) {
        self.append_run(source, false, self.grid_is_query);
    }

    fn can_activate_query_result(&self, selected: Option<usize>) -> bool {
        matches!(self.tabs.current(), Tab::Query)
            && self
                .run_tabs
                .slots
                .get(self.run_tabs.active)
                .is_none_or(|slot| Some(slot.run.number) == selected)
    }

    fn append_run(&mut self, source: Option<Source>, cancel_process: bool, activate: bool) -> Option<&mut RunSlot> {
        let mut run = next_run(self.run_tabs.next_number);
        if let Some(source) = &source {
            run.rows = source.total();
        }
        let mut slot = RunSlot::new(run, source);
        slot.cancel_process = cancel_process;
        if !self.retain_result(slot) {
            return None;
        }
        if activate {
            self.activate_run(self.run_tabs.slots.len() - 1);
        }
        self.run_tabs.slots.last_mut()
    }

    fn retain_result(&mut self, slot: RunSlot) -> bool {
        let Some(evictions) = result_evictions(&self.run_tabs, slot.estimated_bytes) else {
            let message = "Result could not be kept. Close unused result tabs before running more queries.".to_string();
            self.notices.push(message.clone(), notifications::NoticeKind::Error);
            self.status = message;
            return false;
        };
        if !evictions.is_empty() {
            self.notices.push(
                format!(
                    "Closed {} older result tab(s) to stay within the retention limit",
                    evictions.len()
                ),
                notifications::NoticeKind::Success,
            );
        }
        for index in evictions.into_iter().rev() {
            self.run_tabs.slots.remove(index);
            if index < self.run_tabs.active {
                self.run_tabs.active -= 1;
            }
        }
        self.run_tabs.slots.push(slot);
        self.run_tabs.next_number += 1;
        true
    }

    #[cfg(test)]
    pub(crate) fn add_failed_run(&mut self, detail: String, elapsed: std::time::Duration) {
        if self.append_failed_run(detail, elapsed) && self.cell_edit.active.is_none() {
            self.activate_run(self.run_tabs.slots.len() - 1);
        }
    }

    fn append_failed_run(&mut self, detail: String, elapsed: std::time::Duration) -> bool {
        let mut run = next_run(self.run_tabs.next_number);
        run.rows = 0;
        run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
        let source: Source = std::sync::Arc::new(crate::catalog_nav::EmptySource("Failed query".into()));
        let mut slot = RunSlot::new(run, Some(source));
        slot.failure = Some(detail);
        self.retain_result(slot)
    }

    pub(crate) fn close_tab(&mut self, index: usize, cx: &gpui::App) {
        let closing_query = matches!(self.tabs.items.get(index), Some(Tab::Query));
        if closing_query && self.transaction_open() {
            self.status = "Commit or roll back the open transaction before closing the query tab".into();
            return;
        }
        if closing_query && self.query_has_view_edits() && self.reject_view_edits() {
            return;
        }
        if closing_query {
            self.remember_query_draft(cx);
        }
        self.remember_closed_tab(index);
        if matches!(self.tabs.items.get(index), Some(Tab::Diagram)) {
            self.reset_diagram();
        }
        self.tabs.close(index);
        self.sync_grid_owner();
        if !closing_query {
            return;
        }
        self.invalidate_query();
        self.params.rows.clear();
        self.panel.error_location = None;
        if self.grid_is_query {
            self.invalidate_background();
            self.cell_edit.edits = std::mem::take(&mut self.cell_edit.table);
            self.source = self.table_source.clone();
            self.grid_is_query = false;
            self.cell_edit.active = None;
            self.compare = None;
            self.search.cursor = MatchCursor::default();
            self.set_mode(Mode::All);
        }
        self.clear_result_plan();
        self.run_tabs.slots.clear();
        self.run_tabs.active = 0;
        self.run_tabs.next_number = 1;
    }

    pub(crate) fn query_has_view_edits(&self) -> bool {
        (self.grid_is_query && (self.cell_edit.active.is_some() || !self.cell_edit.edits.is_empty()))
            || self.run_tabs.slots.iter().any(|slot| !slot.edits.is_empty())
    }

    pub(crate) fn sync_grid_owner(&mut self) {
        self.tab_scroll.scroll_to_item(strip_position(self.tabs.active));
        let query = match self.tabs.current() {
            Tab::Query => true,
            Tab::Table(name) if *name == self.catalog.grid_table => false,
            Tab::Home | Tab::Settings | Tab::History | Tab::AuditLog | Tab::Diagram => return,
            _ => {
                self.invalidate_background();
                return;
            }
        };
        if query {
            self.clear_fk_peek();
        }
        if query == self.grid_is_query {
            return;
        }
        let saving = self.save.busy || self.save.pending.is_some() || self.form_open();
        if saving || self.cell_edit.active.is_some() {
            self.status = if saving {
                "Finish or cancel the save before switching grids".into()
            } else {
                "Finish or discard the active cell edit using Commands before switching grids".into()
            };
            if let Some(index) = self.tabs.items.iter().position(|tab| match (self.grid_is_query, tab) {
                (true, Tab::Query) => true,
                (false, Tab::Table(name)) => name == &self.catalog.grid_table,
                _ => false,
            }) {
                self.tabs.active = index;
                self.tab_scroll.scroll_to_item(strip_position(index));
            }
            return;
        }
        self.invalidate_background();
        self.reset_edit_history();
        self.cell_edit.active = None;
        self.compare = None;
        self.search.cursor = MatchCursor::default();
        self.search.reset = true;
        if query {
            self.cell_edit.table = std::mem::take(&mut self.cell_edit.edits);
            if let Some(run) = self.run_tabs.slots.get(self.run_tabs.active).map(|slot| slot.run) {
                self.show_run(self.run_tabs.active, run);
            } else {
                self.source = std::sync::Arc::new(crate::catalog_nav::EmptySource("Query".into()));
                self.status.clear();
                self.set_mode(Mode::All);
            }
        } else {
            if let Some(slot) = self.run_tabs.slots.get_mut(self.run_tabs.active) {
                slot.edits = std::mem::take(&mut self.cell_edit.edits);
            }
            self.cell_edit.edits = std::mem::take(&mut self.cell_edit.table);
            self.source = self.table_source.clone();
            self.set_mode(Mode::All);
        }
        self.grid_is_query = query;
    }

    pub(crate) fn activate_run(&mut self, index: usize) {
        let Some(run) = self.run_tabs.slots.get(index).map(|slot| slot.run) else {
            return;
        };
        if index != self.run_tabs.active && self.cell_edit.active.is_some() {
            self.status = "Finish or discard the active cell edit using Commands before switching results".into();
            return;
        }
        self.invalidate_background();
        if let Some(previous) = self.run_tabs.slots.get_mut(self.run_tabs.active) {
            previous.edits = std::mem::take(&mut self.cell_edit.edits);
        }
        self.show_run(index, run);
    }

    pub(crate) fn select_run(&mut self, index: usize) {
        if index != self.run_tabs.active {
            self.activate_run(index);
        }
    }

    pub(crate) fn show_run(&mut self, index: usize, run: Run) {
        self.run_tabs.scroll.scroll_to_item(index);
        self.cell_edit.edits = std::mem::take(&mut self.run_tabs.slots[index].edits);
        self.cell_edit.active = None;
        self.run_tabs.active = index;
        self.source = self
            .run_tabs
            .slots
            .get(index)
            .and_then(|slot| slot.source.clone())
            .unwrap_or_else(|| std::sync::Arc::new(Synthetic::new(run.rows)));
        self.reset_edit_history();
        self.compare = None;
        self.search.cursor = MatchCursor::default();
        self.set_mode(Mode::All);
        self.show_plan_of(self.source.clone().as_ref());
        if self.run_tabs.slots[index].failure.is_some() {
            self.panel.view = ResultView::Messages;
            self.status = format!("{} · failed", run.label());
        } else {
            if self.panel.view == ResultView::Messages {
                self.panel.view = ResultView::Table;
            }
            self.status = format!("{} · {} rows", run.label(), thousands(run.rows));
        }
    }

    pub(crate) fn close_run(&mut self, index: usize) {
        if index >= self.run_tabs.slots.len() {
            return;
        }
        let drops_edits = !self.run_tabs.slots[index].edits.is_empty()
            || (index == self.run_tabs.active && (self.cell_edit.active.is_some() || !self.cell_edit.edits.is_empty()));
        if drops_edits && self.reject_view_edits() {
            return;
        }
        self.run_tabs.slots.remove(index);
        if index < self.run_tabs.active {
            self.run_tabs.active -= 1;
            return;
        }
        if index != self.run_tabs.active {
            return;
        }
        self.invalidate_background();
        self.cell_edit.edits.clear();
        self.reset_edit_history();
        if self.run_tabs.slots.is_empty() {
            self.clear_result_plan();
            self.source = std::sync::Arc::new(crate::catalog_nav::EmptySource("Query".into()));
            self.cell_edit.active = None;
            self.compare = None;
            self.search.cursor = MatchCursor::default();
            self.set_mode(Mode::All);
            self.status = "No results".into();
            return;
        }
        let next = index.min(self.run_tabs.slots.len() - 1);
        self.show_run(next, self.run_tabs.slots[next].run);
    }
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    #[test]
    fn result_retention_protects_the_selected_result_and_in_flight_sources() {
        let held: Source = std::sync::Arc::new(crate::catalog_nav::EmptySource("Held".into()));
        let mut tabs = panel_state::RunTabs::new(RunSlot::new(next_run(1), None));
        tabs.slots[0].estimated_bytes = MAX_RETAINED_RESULT_BYTES / 2;
        let mut owned = RunSlot::new(next_run(2), Some(held.clone()));
        owned.estimated_bytes = MAX_RETAINED_RESULT_BYTES / 4;
        tabs.slots.push(owned);
        let mut idle = RunSlot::new(next_run(3), None);
        idle.estimated_bytes = MAX_RETAINED_RESULT_BYTES / 4;
        tabs.slots.push(idle);

        assert_eq!(result_evictions(&tabs, MAX_RETAINED_RESULT_BYTES / 4), Some(vec![2]));
        assert_eq!(result_evictions(&tabs, MAX_RETAINED_RESULT_BYTES / 2), None);
        drop(held);
        assert_eq!(result_evictions(&tabs, MAX_RETAINED_RESULT_BYTES / 2), Some(vec![1, 2]));
    }

    #[test]
    fn result_retention_bounds_even_empty_results() {
        let mut tabs = panel_state::RunTabs::new(RunSlot::new(next_run(1), None));
        for number in 2..=MAX_RESULT_TABS {
            tabs.slots.push(RunSlot::new(next_run(number), None));
        }
        tabs.active = 1;
        assert_eq!(result_evictions(&tabs, 0), Some(vec![0]));
    }
}

fn strip_position(tab_index: usize) -> usize {
    tab_index.saturating_sub(1)
}
