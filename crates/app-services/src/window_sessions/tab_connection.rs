use std::collections::VecDeque;

use thiserror::Error;
use uuid::Uuid;

use super::{WindowSavePrompt, WindowSessions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TabConnectionError {
    #[error("tab {tab_id} has no recorded connection")]
    NoRecordedConnection { tab_id: Uuid },
}

impl WindowSessions {
    pub fn resolve_tab_connection(&self, tab_id: Uuid) -> Result<Uuid, TabConnectionError> {
        let connection_id = self
            .connection_for_tab(tab_id)
            .ok_or(TabConnectionError::NoRecordedConnection { tab_id })?;
        if self.is_working(connection_id) || self.is_mcp(connection_id) {
            return Ok(connection_id);
        }
        Err(TabConnectionError::NoRecordedConnection { tab_id })
    }
}

pub fn take_latest_for<T>(stack: &mut VecDeque<(Uuid, T)>, connection_id: Uuid) -> Option<T> {
    let index = stack.iter().rposition(|(owner, _)| *owner == connection_id)?;
    stack.remove(index).map(|(_, item)| item)
}

pub fn disconnect_scope(sessions: &WindowSessions, connection_id: Uuid, window_tabs: &[Uuid]) -> Vec<Uuid> {
    if sessions.working_order().len() <= 1 {
        return window_tabs.to_vec();
    }
    let mut tabs = sessions.tabs_for_connection(connection_id);
    tabs.sort();
    tabs
}

pub fn plan_dirty_tabs(sessions: &WindowSessions, dirty_tabs: &[Uuid]) -> Result<WindowSavePrompt, TabConnectionError> {
    let mut prompt = WindowSavePrompt::default();
    for tab_id in dirty_tabs {
        prompt.mark_dirty(*tab_id, sessions.resolve_tab_connection(*tab_id)?);
    }
    Ok(prompt)
}
