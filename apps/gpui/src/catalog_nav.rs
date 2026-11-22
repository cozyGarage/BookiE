use std::sync::Arc;

use bookie_core::TableInfo;
use bookie_gpui::live::{LiveSession, LiveTable, Origin, ROW_CAP};
use bookie_gpui::model::{Cell, ColumnMeta, Diff, FindResult, Predicate};
use bookie_gpui::shell::{Catalog, Tab, table_info, table_label};
use bookie_gpui::source::{ResultSource, Source};
use gpui::{Context, Window};
use tokio_util::sync::CancellationToken;

use crate::{GridView, Mode, Work};

impl GridView {
    pub(crate) fn table_sort(&self, table: &TableInfo) -> Option<&bookie_app_services::read_session::TableSort> {
        self.table_sorts.iter().find(|(identity, _)| identity == table).map(|(_, sort)| sort)
    }

    pub(crate) fn remember_table_sort(&mut self, table: &TableInfo, sort: Option<bookie_app_services::read_session::TableSort>) {
        self.table_sorts.retain(|(identity, _)| identity != table);
        if let Some(sort) = sort {
            self.table_sorts.push((table.clone(), sort));
        }
    }

    pub(crate) fn sort_current_table(
        &mut self,
        column: &str,
        ascending: Option<bool>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.table.update(cx, |table, cx| table.refresh(cx));
        let column = column.to_owned();
        let Some(session) = self.live.clone().filter(|session| session.driver_id == "postgres") else {
            self.status = "Server-side table sorting is not available for this connection.".into();
            return;
        };
        let Tab::Table(table) = self.tabs.current().clone() else {
            return;
        };
        if self.reject_view_edits() {
            cx.notify();
            return;
        }
        let sort = ascending.map(|ascending| bookie_app_services::read_session::TableSort {
            column: column.clone(),
            ascending,
        });
        let current_session = session.clone();
        let current_table = table.clone();
        let requested_sort = sort.clone();
        self.status = format!("Sorting {column}…");
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                session
                    .load_table_sorted_controlled(&table, ROW_CAP, sort.as_ref(), &cancel)
                    .map(|loaded| Arc::new(loaded) as Source)
            },
            move |this, result| {
                if !this.is_current_session(&current_session)
                    || this.catalog.grid_table != current_table
                    || this.grid_is_query
                {
                    return;
                }
                match result {
                    Ok(source) => {
                        if this.reject_view_edits() {
                            return;
                        }
                        this.remember_table_sort(&current_table, requested_sort);
                        this.show_loaded_table(current_table, source);
                    }
                    Err(message) => this.status = format!("Could not sort {column}: {message}"),
                }
            },
        );
    }

    pub(crate) fn count_table_rows(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self
            .live
            .clone()
            .filter(|_| !self.grid_is_query && self.compare.is_none() && !self.catalog.grid_table.name.is_empty())
        else {
            return;
        };
        let table = self.catalog.grid_table.clone();
        let current_session = session.clone();
        let current_table = table.clone();
        self.status = format!("Counting all rows in {}…", table.name);
        self.begin_background(
            Work::Count,
            cx,
            move |cancel| session.count_table_rows(&table, &cancel),
            move |this, result| {
                if this.grid_is_query
                    || this.catalog.grid_table != current_table
                    || !this
                        .live
                        .as_ref()
                        .is_some_and(|live| Arc::ptr_eq(live, &current_session))
                {
                    return;
                }
                this.status = match result {
                    Ok(count) => format!(
                        "Database table {} has {} rows at count time",
                        current_table.name,
                        crate::thousands(count)
                    ),
                    Err(message) => format!("Could not count rows in {}: {message}", current_table.name),
                };
            },
        );
    }

    pub(super) fn attach_live(&mut self, table: Arc<LiveTable>) {
        let session = table.session().clone();
        let Some(identity) = table.table_info().cloned() else {
            self.status = "The loaded result has no table identity.".into();
            return;
        };
        self.tabs.items[1] = Tab::Table(identity.clone());
        self.attach_session(session, identity);
    }

    pub(crate) fn is_current_session(&self, session: &Arc<LiveSession>) -> bool {
        self.live.as_ref().is_some_and(|live| Arc::ptr_eq(live, session))
    }

    fn attach_session(&mut self, session: Arc<LiveSession>, table: TableInfo) {
        self.table_sorts.clear();
        self.edits_changed();
        self.reset_edit_history();
        self.close_insert_form();
        self.close_import_bar();
        self.close_structure_bar();
        self.catalog = Catalog {
            connection: format!("{} · {}", session.driver_id, session.label),
            database: session.label.clone(),
            tables: session.tables.clone(),
            grid_table: table,
        };
        self.live = Some(session);
        self.panel.follow = None;
        self.sidebar.catalog.clear();
        self.sidebar.fields.clear();
        self.sidebar.collapsed_schemas = Default::default();
        self.run_tabs.slots.clear();
        self.run_tabs.active = 0;
        self.run_tabs.next_number = 1;
    }

    pub(super) fn connect(&mut self, origin: Origin, cx: &mut Context<Self>) {
        self.connect_preferring(origin, None, cx);
    }

    pub(super) fn connect_preferring(&mut self, origin: Origin, prefer: Option<TableInfo>, cx: &mut Context<Self>) {
        if self.reject_view_edits() {
            cx.notify();
            return;
        }
        self.status = "Connecting…".into();
        self.search.origin = Some(origin.clone());
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| open_first_table(&origin, prefer.as_ref(), &cancel),
            |this, opened| match opened {
                Ok(table) => this.connect_live(table),
                Err(message) => this.status = message,
            },
        );
    }

    fn connect_live(&mut self, opened: Opened) {
        if self.reject_view_edits() {
            return;
        }
        self.cell_edit.edits.clear();
        self.cell_edit.table.clear();
        self.compare = None;
        self.search.cursor = Default::default();
        self.tabs = bookie_gpui::shell::Tabs::default();
        match opened {
            Opened::Table(table) => {
                self.source = table.clone();
                self.table_source = table.clone();
                self.attach_live(table);
                self.grid_is_query = false;
            }
            Opened::Empty(session) => {
                let origin = format!("{} · {} · read-only", session.driver_id, session.label);
                let source: Source = Arc::new(EmptySource(origin));
                self.source = source.clone();
                self.table_source = source;
                self.tabs.items.truncate(1);
                self.tabs.open_query();
                self.attach_session(session, table_info(""));
                self.grid_is_query = true;
            }
        }
        self.set_mode(Mode::All);
        self.tabs.active = 1;
        self.sidebar.connection_open = true;
        self.sidebar.database_open = true;
        self.status = "Connected".into();
    }

    pub(super) fn has_view_edits(&self) -> bool {
        self.cell_edit.active.is_some()
            || !self.cell_edit.edits.is_empty()
            || !self.cell_edit.table.is_empty()
            || self.run_tabs.slots.iter().any(|slot| !slot.edits.is_empty())
    }

    pub(super) fn reject_view_edits(&mut self) -> bool {
        if self.save.busy {
            self.status = "A save is running; wait for it to finish first".into();
            return true;
        }
        if !self.has_view_edits() {
            return false;
        }
        self.status = "Discard all view-only edits using Commands before closing or switching.".into();
        true
    }

    pub(super) fn discard_view_edits(&mut self) {
        self.edits_changed();
        self.reset_edit_history();
        self.cell_edit.active = None;
        self.cell_edit.edits.clear();
        self.cell_edit.table.clear();
        for slot in &mut self.run_tabs.slots {
            slot.edits.clear();
        }
        self.status = "Discarded all view-only edits.".into();
    }

    pub(super) fn open_table_named(&mut self, table: &TableInfo, window: &mut Window, cx: &mut Context<Self>) {
        if self.catalog.tables.is_empty() {
            return;
        }
        let Some(session) = self.live.clone().filter(|_| table != &self.catalog.grid_table) else {
            self.tabs.open_table(table);
            self.sync_grid_owner();
            if self.tabs.shows_grid_for(&self.catalog.grid_table) {
                window.focus(&self.focus, cx);
            }
            return;
        };
        if self.reject_view_edits() {
            cx.notify();
            return;
        }
        self.status = format!("Loading {}…", table_label(table));
        let wanted = table.clone();
        let sort = self.table_sort(&wanted).cloned();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| load(&session, &wanted, sort.as_ref(), &cancel),
            move |this, loaded| match loaded {
                Ok((name, table)) => this.show_loaded_table(name, table),
                Err(message) => this.status = message,
            },
        );
    }

    pub(crate) fn reload_after_save(&mut self, done: String, cx: &mut Context<Self>) {
        let Some(session) = self.live.clone() else {
            self.save.busy = false;
            return;
        };
        self.cell_edit.edits.clear();
        let wanted = self.catalog.grid_table.clone();
        let sort = self.table_sort(&wanted).cloned();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| load(&session, &wanted, sort.as_ref(), &cancel),
            move |this, loaded| {
                this.save.busy = false;
                match loaded {
                    Ok((name, table)) => {
                        this.show_loaded_table(name, table);
                        this.status = done;
                    }
                    Err(message) => this.status = format!("{done}, but the table could not be reloaded: {message}"),
                }
            },
        );
    }

    fn show_loaded_table(&mut self, identity: TableInfo, table: Source) {
        if self.reject_view_edits() {
            return;
        }
        self.edits_changed();
        self.reset_edit_history();
        self.close_insert_form();
        self.close_import_bar();
        self.close_structure_bar();
        self.sidebar.fields.clear();
        self.catalog.grid_table = identity.clone();
        self.source = table.clone();
        self.table_source = table;
        self.cell_edit.edits.clear();
        self.cell_edit.table.clear();
        self.compare = None;
        self.search.cursor = Default::default();
        self.set_mode(Mode::All);
        self.tabs.open_table(&identity);
        self.grid_is_query = false;
        self.sync_grid_owner();
        self.status.clear();
    }
}

fn load(
    session: &Arc<LiveSession>,
    table: &TableInfo,
    sort: Option<&bookie_app_services::read_session::TableSort>,
    cancellation: &tokio_util::sync::CancellationToken,
) -> Result<(TableInfo, Source), String> {
    let loaded = session.load_table_sorted_controlled(table, ROW_CAP, sort, cancellation)?;
    Ok((table.clone(), Arc::new(loaded)))
}

enum Opened {
    Table(Arc<LiveTable>),
    Empty(Arc<LiveSession>),
}

fn open_first_table(
    origin: &Origin,
    prefer: Option<&TableInfo>,
    cancellation: &CancellationToken,
) -> Result<Opened, String> {
    let session = LiveSession::open_controlled(origin, cancellation)?;
    if cancellation.is_cancelled() {
        return Err("The operation was cancelled.".into());
    }
    match prefer.and_then(|wanted| session.tables.iter().find(|table| *table == wanted)).or(session.tables.first()) {
        Some(first) => Ok(Opened::Table(Arc::new(session.load_table_controlled(
            first,
            ROW_CAP,
            cancellation,
        )?))),
        None => Ok(Opened::Empty(session)),
    }
}

struct EmptySource(String);

impl ResultSource for EmptySource {
    fn columns(&self) -> &[ColumnMeta] {
        &[]
    }
    fn origin(&self) -> String {
        self.0.clone()
    }
    fn total(&self) -> u64 {
        0
    }
    fn row(&self, _index: u64) -> Vec<Cell> {
        Vec::new()
    }
    fn find(&self, _query: &str, _cap: usize, _cancel: &CancellationToken) -> Result<FindResult, &'static str> {
        Err("No result is loaded")
    }
    fn filter(&self, _predicate: &Predicate, _cancel: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        Err("No result is loaded")
    }
    fn compare(&self, _keyed: bool, _cancel: &CancellationToken) -> Result<Diff, &'static str> {
        Err("No result is loaded")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_cancelled_connection_request_does_not_open_its_file() {
        let cancellation = tokio_util::sync::CancellationToken::new();
        cancellation.cancel();
        let path = std::env::temp_dir().join(format!("bookie-cancelled-open-{}.sqlite", uuid::Uuid::new_v4()));
        let result = open_first_table(&Origin::File(PathBuf::from(&path)), None, &cancellation);
        let was_opened = path.exists();
        let _ = std::fs::remove_file(path);
        assert!(result.is_err(), "cancelled request succeeded");
        assert!(!was_opened, "cancelled request created its database file");
    }
}
