use std::collections::HashMap;

use bookie_app_services::window_sessions::{GuardedSessionMap, WindowSessions};
use bookie_core::TableInfo;
use bookie_policy::Principal;
use relm4::ComponentSender;
use relm4::adw::prelude::*;
use uuid::Uuid;

use super::types::read_workspace_tab_id;
use super::{App, AppMsg};

#[derive(Clone)]
pub(super) struct SessionCatalog {
    pub tables: Vec<TableInfo>,
    pub views: Vec<TableInfo>,
    pub database: String,
}

#[derive(Default)]
pub(super) struct MultiSessionState {
    pub sessions: WindowSessions,
    pub guards: GuardedSessionMap,
    pub catalogs: HashMap<Uuid, SessionCatalog>,
}

pub(super) fn opens_additional_session(connected: bool, front: Option<Uuid>, target: Uuid) -> bool {
    connected && front.is_some_and(|id| id != target)
}

pub(super) fn connection_aware_tab_title(
    base: &str,
    connection_name: Option<&str>,
    name_with_connection: bool,
) -> String {
    match (name_with_connection, connection_name) {
        (true, Some(name)) if !name.is_empty() => format!("{base} · {name}"),
        _ => base.to_string(),
    }
}

impl App {
    pub(super) fn register_working_session(
        &mut self,
        id: Uuid,
        tables: Vec<TableInfo>,
        views: Vec<TableInfo>,
        database: String,
    ) {
        self.multi.sessions.open_working(id);
        if let Some(guard) = self.database.guard(id, Principal::human_gui()) {
            self.multi.guards.insert(id, guard);
        }
        self.multi.catalogs.insert(
            id,
            SessionCatalog {
                tables,
                views,
                database,
            },
        );
    }

    pub(super) fn claim_orphan_tabs(&mut self, connection_id: Uuid) {
        let tab_ids: Vec<Uuid> = self.workspace_tabs.borrow().keys().copied().collect();
        for tab_id in tab_ids {
            if self.multi.sessions.connection_for_tab(tab_id).is_some() {
                continue;
            }
            let _ = self.multi.sessions.attach_tab(tab_id, connection_id);
        }
    }

    pub(super) fn sync_orphan_tabs_to_front(&mut self) {
        let Some(front) = self.multi.sessions.front() else {
            return;
        };
        self.claim_orphan_tabs(front);
    }

    pub(super) fn focus_working_session(&mut self, connection_id: Uuid, sender: ComponentSender<Self>) {
        if !self.multi.sessions.is_working(connection_id) {
            return;
        }
        if self.connection_id == Some(connection_id) {
            return;
        }
        self.rebind_front_session(connection_id, sender);
        self.select_first_tab_for_connection(connection_id);
    }

    pub(super) fn rebind_front_session(&mut self, connection_id: Uuid, sender: ComponentSender<Self>) {
        if !self.multi.sessions.focus_working(connection_id) {
            return;
        }
        self.connection_id = Some(connection_id);
        self.overview_front_connection_id.set(Some(connection_id));
        self.workspace_save_token = Some(self.workspace.connection_save_token(connection_id));
        let metadata = self.database.metadata(connection_id);
        self.current_driver_id = metadata.as_ref().map(|metadata| metadata.driver_id.clone());
        self.read_only = metadata.as_ref().map(|metadata| metadata.read_only).unwrap_or(false);
        self.read_only_badge.set_visible(self.read_only);
        let catalog = self.multi.catalogs.get(&connection_id).cloned();
        self.connected_database = catalog.as_ref().map(|catalog| catalog.database.clone());
        if let Some(catalog) = catalog {
            self.sidebar_views = catalog.views;
            self.reset_sidebar_catalog();
            self.repopulate_sidebar(&catalog.tables);
            self.rebuild_schema_filter_popover(&sender);
            self.refresh_schema_index();
        }
        if let Some(driver_id) = self.current_driver_id.clone() {
            self.show_database_switcher(&driver_id, connection_id, &sender);
        }
        self.refresh_multi_session_tab_titles();
        self.refresh_window_title();
        self.sync_sidebar_selection();
    }

    pub(super) fn follow_selected_tab_session(&mut self, sender: ComponentSender<Self>) {
        self.sync_orphan_tabs_to_front();
        let Some(tab_id) = self.selected_workspace_tab_id() else {
            return;
        };
        let Some(connection_id) = self.multi.sessions.select_tab(tab_id) else {
            return;
        };
        if self.connection_id == Some(connection_id) {
            return;
        }
        if !self.multi.sessions.is_working(connection_id) {
            return;
        }
        self.rebind_front_session(connection_id, sender);
    }

    pub(super) fn refresh_multi_session_tab_titles(&self) {
        let name_with_connection = self.multi.sessions.names_tabs_with_connection();
        for (tab_id, tab) in self.workspace_tabs.borrow().iter() {
            let page = match tab {
                super::WorkspaceTab::Editor(slot) => &slot.page,
                super::WorkspaceTab::Table(slot) => &slot.page,
                super::WorkspaceTab::Structure(slot) => &slot.page,
            };
            let base =
                strip_connection_title_suffix(page.title().as_str(), self.multi.sessions.working_order(), |id| {
                    self.database.metadata(id).map(|metadata| metadata.name)
                });
            let connection_name = self
                .multi
                .sessions
                .connection_for_tab(*tab_id)
                .and_then(|id| self.database.metadata(id))
                .map(|metadata| metadata.name);
            let title = connection_aware_tab_title(&base, connection_name.as_deref(), name_with_connection);
            page.set_title(&title);
        }
    }

    pub(super) fn clear_multi_session_state(&mut self) {
        self.overview_front_connection_id.set(None);
        for id in self.multi.sessions.restore_keys() {
            self.database.close(id);
            crate::services::window_registry::unregister(id);
            self.multi.guards.remove(id);
        }
        self.multi = MultiSessionState::default();
    }

    pub(super) fn disconnect_front_keeping_others(&mut self, sender: ComponentSender<Self>) -> bool {
        let Some(front) = self.connection_id else {
            return false;
        };
        let next = self
            .multi
            .sessions
            .working_order()
            .iter()
            .copied()
            .find(|working| *working != front);
        let Some(next) = next else {
            return false;
        };
        if self.window_state.borrow().last_connection_id == Some(front) {
            self.set_last_connection_id(Some(next), sender.clone());
        }
        self.drop_working_session(front, sender);
        true
    }

    pub(super) fn drop_working_session(&mut self, connection_id: Uuid, sender: ComponentSender<Self>) {
        let closed_tabs = self.multi.sessions.disconnect(connection_id);
        self.multi.guards.remove(connection_id);
        self.multi.catalogs.remove(&connection_id);
        self.database.close(connection_id);
        crate::services::window_registry::unregister(connection_id);
        for tab_id in closed_tabs {
            sender.input(AppMsg::FinishCloseWorkspaceTab(tab_id));
        }
        match self.multi.sessions.front() {
            Some(front) => {
                self.connected = true;
                self.rebind_front_session(front, sender);
            }
            None => {
                self.connection_id = None;
                self.overview_front_connection_id.set(None);
                self.connected = false;
                self.workspace_save_token = None;
                self.connected_database = None;
                self.current_driver_id = None;
                self.read_only = false;
                self.read_only_badge.set_visible(false);
                self.databases_button.set_visible(false);
                self.disconnect_action.set_enabled(false);
                self.refresh_window_title();
            }
        }
    }

    fn select_first_tab_for_connection(&self, connection_id: Uuid) {
        let Some(tab_view) = self.workspace_tab_view.as_ref() else {
            return;
        };
        let Some(tab_id) = self
            .multi
            .sessions
            .tabs_for_connection(connection_id)
            .into_iter()
            .next()
        else {
            return;
        };
        let pages = tab_view.pages();
        for index in 0..pages.n_items() {
            let Some(page) = pages.item(index).and_downcast::<relm4::adw::TabPage>() else {
                continue;
            };
            if read_workspace_tab_id(&page) == Some(tab_id) {
                tab_view.set_selected_page(&page);
                return;
            }
        }
    }
}

fn strip_connection_title_suffix(
    title: &str,
    working: &[Uuid],
    mut name_for: impl FnMut(Uuid) -> Option<String>,
) -> String {
    for id in working {
        let Some(name) = name_for(*id) else {
            continue;
        };
        let suffix = format!(" · {name}");
        if let Some(base) = title.strip_suffix(&suffix) {
            return base.to_string();
        }
    }
    title.to_string()
}

pub(super) fn overview_new_tab_connection(front: &std::cell::Cell<Option<Uuid>>) -> Option<Uuid> {
    front.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additional_session_is_detected_only_when_front_differs() {
        let front = Uuid::new_v4();
        let other = Uuid::new_v4();
        assert!(!opens_additional_session(false, None, other));
        assert!(!opens_additional_session(true, Some(front), front));
        assert!(opens_additional_session(true, Some(front), other));
    }

    #[test]
    fn tab_titles_include_connection_names_when_requested() {
        assert_eq!(
            connection_aware_tab_title("orders", Some("prod"), true),
            "orders · prod"
        );
        assert_eq!(connection_aware_tab_title("orders", Some("prod"), false), "orders");
        assert_eq!(connection_aware_tab_title("orders", None, true), "orders");
    }

    #[test]
    fn connection_title_suffix_strips_known_names() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let title = strip_connection_title_suffix("orders · prod", &[first, second], |id| {
            if id == first {
                Some("prod".into())
            } else {
                Some("staging".into())
            }
        });
        assert_eq!(title, "orders");
    }

    #[test]
    fn overview_new_editor_resolves_the_current_front_session() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let front = std::cell::Cell::new(Some(first));
        let create_tab = || overview_new_tab_connection(&front);

        assert_eq!(create_tab(), Some(first));
        front.set(Some(second));
        assert_eq!(create_tab(), Some(second));
    }
}
