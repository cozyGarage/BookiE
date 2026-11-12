use std::collections::HashMap;
use std::sync::Arc;

use bookie_policy::PolicyGuard;
use uuid::Uuid;

mod open_sessions;
mod save_prompt;
mod tab_connection;

pub use open_sessions::{
    OPEN_SESSIONS_VERSION, OpenSessionsLoad, OpenSessionsRecord, StartupRestorePlan, parse_open_sessions,
    restore_window_keys, startup_restore_keys, startup_restore_plan,
};
pub use save_prompt::WindowSavePrompt;
pub use tab_connection::{TabConnectionError, disconnect_scope, plan_dirty_tabs, take_latest_for};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachTabError {
    UnknownConnection,
    TabBelongsToOtherConnection,
}

#[derive(Debug, Default)]
pub struct WindowSessions {
    working: Vec<Uuid>,
    front: Option<Uuid>,
    tabs: HashMap<Uuid, Uuid>,
    mcp: Vec<Uuid>,
}

impl WindowSessions {
    pub fn working_order(&self) -> &[Uuid] {
        &self.working
    }

    pub fn front(&self) -> Option<Uuid> {
        self.front
    }

    pub fn is_working(&self, connection_id: Uuid) -> bool {
        self.working.contains(&connection_id)
    }

    pub fn is_mcp(&self, connection_id: Uuid) -> bool {
        self.mcp.contains(&connection_id)
    }

    pub fn open_working(&mut self, connection_id: Uuid) -> bool {
        if self.working.contains(&connection_id) {
            return false;
        }
        self.mcp.retain(|id| *id != connection_id);
        self.working.push(connection_id);
        if self.front.is_none() {
            self.front = Some(connection_id);
        }
        true
    }

    pub fn focus_working(&mut self, connection_id: Uuid) -> bool {
        if !self.working.contains(&connection_id) {
            return false;
        }
        self.front = Some(connection_id);
        true
    }

    pub fn open_mcp(&mut self, connection_id: Uuid) -> bool {
        if self.working.contains(&connection_id) || self.mcp.contains(&connection_id) {
            return false;
        }
        self.mcp.push(connection_id);
        true
    }

    pub fn attach_tab(&mut self, tab_id: Uuid, connection_id: Uuid) -> Result<(), AttachTabError> {
        if !self.working.contains(&connection_id) && !self.mcp.contains(&connection_id) {
            return Err(AttachTabError::UnknownConnection);
        }
        match self.tabs.get(&tab_id) {
            Some(existing) if *existing != connection_id => Err(AttachTabError::TabBelongsToOtherConnection),
            Some(_) => Ok(()),
            None => {
                self.tabs.insert(tab_id, connection_id);
                Ok(())
            }
        }
    }

    pub fn detach_tab(&mut self, tab_id: Uuid) -> Option<Uuid> {
        self.tabs.remove(&tab_id)
    }

    pub fn connection_for_tab(&self, tab_id: Uuid) -> Option<Uuid> {
        self.tabs.get(&tab_id).copied()
    }

    pub fn tabs_for_connection(&self, connection_id: Uuid) -> Vec<Uuid> {
        self.tabs
            .iter()
            .filter_map(|(tab, connection)| (*connection == connection_id).then_some(*tab))
            .collect()
    }

    pub fn select_tab(&mut self, tab_id: Uuid) -> Option<Uuid> {
        let connection_id = self.connection_for_tab(tab_id)?;
        if self.working.contains(&connection_id) {
            self.front = Some(connection_id);
        }
        Some(connection_id)
    }

    pub fn names_tabs_with_connection(&self) -> bool {
        self.working.len() > 1
    }

    pub fn disconnect(&mut self, connection_id: Uuid) -> Vec<Uuid> {
        let closed_tabs = self.tabs_for_connection(connection_id);
        for tab_id in &closed_tabs {
            self.tabs.remove(tab_id);
        }
        self.working.retain(|id| *id != connection_id);
        self.mcp.retain(|id| *id != connection_id);
        if self.front == Some(connection_id) {
            self.front = self.working.first().copied();
        }
        closed_tabs
    }

    pub fn restore_keys(&self) -> Vec<Uuid> {
        self.working.clone()
    }
}

#[derive(Default)]
pub struct GuardedSessionMap {
    sessions: HashMap<Uuid, Arc<PolicyGuard>>,
}

impl GuardedSessionMap {
    pub fn insert(&mut self, connection_id: Uuid, guard: Arc<PolicyGuard>) -> Option<Arc<PolicyGuard>> {
        self.sessions.insert(connection_id, guard)
    }

    pub fn get(&self, connection_id: Uuid) -> Option<&Arc<PolicyGuard>> {
        self.sessions.get(&connection_id)
    }

    pub fn remove(&mut self, connection_id: Uuid) -> Option<Arc<PolicyGuard>> {
        self.sessions.remove(&connection_id)
    }

    pub fn contains(&self, connection_id: Uuid) -> bool {
        self.sessions.contains_key(&connection_id)
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

#[cfg(test)]
mod tests;
