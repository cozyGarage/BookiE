use super::*;

pub(crate) struct RunSlot {
    pub run: Run,
    pub edits: grid_edit::Edits,
    pub source: Option<Source>,
}

impl RunSlot {
    pub fn new(run: Run, source: Option<Source>) -> Self {
        Self {
            run,
            edits: grid_edit::Edits::new(),
            source,
        }
    }
}

impl GridView {
    pub(crate) fn run_query(&mut self, cx: &mut Context<Self>) {
        let sql = self.sql_input.read(cx).value().to_string();
        if self.live.is_some() && sql.trim().is_empty() {
            self.status = "Type a statement first".into();
            return;
        }
        self.run_with_parameters(sql, cx);
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
            Some(statement) => {
                self.panel.explaining = true;
                self.run_sql(statement, cx)
            }
            None => self.status = "This database cannot explain a query".into(),
        }
    }

    pub(crate) fn run_sql(&mut self, sql: String, cx: &mut Context<Self>) {
        self.run_sql_with(sql, Vec::new(), cx);
    }

    pub(crate) fn run_sql_with(&mut self, sql: String, params: Vec<bookie_core::Value>, cx: &mut Context<Self>) {
        if self.save.busy {
            self.status = "A save is running; wait for it to finish first".into();
            return;
        }
        if let Some(session) = self.live.clone() {
            self.panel.error_location = None;
            self.status = "Running…".into();
            self.begin_background(
                Work::Load,
                cx,
                {
                    let statement = sql.clone();
                    move |cancel| {
                        let (started, clock) = (Instant::now(), std::time::SystemTime::now());
                        let result =
                            session.run_query_bound_detailed(&statement, &params, bookie_gpui::live::ROW_CAP, &cancel);
                        let (rows, error) = match &result {
                            Ok(table) => (
                                i64::try_from(bookie_gpui::source::ResultSource::total(table)).ok(),
                                None,
                            ),
                            Err(error) => (None, Some(error.message.clone())),
                        };
                        session.record_history(&statement, clock, started.elapsed(), rows, error);
                        (statement, started.elapsed(), result)
                    }
                },
                |this, (sql, elapsed, result)| match result {
                    Ok(table) => {
                        this.panel.error_location = None;
                        let (rows, truncated) = (bookie_gpui::source::ResultSource::total(&table), table.truncated);
                        this.panel.messages.statement_ok(&sql, rows, truncated, elapsed);
                        this.show_plan_of(&table);
                        this.add_run(Some(std::sync::Arc::new(table)));
                        if let Some(slot) = this.run_tabs.slots.last_mut() {
                            slot.run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
                        }
                    }
                    Err(error) => {
                        this.panel.explaining = false;
                        this.panel.messages.statement_failed(&sql, &error.message, elapsed);
                        this.panel.error_location = error
                            .position
                            .zip(this.panel.messages.entries().last())
                            .map(|(position, message)| (sql.clone(), position, message.id));
                        this.panel.view = ResultView::Messages;
                        this.status = error.message;
                    }
                },
            );
            return;
        }
        self.add_run(None);
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
                    this.add_run(Some(std::sync::Arc::new(table)));
                    if let Some(slot) = this.run_tabs.slots.last_mut() {
                        slot.run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
                    }
                }
                Err(message) => this.status = message,
            },
        );
    }

    pub(crate) fn add_run(&mut self, source: Option<Source>) {
        let mut run = next_run(self.run_tabs.next_number);
        if let Some(source) = &source {
            run.rows = source.total();
        }
        self.run_tabs.next_number += 1;
        self.run_tabs.slots.push(RunSlot::new(run, source));
        self.activate_run(self.run_tabs.slots.len() - 1);
    }

    pub(crate) fn close_tab(&mut self, index: usize) {
        let closing_query = matches!(self.tabs.items.get(index), Some(Tab::Query));
        if closing_query && self.query_has_view_edits() && self.reject_view_edits() {
            return;
        }
        self.remember_closed_tab(index);
        self.tabs.close(index);
        self.sync_grid_owner();
        if !closing_query {
            return;
        }
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
        self.run_tabs.slots.clear();
        self.run_tabs.active = 0;
        self.run_tabs.next_number = 1;
    }

    pub(crate) fn query_has_view_edits(&self) -> bool {
        (self.grid_is_query && (self.cell_edit.active.is_some() || !self.cell_edit.edits.is_empty()))
            || self.run_tabs.slots.iter().any(|slot| !slot.edits.is_empty())
    }

    pub(crate) fn sync_grid_owner(&mut self) {
        self.tab_scroll.scroll_to_item(self.tabs.active);
        let query = match self.tabs.current() {
            Tab::Query => true,
            Tab::Table(name) if *name == self.catalog.grid_table => false,
            _ => {
                self.invalidate_background();
                return;
            }
        };
        if query == self.grid_is_query {
            return;
        }
        let saving = self.save.busy || self.save.pending.is_some() || self.form_open();
        if saving || self.cell_edit.active.is_some() {
            self.status = if saving {
                "Finish or cancel the save before switching grids.".into()
            } else {
                "Finish or discard the active cell edit using Commands before switching grids.".into()
            };
            if let Some(index) = self.tabs.items.iter().position(|tab| match (self.grid_is_query, tab) {
                (true, Tab::Query) => true,
                (false, Tab::Table(name)) => name == &self.catalog.grid_table,
                _ => false,
            }) {
                self.tabs.active = index;
                self.tab_scroll.scroll_to_item(index);
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
                self.source = self.table_source.clone();
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
            self.status = "Finish or discard the active cell edit using Commands before switching results.".into();
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
        self.status = format!("{} · {} rows", run.label(), thousands(run.rows));
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
            self.source = self.table_source.clone();
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
