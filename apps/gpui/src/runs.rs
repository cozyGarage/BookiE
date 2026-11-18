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
        self.run_sql(sql, cx);
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
        match bookie_core::sql_dialect::explain_statement(&session.driver_id, &sql) {
            Some(statement) => self.run_sql(statement, cx),
            None => self.status = "This database cannot explain a query".into(),
        }
    }

    fn run_sql(&mut self, sql: String, cx: &mut Context<Self>) {
        if let Some(session) = self.live.clone() {
            self.status = "Running…".into();
            self.begin_background(
                Work::Load,
                cx,
                {
                    let statement = sql.clone();
                    move |cancel| {
                        let (started, clock) = (Instant::now(), std::time::SystemTime::now());
                        let result = session.run_query_controlled(&statement, bookie_gpui::live::ROW_CAP, &cancel);
                        let (rows, error) = match &result {
                            Ok(table) => (i64::try_from(bookie_gpui::source::ResultSource::total(table)).ok(), None),
                            Err(message) => (None, Some(message.clone())),
                        };
                        session.record_history(&statement, clock, started.elapsed(), rows, error);
                        (statement, started.elapsed(), result)
                    }
                },
                |this, (sql, elapsed, result)| match result {
                    Ok(table) => {
                        let (rows, truncated) = (bookie_gpui::source::ResultSource::total(&table), table.truncated);
                        this.panel.messages.statement_ok(&sql, rows, truncated, elapsed);
                        this.add_run(Some(std::sync::Arc::new(table)));
                        if let Some(slot) = this.runs.last_mut() {
                            slot.run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
                        }
                    }
                    Err(message) => {
                        this.panel.messages.statement_failed(&sql, &message, elapsed);
                        this.panel.view = ResultView::Messages;
                        this.status = message;
                    }
                },
            );
            return;
        }
        self.add_run(None);
    }

    pub(crate) fn add_run(&mut self, source: Option<Source>) {
        let mut run = next_run(self.next_run_number);
        if let Some(source) = &source {
            run.rows = source.total();
        }
        self.next_run_number += 1;
        self.runs.push(RunSlot::new(run, source));
        self.activate_run(self.runs.len() - 1);
    }

    pub(crate) fn close_tab(&mut self, index: usize) {
        let closing_query = matches!(self.tabs.items.get(index), Some(Tab::Query));
        if closing_query && self.query_has_view_edits() && self.reject_view_edits() {
            return;
        }
        self.tabs.close(index);
        self.sync_grid_owner();
        if !closing_query {
            return;
        }
        if self.grid_is_query {
            self.invalidate_background();
            self.edits = std::mem::take(&mut self.table_edits);
            self.source = self.table_source.clone();
            self.grid_is_query = false;
            self.editing = None;
            self.compare = None;
            self.cursor = MatchCursor::default();
            self.set_mode(Mode::All);
        }
        self.runs.clear();
        self.active_run = 0;
        self.next_run_number = 1;
    }

    pub(crate) fn query_has_view_edits(&self) -> bool {
        (self.grid_is_query && (self.editing.is_some() || !self.edits.is_empty()))
            || self.runs.iter().any(|slot| !slot.edits.is_empty())
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
        if self.editing.is_some() {
            self.status = "Finish or discard the active cell edit using Commands before switching grids.".into();
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
        self.editing = None;
        self.compare = None;
        self.cursor = MatchCursor::default();
        self.find_reset = true;
        if query {
            self.table_edits = std::mem::take(&mut self.edits);
            if let Some(run) = self.runs.get(self.active_run).map(|slot| slot.run) {
                self.show_run(self.active_run, run);
            } else {
                self.source = self.table_source.clone();
                self.set_mode(Mode::All);
            }
        } else {
            if let Some(slot) = self.runs.get_mut(self.active_run) {
                slot.edits = std::mem::take(&mut self.edits);
            }
            self.edits = std::mem::take(&mut self.table_edits);
            self.source = self.table_source.clone();
            self.set_mode(Mode::All);
        }
        self.grid_is_query = query;
    }

    pub(crate) fn activate_run(&mut self, index: usize) {
        let Some(run) = self.runs.get(index).map(|slot| slot.run) else {
            return;
        };
        if index != self.active_run && self.editing.is_some() {
            self.status = "Finish or discard the active cell edit using Commands before switching results.".into();
            return;
        }
        self.invalidate_background();
        if let Some(previous) = self.runs.get_mut(self.active_run) {
            previous.edits = std::mem::take(&mut self.edits);
        }
        self.show_run(index, run);
    }

    pub(crate) fn select_run(&mut self, index: usize) {
        if index != self.active_run {
            self.activate_run(index);
        }
    }

    pub(crate) fn show_run(&mut self, index: usize, run: Run) {
        self.run_scroll.scroll_to_item(index);
        self.edits = std::mem::take(&mut self.runs[index].edits);
        self.editing = None;
        self.active_run = index;
        self.source = self
            .runs
            .get(index)
            .and_then(|slot| slot.source.clone())
            .unwrap_or_else(|| std::sync::Arc::new(Synthetic::new(run.rows)));
        self.compare = None;
        self.cursor = MatchCursor::default();
        self.set_mode(Mode::All);
        self.status = format!("{} · {} rows", run.label(), thousands(run.rows));
    }

    pub(crate) fn close_run(&mut self, index: usize) {
        if index >= self.runs.len() {
            return;
        }
        let drops_edits = !self.runs[index].edits.is_empty()
            || (index == self.active_run && (self.editing.is_some() || !self.edits.is_empty()));
        if drops_edits && self.reject_view_edits() {
            return;
        }
        self.runs.remove(index);
        if index < self.active_run {
            self.active_run -= 1;
            return;
        }
        if index != self.active_run {
            return;
        }
        self.invalidate_background();
        self.edits.clear();
        if self.runs.is_empty() {
            self.source = self.table_source.clone();
            self.editing = None;
            self.compare = None;
            self.cursor = MatchCursor::default();
            self.set_mode(Mode::All);
            self.status = "No results".into();
            return;
        }
        let next = index.min(self.runs.len() - 1);
        self.show_run(next, self.runs[next].run);
    }
}
