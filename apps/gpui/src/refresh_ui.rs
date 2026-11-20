use super::*;

impl GridView {
    pub(crate) fn refresh_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "refresh-table",
            label: "Refresh the table from the database".to_string(),
            enabled: self.live.is_some() && !self.grid_is_query,
        }]
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
