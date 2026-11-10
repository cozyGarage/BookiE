use std::sync::Arc;

use bookie_gpui::live::{LiveSession, LiveTable, ROW_CAP};
use bookie_gpui::shell::{Catalog, Tab};
use bookie_gpui::source::Source;
use gpui::{Context, Window};

use crate::{GridView, Mode};

impl GridView {
    pub(super) fn attach_live(&mut self, table: Arc<LiveTable>) {
        let session = table.session().clone();
        self.catalog = Catalog {
            connection: format!("{} · {}", session.driver_id, session.label),
            database: session.label.clone(),
            tables: session.tables.clone(),
            grid_table: table.table_name().to_string(),
        };
        self.tabs.items[1] = Tab::Table(self.catalog.grid_table.clone());
        self.live = Some(session);
    }

    pub(super) fn open_table_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
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
