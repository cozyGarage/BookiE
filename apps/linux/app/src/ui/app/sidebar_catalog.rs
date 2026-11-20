use bookie_core::{CatalogObject, CatalogObjectKind};
use relm4::ComponentSender;

use super::{App, AppMsg};
use crate::services::catalog::CatalogOrigin;

const MAX_SIDEBAR_CATALOG_BYTES: usize = 64 * 1024 * 1024;

fn replace_catalog_request(
    requests: &std::cell::RefCell<std::collections::HashMap<CatalogObjectKind, tokio_util::sync::CancellationToken>>,
    parent: &tokio_util::sync::CancellationToken,
    kind: CatalogObjectKind,
) -> tokio_util::sync::CancellationToken {
    let token = parent.child_token();
    if let Some(previous) = requests.borrow_mut().insert(kind, token.clone()) {
        previous.cancel();
    }
    token
}

fn estimated_catalog_bytes_within(objects: &Vec<CatalogObject>, limit: usize) -> Option<usize> {
    let mut bytes = objects.capacity().checked_mul(std::mem::size_of::<CatalogObject>())?;
    if bytes > limit {
        return None;
    }
    for object in objects {
        bytes = bytes
            .checked_add(object.name.capacity())?
            .checked_add(object.schema.as_ref().map_or(0, String::capacity))?
            .checked_add(object.detail.as_ref().map_or(0, String::capacity))?;
        if bytes > limit {
            return None;
        }
    }
    Some(bytes)
}

fn sidebar_catalog_fits(
    catalogs: &std::collections::HashMap<CatalogObjectKind, Vec<CatalogObject>>,
    replacing: CatalogObjectKind,
    incoming: &Vec<CatalogObject>,
    limit: usize,
) -> bool {
    let mut remaining = limit;
    for (kind, objects) in catalogs {
        if *kind == replacing {
            continue;
        }
        let Some(bytes) = estimated_catalog_bytes_within(objects, remaining) else {
            return false;
        };
        remaining -= bytes;
    }
    estimated_catalog_bytes_within(incoming, remaining).is_some()
}

fn replace_catalog_kind(
    catalogs: &mut std::collections::HashMap<CatalogObjectKind, Vec<CatalogObject>>,
    kind: CatalogObjectKind,
    objects: Vec<CatalogObject>,
) -> bool {
    if objects.is_empty() {
        catalogs.remove(&kind).is_some()
    } else {
        catalogs.insert(kind, objects);
        true
    }
}

impl App {
    pub(super) fn reset_sidebar_catalog(&mut self) {
        self.sidebar_catalog.clear();
        self.sidebar_catalog_requests.borrow_mut().clear();
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
        let token = replace_catalog_request(
            &self.sidebar_catalog_requests,
            &self.sidebar_catalog_cancel.borrow(),
            kind,
        );
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
            Ok(objects) => {
                if !objects.is_empty()
                    && !sidebar_catalog_fits(&self.sidebar_catalog, kind, &objects, MAX_SIDEBAR_CATALOG_BYTES)
                {
                    if replace_catalog_kind(&mut self.sidebar_catalog, kind, Vec::new()) {
                        self.repopulate_sidebar(&self.sidebar_tables.clone(), sender.clone());
                    }
                    self.show_toast(&bookie_app_services::tr!(
                        "This catalog exceeds the estimated 64 MiB supplemental sidebar cache limit."
                    ));
                    return;
                }
                let empty = objects.is_empty();
                if replace_catalog_kind(&mut self.sidebar_catalog, kind, objects) {
                    self.repopulate_sidebar(&self.sidebar_tables.clone(), sender);
                }
                if empty {
                    self.show_toast(&bookie_app_services::tr!("Nothing of this kind was found."));
                }
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
        let routine_bytes = estimated_catalog_bytes_within(&catalogs[&CatalogObjectKind::Routine], usize::MAX).unwrap();
        let bytes_without_routine =
            estimated_catalog_bytes_within(&catalogs[&CatalogObjectKind::Trigger], usize::MAX).unwrap();
        let replacement_bytes = estimated_catalog_bytes_within(&replacement, usize::MAX).unwrap();
        let all_bytes = routine_bytes + bytes_without_routine + replacement_bytes;

        assert_eq!(
            estimated_catalog_bytes_within(&replacement, replacement_bytes),
            Some(replacement_bytes)
        );
        assert_eq!(
            estimated_catalog_bytes_within(&replacement, replacement_bytes - 1),
            None
        );

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
            all_bytes - 1,
        ));
        assert!(sidebar_catalog_fits(
            &catalogs,
            CatalogObjectKind::Sequence,
            &replacement,
            all_bytes,
        ));
    }

    #[test]
    fn replacing_a_catalog_request_cancels_the_stale_response() {
        let requests = std::cell::RefCell::new(std::collections::HashMap::new());
        let parent = tokio_util::sync::CancellationToken::new();
        let stale = replace_catalog_request(&requests, &parent, CatalogObjectKind::Sequence);
        let current = replace_catalog_request(&requests, &parent, CatalogObjectKind::Sequence);

        assert!(stale.is_cancelled());
        assert!(!current.is_cancelled());
        parent.cancel();
        assert!(current.is_cancelled());
    }

    #[test]
    fn an_empty_catalog_response_removes_the_cached_kind() {
        let mut catalogs = std::collections::HashMap::from([(CatalogObjectKind::Sequence, vec![object("stale")])]);

        assert!(replace_catalog_kind(
            &mut catalogs,
            CatalogObjectKind::Sequence,
            Vec::new()
        ));
        assert!(catalogs.is_empty());
        assert!(!replace_catalog_kind(
            &mut catalogs,
            CatalogObjectKind::Sequence,
            Vec::new()
        ));
    }
}
