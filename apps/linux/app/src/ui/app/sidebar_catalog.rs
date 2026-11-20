use bookie_core::{CatalogObject, CatalogObjectKind};
use relm4::ComponentSender;

use super::{App, AppMsg};
use crate::services::catalog::CatalogOrigin;

const MAX_SIDEBAR_CATALOG_BYTES: usize = 64 * 1024 * 1024;

fn estimated_catalog_bytes(objects: &Vec<CatalogObject>) -> usize {
    objects
        .capacity()
        .saturating_mul(std::mem::size_of::<CatalogObject>())
        .saturating_add(objects.iter().fold(0, |bytes, object| {
            bytes
                .saturating_add(object.name.capacity())
                .saturating_add(object.schema.as_ref().map_or(0, String::capacity))
                .saturating_add(object.detail.as_ref().map_or(0, String::capacity))
        }))
}

fn sidebar_catalog_fits(
    catalogs: &std::collections::HashMap<CatalogObjectKind, Vec<CatalogObject>>,
    replacing: CatalogObjectKind,
    incoming: &Vec<CatalogObject>,
    limit: usize,
) -> bool {
    let retained_bytes = catalogs
        .iter()
        .filter(|(kind, _)| **kind != replacing)
        .fold(0usize, |bytes, (_, objects)| {
            bytes.saturating_add(estimated_catalog_bytes(objects))
        });
    retained_bytes.saturating_add(estimated_catalog_bytes(incoming)) <= limit
}

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
        let hidden_schemas = self.hidden_schema_names();
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let reply = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token.clone());
                    let result = crate::services::catalog::list_objects_excluding(
                        conn.as_ref(),
                        kind,
                        None,
                        &hidden_schemas,
                        &control,
                    )
                    .await
                    .map_err(|error| crate::services::error_text::driver_message(&error));
                    reply.input(AppMsg::SidebarCatalogLoaded {
                        origin,
                        kind,
                        request: token,
                        result,
                    });
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_sidebar_catalog_loaded(
        &mut self,
        origin: &CatalogOrigin,
        kind: CatalogObjectKind,
        request: &tokio_util::sync::CancellationToken,
        result: Result<Vec<CatalogObject>, String>,
        sender: ComponentSender<Self>,
    ) {
        if request.is_cancelled() || !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        match result {
            Ok(objects) if objects.is_empty() => {
                self.show_toast(&bookie_app_services::tr!("Nothing of this kind was found."));
            }
            Ok(objects) => {
                if !sidebar_catalog_fits(&self.sidebar_catalog, kind, &objects, MAX_SIDEBAR_CATALOG_BYTES) {
                    self.show_toast(&bookie_app_services::tr!(
                        "The catalog exceeds the 64 MiB sidebar limit."
                    ));
                    return;
                }
                self.sidebar_catalog.insert(kind, objects);
                self.repopulate_sidebar(&self.sidebar_tables.clone(), sender);
            }
            Err(message) => self.show_toast(&message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(detail: &str) -> CatalogObject {
        CatalogObject {
            kind: CatalogObjectKind::Routine,
            schema: Some("public".into()),
            name: "routine".into(),
            detail: Some(detail.into()),
        }
    }

    #[test]
    fn catalog_budget_counts_retained_kinds_and_replaces_the_loaded_kind() {
        let catalogs = std::collections::HashMap::from([
            (CatalogObjectKind::Routine, vec![object("old")]),
            (CatalogObjectKind::Trigger, vec![object("trigger")]),
        ]);
        let replacement = vec![object("new")];
        let bytes_without_routine = estimated_catalog_bytes(&catalogs[&CatalogObjectKind::Trigger]);

        let replacement_bytes = estimated_catalog_bytes(&replacement);
        assert!(sidebar_catalog_fits(
            &catalogs,
            CatalogObjectKind::Routine,
            &replacement,
            bytes_without_routine + replacement_bytes,
        ));
        assert!(!sidebar_catalog_fits(
            &catalogs,
            CatalogObjectKind::Sequence,
            &replacement,
            bytes_without_routine + replacement_bytes - 1,
        ));
    }
}
