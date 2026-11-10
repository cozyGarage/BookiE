use std::sync::Arc;

use bookie_gpui::live::{LiveSession, LiveTable, Origin, ROW_CAP};
use bookie_gpui::model::{Cell, ColumnMeta, Diff, FindResult, Predicate};
use bookie_gpui::shell::{Catalog, Tab};
use bookie_gpui::source::{ResultSource, Source};
use gpui::{Context, Window};
use tokio_util::sync::CancellationToken;

use crate::{GridView, Mode};

impl GridView {
    pub(super) fn attach_live(&mut self, table: Arc<LiveTable>) {
        let session = table.session().clone();
        let table_name = table.table_name().to_string();
        self.tabs.items[1] = Tab::Table(table_name.clone());
        self.attach_session(session, table_name);
    }

    fn attach_session(&mut self, session: Arc<LiveSession>, table_name: String) {
        self.catalog = Catalog {
            connection: format!("{} · {}", session.driver_id, session.label),
            database: session.label.clone(),
            tables: session.tables.clone(),
            grid_table: table_name,
        };
        self.live = Some(session);
        self.runs.clear();
        self.run_sources.clear();
        self.run_edits.clear();
        self.active_run = 0;
        self.next_run_number = 1;
    }

    pub(super) fn connect(&mut self, origin: Origin, cx: &mut Context<Self>) {
        self.status = "Connecting…".into();
        self.begin_background(
            cx,
            move |cancel| open_first_table(&origin, &cancel),
            |this, opened| match opened {
                Ok(table) => this.connect_live(table),
                Err(message) => this.status = message,
            },
        );
    }

    fn connect_live(&mut self, opened: Opened) {
        self.edits.clear();
        self.table_edits.clear();
        self.editing = None;
        self.compare = None;
        self.cursor = Default::default();
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
                self.attach_session(session, String::new());
                self.grid_is_query = true;
            }
        }
        self.set_mode(Mode::All);
        self.tabs.active = 1;
        self.connection_open = true;
        self.database_open = true;
        self.status = "Connected".into();
    }

    pub(super) fn open_table_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.catalog.tables.is_empty() {
            return;
        }
        let Some(session) = self.live.clone().filter(|_| name != self.catalog.grid_table) else {
            self.tabs.open_table(name);
            self.sync_grid_owner();
            if self.tabs.shows_grid_for(&self.catalog.grid_table) {
                window.focus(&self.focus, cx);
            }
            return;
        };
        self.status = format!("Loading {name}…");
        let wanted = name.to_string();
        self.begin_background(
            cx,
            move |cancel| load(&session, &wanted, &cancel),
            move |this, loaded| match loaded {
                Ok((name, table)) => this.show_loaded_table(name, table),
                Err(message) => this.status = message,
            },
        );
    }

    fn show_loaded_table(&mut self, name: String, table: Source) {
        self.catalog.grid_table = name.clone();
        self.source = table.clone();
        self.table_source = table;
        self.edits.clear();
        self.table_edits.clear();
        self.compare = None;
        self.cursor = Default::default();
        self.set_mode(Mode::All);
        self.tabs.open_table(&name);
        self.grid_is_query = false;
        self.sync_grid_owner();
        self.status.clear();
    }
}

fn load(
    session: &Arc<LiveSession>,
    name: &str,
    cancellation: &tokio_util::sync::CancellationToken,
) -> Result<(String, Source), String> {
    let table = session.load_table_controlled(name, ROW_CAP, cancellation)?;
    Ok((name.to_string(), Arc::new(table)))
}

enum Opened {
    Table(Arc<LiveTable>),
    Empty(Arc<LiveSession>),
}

fn open_first_table(origin: &Origin, cancellation: &CancellationToken) -> Result<Opened, String> {
    let session = LiveSession::open_controlled(origin, cancellation)?;
    if cancellation.is_cancelled() {
        return Err("The operation was cancelled.".into());
    }
    match session.tables.first() {
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
        let result = open_first_table(&Origin::File(PathBuf::from("/missing/bookie.db")), &cancellation);
        assert!(
            result.as_ref().is_err_and(|message| message.contains("Cancelled")),
            "{:?}",
            result.as_ref().map(|_| ())
        );
    }
}
