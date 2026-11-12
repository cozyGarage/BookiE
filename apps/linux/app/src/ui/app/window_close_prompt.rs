use bookie_app_services::window_sessions::plan_dirty_tabs;
use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw};
use uuid::Uuid;

use super::{App, AppMsg};

pub(super) fn save_prompt_body(connection_names: &[String]) -> String {
    match connection_names {
        [] | [_] if connection_names.is_empty() => bookie_app_services::tr!(
            "One or more tabs have unsaved changes. They will be permanently lost if you discard them."
        ),
        [name] => bookie_app_services::tr!(
            "The tabs of “{connections}” have unsaved changes. They will be permanently lost if you discard them."
        )
        .replace("{connections}", name),
        names => bookie_app_services::tr!(
            "Tabs of these connections have unsaved changes: {connections}. Each is saved to its own connection. They will be permanently lost if you discard them."
        )
        .replace("{connections}", &names.join(", ")),
    }
}

pub(super) fn dirty_tab_ids(browse: &[Uuid], structure: &[Uuid]) -> Vec<Uuid> {
    let mut dirty = browse.to_vec();
    for id in structure {
        if !dirty.contains(id) {
            dirty.push(*id);
        }
    }
    dirty
}

impl App {
    pub(super) fn prompt_window_close_with_pending(&mut self, sender: ComponentSender<Self>) {
        let tab_ids = self.this_window_tab_ids();
        let browse = self.change_trackers.pending_among(tab_ids.iter().copied());
        let structure = self.structure_trackers.pending_among(tab_ids);
        let dirty = dirty_tab_ids(&browse, &structure);
        let prompt = match plan_dirty_tabs(&self.multi.sessions, &dirty) {
            Ok(prompt) => prompt,
            Err(error) => {
                tracing::warn!(%error, "window close refused: unsaved tab without a connection");
                self.show_error_alert(
                    &bookie_app_services::tr!("Cannot close the window yet"),
                    &bookie_app_services::tr!(
                        "A tab with unsaved changes is not linked to an open connection, so its changes cannot be saved. Discard that tab's changes first."
                    ),
                );
                return;
            }
        };
        let names: Vec<String> = prompt
            .connections_in_order(self.multi.sessions.working_order())
            .into_iter()
            .filter_map(|id| self.database.metadata(id).map(|metadata| metadata.name))
            .collect();
        let dialog = adw::AlertDialog::new(None, None);
        dialog.set_heading(Some(&bookie_app_services::tr!("Save changes before closing?")));
        dialog.set_body(&save_prompt_body(&names));
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("discard", &bookie_app_services::tr!("Discard"));
        dialog.add_response("save", &bookie_app_services::tr!("Save"));
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        let handles = self.window_close_handles(browse, structure, sender);
        dialog.connect_response(None, move |_, response| handles.respond(response));
        dialog.present(Some(&self.window));
    }

    fn window_close_handles(
        &self,
        browse: Vec<Uuid>,
        structure: Vec<Uuid>,
        sender: ComponentSender<Self>,
    ) -> WindowCloseResponder {
        WindowCloseResponder {
            browse,
            structure,
            sender,
            window: self.window.clone(),
            force_close: self.force_close.clone(),
            close_after_save: self.close_after_save.clone(),
            close_window_after_save: self.close_window_after_save.clone(),
            change_trackers: self.change_trackers.clone(),
            structure_trackers: self.structure_trackers.clone(),
        }
    }
}

struct WindowCloseResponder {
    browse: Vec<Uuid>,
    structure: Vec<Uuid>,
    sender: ComponentSender<App>,
    window: adw::ApplicationWindow,
    force_close: std::rc::Rc<std::cell::Cell<bool>>,
    close_after_save: std::rc::Rc<std::cell::RefCell<bookie_app_services::workspace::CloseAfterSave>>,
    close_window_after_save: std::rc::Rc<std::cell::Cell<bool>>,
    change_trackers: crate::services::change_tracker::WorkspaceChangeTrackers,
    structure_trackers: crate::services::structure_tracker::WorkspaceStructureTrackers,
}

impl WindowCloseResponder {
    fn respond(&self, response: &str) {
        match response {
            "discard" => self.discard(),
            "save" => self.save_all(),
            _ => {}
        }
    }

    fn discard(&self) {
        for tab_id in &self.browse {
            self.change_trackers.with_tab(*tab_id, |tracker| tracker.clear());
        }
        for tab_id in &self.structure {
            self.structure_trackers.with_tab(*tab_id, |tracker| tracker.clear());
        }
        self.force_close.set(true);
        self.window.close();
    }

    fn save_all(&self) {
        {
            let mut pending = self.close_after_save.borrow_mut();
            for id in self.browse.iter().chain(&self.structure) {
                pending.begin(*id, 1);
            }
        }
        self.close_window_after_save.set(true);
        for id in &self.browse {
            self.sender.input(AppMsg::SaveActiveBrowseTabById(*id));
        }
        for id in &self.structure {
            self.sender.input(AppMsg::SaveActiveStructureTabById(*id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_with_row_and_structure_changes_is_listed_once() {
        let [a, b] = [Uuid::new_v4(), Uuid::new_v4()];
        assert_eq!(dirty_tab_ids(&[a, b], &[b]), vec![a, b]);
        assert_eq!(dirty_tab_ids(&[a], &[b]), vec![a, b]);
    }

    #[test]
    fn the_close_prompt_names_every_connection_with_unsaved_tabs() {
        let body = save_prompt_body(&["prod".into(), "staging".into()]);
        assert!(body.contains("prod, staging"));
        assert!(save_prompt_body(&["prod".into()]).contains("prod"));
        assert!(!save_prompt_body(&[]).contains('{'));
    }
}
