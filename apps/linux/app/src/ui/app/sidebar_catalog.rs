use bookie_core::{CatalogObject, CatalogObjectKind};
use relm4::ComponentSender;

use super::{App, AppMsg};
use crate::services::catalog::CatalogOrigin;

impl App {
    pub(super) fn reset_sidebar_catalog(&mut self) {
        self.sidebar_catalog.clear();
        let fresh = tokio_util::sync::CancellationToken::new();
        self.sidebar_catalog_cancel.replace(fresh).cancel();
    }

    pub(super) fn on_load_sidebar_catalog(&self, kind: CatalogObjectKind, sender: &ComponentSender<Self>) {
        let (Some(origin), Some(conn)) = (
            CatalogOrigin::capture(self.connection_id, &self.database),
            self.window_connection(),
        ) else {
            return;
        };
        let token = self.sidebar_catalog_cancel.borrow().child_token();
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let reply = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    let result = conn
                        .list_objects_controlled(kind, None, &control)
                        .await
                        .map_err(|error| crate::services::error_text::driver_message(&error));
                    reply.input(AppMsg::SidebarCatalogLoaded { origin, kind, result });
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_sidebar_catalog_loaded(
        &mut self,
        origin: &CatalogOrigin,
        kind: CatalogObjectKind,
        result: Result<Vec<CatalogObject>, String>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        match result {
            Ok(objects) if objects.is_empty() => {
                self.show_toast(&crate::tr!("Nothing of this kind was found."));
            }
            Ok(objects) => {
                self.sidebar_catalog.insert(kind, objects);
                self.repopulate_sidebar(&self.sidebar_tables.clone());
            }
            Err(message) => self.show_toast(&message),
        }
    }
}
