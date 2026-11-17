use super::*;

impl GridView {
    pub(crate) fn run_query(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.live.clone() {
            let sql = self.sql_input.read(cx).value().to_string();
            if sql.trim().is_empty() {
                self.status = "Type a statement first".into();
                return;
            }
            self.status = "Running…".into();
            self.begin_background(
                Work::Load,
                cx,
                {
                    let statement = sql.clone();
                    move |cancel| {
                        let started = Instant::now();
                        let result = session.run_query_controlled(&statement, bookie_gpui::live::ROW_CAP, &cancel);
                        (statement, started.elapsed(), result)
                    }
                },
                |this, (sql, elapsed, result)| match result {
                    Ok(table) => {
                        let (rows, truncated) = (bookie_gpui::source::ResultSource::total(&table), table.truncated);
                        this.messages.statement_ok(&sql, rows, truncated, elapsed);
                        this.add_run(Some(std::sync::Arc::new(table)));
                        if let Some(run) = this.runs.last_mut() {
                            run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
                        }
                    }
                    Err(message) => {
                        this.messages.statement_failed(&sql, &message, elapsed);
                        this.result_view = ResultView::Messages;
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
        self.runs.push(run);
        self.run_sources.push(source);
        self.run_edits.push(grid_edit::Edits::new());
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
        self.run_sources.clear();
        self.run_edits.clear();
        self.active_run = 0;
        self.next_run_number = 1;
    }

    pub(crate) fn query_has_view_edits(&self) -> bool {
        (self.grid_is_query && (self.editing.is_some() || !self.edits.is_empty()))
            || self.run_edits.iter().any(|edits| !edits.is_empty())
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
            if let Some(run) = self.runs.get(self.active_run).copied() {
                self.show_run(self.active_run, run);
            } else {
                self.source = self.table_source.clone();
                self.set_mode(Mode::All);
            }
        } else {
            if let Some(edits) = self.run_edits.get_mut(self.active_run) {
                *edits = std::mem::take(&mut self.edits);
            }
            self.edits = std::mem::take(&mut self.table_edits);
            self.source = self.table_source.clone();
            self.set_mode(Mode::All);
        }
        self.grid_is_query = query;
    }

    pub(crate) fn activate_run(&mut self, index: usize) {
        let Some(run) = self.runs.get(index).copied() else {
            return;
        };
        if index != self.active_run && self.editing.is_some() {
            self.status = "Finish or discard the active cell edit using Commands before switching results.".into();
            return;
        }
        self.invalidate_background();
        if let Some(previous) = self.run_edits.get_mut(self.active_run) {
            *previous = std::mem::take(&mut self.edits);
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
        self.edits = std::mem::take(&mut self.run_edits[index]);
        self.editing = None;
        self.active_run = index;
        self.source = self
            .run_sources
            .get(index)
            .cloned()
            .flatten()
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
        let drops_edits = !self.run_edits[index].is_empty()
            || (index == self.active_run && (self.editing.is_some() || !self.edits.is_empty()));
        if drops_edits && self.reject_view_edits() {
            return;
        }
        self.runs.remove(index);
        self.run_sources.remove(index);
        self.run_edits.remove(index);
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
        self.show_run(next, self.runs[next]);
    }
}
