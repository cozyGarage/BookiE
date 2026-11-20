use bookie_gpui::shell::Tab;

use super::*;

const KEPT: usize = 10;

#[derive(Default)]
pub(crate) struct ClosedTabs(Vec<Tab>);

impl ClosedTabs {
    pub(crate) fn remember(&mut self, tab: Tab) {
        if tab == Tab::Home {
            return;
        }
        self.0.retain(|kept| *kept != tab);
        self.0.push(tab);
        if self.0.len() > KEPT {
            self.0.remove(0);
        }
    }

    pub(crate) fn take(&mut self) -> Option<Tab> {
        self.0.pop()
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Default)]
pub(crate) struct TabRecency {
    current: Option<Tab>,
    previous: Option<Tab>,
}

impl TabRecency {
    pub(crate) fn observe(&mut self, tab: &Tab) {
        if self.current.as_ref() != Some(tab) {
            self.previous = self.current.replace(tab.clone());
        }
    }

    pub(crate) fn previous(&self) -> Option<&Tab> {
        self.previous.as_ref()
    }
}

impl GridView {
    pub(crate) fn observe_tab(&mut self) {
        let tab = self.tabs.current().clone();
        self.search.recent.observe(&tab);
    }

    pub(crate) fn switch_to_previous_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self
            .search
            .recent
            .previous()
            .and_then(|previous| self.tabs.items.iter().position(|tab| tab == previous));
        let Some(index) = wanted else {
            self.status = "There is no previous tab to switch to".into();
            return;
        };
        self.tabs.active = index;
        self.sync_grid_owner();
        self.show_current_table(window, cx);
        cx.notify();
    }

    pub(crate) fn remember_closed_tab(&mut self, index: usize) {
        if let Some(tab) = self.tabs.items.get(index).cloned() {
            self.search.closed.remember(tab);
        }
    }

    pub(crate) fn show_current_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Tab::Table(table) = self.tabs.current().clone()
            && table != self.catalog.grid_table
        {
            self.open_table_named(&table, window, cx);
        }
    }

    pub(crate) fn recent_tab_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "recent-tab",
            label: "Switch to the previous tab".to_string(),
            enabled: self.tabs.items.len() > 1,
        }]
    }

    pub(crate) fn closed_tab_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "reopen-tab",
            label: "Reopen the last closed tab".to_string(),
            enabled: self.search.closed.len() > 0,
        }]
    }

    pub(crate) fn reopen_closed_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.search.closed.take() else {
            self.status = "There is no closed tab to reopen".into();
            return;
        };
        match tab {
            Tab::Table(table) => self.open_table_named(&table, window, cx),
            Tab::Query => self.tabs.open_query(),
            Tab::Settings => self.tabs.open_settings(),
            Tab::History => self.tabs.open_history(),
            Tab::AuditLog => self.tabs.open_audit_log(),
            Tab::NewConnection => self.tabs.open_new_connection(),
            Tab::Diagram => self.open_schema_diagram(cx),
            Tab::Home => return,
        }
        self.sync_grid_owner();
        cx.notify();
    }
}

#[cfg(test)]
#[path = "closed_tabs_ui_tests.rs"]
mod tests;
