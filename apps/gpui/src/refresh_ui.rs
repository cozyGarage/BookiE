use super::*;

impl GridView {
    pub(crate) fn refresh_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "refresh-table",
            label: "Refresh the table from the database".to_string(),
            enabled: self.live.is_some() && !self.grid_is_query,
        }]
    }

    pub(crate) fn reconnect_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "reconnect",
            label: "Reconnect and refresh the table list (closes other tabs)".to_string(),
            enabled: self.live.is_some() && self.search.origin.is_some(),
        }]
    }

    pub(crate) fn reconnect_now(&mut self, cx: &mut Context<Self>) {
        let Some(origin) = self.search.origin.clone().filter(|_| self.live.is_some()) else {
            self.status = "Open a connection from Home first; there is nothing to reconnect".into();
            return;
        };
        if self.save.busy || self.save.pending.is_some() || !self.cell_edit.edits.is_empty() {
            self.status = "Save or discard the pending edits before reconnecting".into();
            cx.notify();
            return;
        }
        let prefer = (!self.grid_is_query).then(|| self.catalog.grid_table.clone());
        self.connect_preferring(origin, prefer, cx);
    }

    pub(crate) fn refresh_table(&mut self, cx: &mut Context<Self>) {
        if self.live.is_none() || self.grid_is_query {
            self.status = "Open a table from a connection to refresh it".into();
            return;
        }
        if self.save.busy || self.save.pending.is_some() || !self.cell_edit.edits.is_empty() {
            self.status = "Save or discard the pending edits before refreshing the table".into();
            cx.notify();
            return;
        }
        self.status = "Refreshing…".into();
        self.reload_after_save("Refreshed the table".into(), cx);
    }
}
