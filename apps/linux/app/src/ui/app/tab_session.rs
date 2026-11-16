use std::sync::Arc;

use bookie_app_services::window_sessions::TabConnectionError;
use bookie_core::Connection;
use uuid::Uuid;

use relm4::ComponentSender;

use super::{App, AppMsg};
use crate::services::catalog::CatalogOrigin;

impl App {
    pub(super) fn resolve_tab_connection(&self, tab_id: Uuid) -> Result<Uuid, TabConnectionError> {
        self.multi.sessions.resolve_tab_connection(tab_id)
    }

    pub(super) fn attach_tab_to_front(&mut self, tab_id: Uuid) {
        self.attach_tab_to(tab_id, self.connection_id);
    }

    pub(super) fn attach_tab_to(&mut self, tab_id: Uuid, connection_id: Option<Uuid>) {
        let Some(connection_id) = connection_id else {
            tracing::warn!(%tab_id, "tab created without a connection");
            return;
        };
        if let Err(error) = self.multi.sessions.attach_tab(tab_id, connection_id) {
            tracing::warn!(?error, %tab_id, "tab could not be linked to its connection");
        }
    }

    pub(super) fn announce_save_failure(&self, tab_id: Uuid, message: &str) {
        if !self.multi.sessions.names_tabs_with_connection() {
            return;
        }
        let name = self
            .resolve_tab_connection(tab_id)
            .ok()
            .and_then(|id| self.database.metadata(id))
            .map(|metadata| metadata.name);
        let Some(name) = name else {
            return;
        };
        self.show_toast(
            &bookie_app_services::tr!("Saving to “{connection}” failed: {message}")
                .replace("{connection}", &name)
                .replace("{message}", message),
        );
    }

    pub(super) fn teardown_includes(&self, action: super::SessionTeardownAction, tab_id: Uuid) -> bool {
        match action {
            super::SessionTeardownAction::CloseTab(target) => target == tab_id,
            super::SessionTeardownAction::Disconnect => self.disconnect_scope_tabs().contains(&tab_id),
            _ => true,
        }
    }

    pub(super) fn disconnect_scope_tabs(&self) -> Vec<Uuid> {
        let window_tabs = self.this_window_tab_ids();
        match self.connection_id {
            Some(front) => {
                bookie_app_services::window_sessions::disconnect_scope(&self.multi.sessions, front, &window_tabs)
            }
            None => window_tabs,
        }
    }

    pub(super) fn disconnect_discard_body(&self) -> String {
        let name = self
            .connection_id
            .filter(|_| self.multi.sessions.working_order().len() > 1)
            .and_then(|id| self.database.metadata(id))
            .map(|metadata| metadata.name);
        match name {
            Some(name) => bookie_app_services::tr!(
                "Disconnecting will close every tab of “{connection}” and drop its unsaved row edits and DDL changes. Other connections stay open."
            )
            .replace("{connection}", &name),
            None => bookie_app_services::tr!(
                "Disconnecting will close every tab and drop every unsaved row edit and DDL change."
            ),
        }
    }

    pub(super) fn disconnect_has_pending(&self) -> bool {
        let scope = self.disconnect_scope_tabs();
        self.change_trackers.any_pending_in(scope.iter().copied()) || self.structure_trackers.any_pending_in(scope)
    }

    pub(super) fn tab_belongs_to(&self, tab_id: Uuid, connection_id: Option<Uuid>) -> bool {
        connection_id.is_some() && self.multi.sessions.connection_for_tab(tab_id) == connection_id
    }

    pub(super) fn tab_connection(&self, tab_id: Uuid) -> Option<Arc<dyn Connection>> {
        let id = self.reported_tab_connection(tab_id)?;
        self.database.get(id)
    }

    pub(super) fn tab_origin(&self, tab_id: Uuid) -> Option<CatalogOrigin> {
        let id = self.reported_tab_connection(tab_id)?;
        CatalogOrigin::capture(Some(id), &self.database)
    }

    pub(super) fn tab_driver_id(&self, tab_id: Uuid) -> Option<String> {
        let id = self.resolve_tab_connection(tab_id).ok()?;
        self.database.metadata(id).map(|metadata| metadata.driver_id)
    }

    pub(super) fn tab_timeout_secs(&self, tab_id: Uuid) -> u32 {
        let id = self.resolve_tab_connection(tab_id).ok();
        crate::services::operation_control::timeout_for(&self.preferences, &self.database, id)
    }

    pub(super) fn tab_owns_origin(&self, tab_id: Uuid, origin: &CatalogOrigin) -> bool {
        let Ok(id) = self.resolve_tab_connection(tab_id) else {
            return false;
        };
        origin.matches(Some(id), self.database.identity(origin.id).as_ref())
    }

    pub(super) fn report_refused_tab_save(&self, tab_id: Uuid, sender: &ComponentSender<Self>) {
        sender.input(AppMsg::StructureSaveFailed(
            tab_id,
            bookie_app_services::tr!("This tab is not linked to an open connection. Reopen it from the sidebar."),
        ));
    }

    fn reported_tab_connection(&self, tab_id: Uuid) -> Option<Uuid> {
        match self.resolve_tab_connection(tab_id) {
            Ok(id) => Some(id),
            Err(error) => {
                tracing::warn!(%error, "tab refused: no connection recorded");
                self.show_toast(&bookie_app_services::tr!(
                    "This tab is not linked to an open connection. Reopen it from the sidebar."
                ));
                None
            }
        }
    }
}
