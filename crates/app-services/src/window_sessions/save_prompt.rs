use std::collections::HashMap;

use uuid::Uuid;

#[derive(Debug, Default)]
pub struct WindowSavePrompt {
    dirty_tabs: HashMap<Uuid, Uuid>,
}

impl WindowSavePrompt {
    pub fn mark_dirty(&mut self, tab_id: Uuid, connection_id: Uuid) {
        self.dirty_tabs.insert(tab_id, connection_id);
    }

    pub fn clear_tab(&mut self, tab_id: Uuid) {
        self.dirty_tabs.remove(&tab_id);
    }

    pub fn clear_connection(&mut self, connection_id: Uuid) {
        self.dirty_tabs.retain(|_, connection| *connection != connection_id);
    }

    pub fn cancel(&mut self) {
        self.dirty_tabs.clear();
    }

    pub fn needs_prompt(&self) -> bool {
        !self.dirty_tabs.is_empty()
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty_tabs.len()
    }

    pub fn dirty_tabs(&self) -> impl Iterator<Item = (Uuid, Uuid)> + '_ {
        self.dirty_tabs.iter().map(|(tab, connection)| (*tab, *connection))
    }

    pub fn dirty_connections(&self) -> Vec<Uuid> {
        let mut connections = Vec::new();
        for connection in self.dirty_tabs.values().copied() {
            if !connections.contains(&connection) {
                connections.push(connection);
            }
        }
        connections
    }

    pub fn connections_in_order(&self, order: &[Uuid]) -> Vec<Uuid> {
        order
            .iter()
            .copied()
            .filter(|connection| self.dirty_tabs.values().any(|dirty| dirty == connection))
            .collect()
    }

    pub fn tabs_for_connection(&self, connection_id: Uuid) -> Vec<Uuid> {
        let mut tabs: Vec<Uuid> = self
            .dirty_tabs
            .iter()
            .filter_map(|(tab, connection)| (*connection == connection_id).then_some(*tab))
            .collect();
        tabs.sort();
        tabs
    }
}
