use bookie_app_services::workspace::{close_other_page_indexes, close_page_indexes_to_right};
use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw};
use uuid::Uuid;

use super::types::read_workspace_tab_id;
use super::{App, AppMsg, SessionTeardownAction, WorkspaceTab};

struct PendingWorkspaceTabChanges {
    browse: bool,
    structure: bool,
    file: bool,
}

impl App {
    pub(super) fn close_workspace_tab_by_id(&mut self, id: Uuid, sender: ComponentSender<Self>) {
        let Some(tab_view) = self.workspace_tab_view.clone() else {
            return;
        };
        if self.workspace_tab_has_pending_changes(id) {
            self.prompt_close_workspace_tab(id, &tab_view, sender);
            return;
        }
        self.close_workspace_tab_after_pending(id, &tab_view, sender);
    }

    fn workspace_tab_has_pending_changes(&self, id: Uuid) -> bool {
        self.workspace_tabs
            .borrow()
            .get(&id)
            .and_then(|tab| match tab {
                WorkspaceTab::Structure(slot) => self
                    .structure_trackers
                    .with_tab_ref(slot.id, |tracker| tracker.has_pending()),
                WorkspaceTab::Table(slot) => {
                    let data_dirty = self
                        .change_trackers
                        .with_tab_ref(slot.id, |tracker| tracker.has_pending())
                        .unwrap_or(false);
                    let structure_dirty = self
                        .structure_trackers
                        .with_tab_ref(slot.id, |tracker| tracker.has_pending())
                        .unwrap_or(false);
                    Some(data_dirty || structure_dirty)
                }
                WorkspaceTab::Editor(slot) => Some(slot.has_unsaved_file_changes()),
            })
            .unwrap_or(false)
    }

    fn prompt_close_workspace_tab(&self, id: Uuid, tab_view: &adw::TabView, sender: ComponentSender<Self>) {
        let page = self.workspace_tabs.borrow().get(&id).map(|tab| match tab {
            WorkspaceTab::Editor(slot) => slot.page.clone(),
            WorkspaceTab::Structure(slot) => slot.page.clone(),
            WorkspaceTab::Table(slot) => slot.page.clone(),
        });
        let Some(page) = page else { return };
        let raw_title = page.title().to_string();
        let tab_label = raw_title.strip_prefix("• ").unwrap_or(&raw_title).to_string();
        let dialog = adw::AlertDialog::new(None, None);
        dialog.set_heading(Some(
            &bookie_app_services::tr!("Save changes to “{name}”?").replace("{name}", &tab_label),
        ));
        dialog.set_body(&bookie_app_services::tr!(
            "Unsaved changes will be permanently lost if you discard them."
        ));
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("discard", &bookie_app_services::tr!("Discard"));
        dialog.add_response("save", &bookie_app_services::tr!("Save"));
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        self.connect_pending_workspace_tab_close_response(id, &dialog, tab_view, &page, sender);
        dialog.present(Some(&self.window));
    }

    fn connect_pending_workspace_tab_close_response(
        &self,
        id: Uuid,
        dialog: &adw::AlertDialog,
        tab_view: &adw::TabView,
        page: &adw::TabPage,
        sender: ComponentSender<Self>,
    ) {
        let changes = self.pending_workspace_tab_changes(id);
        let tab_view_for_resp = tab_view.clone();
        let page_for_resp = page.clone();
        let sender_for_resp = sender;
        let close_after_save = self.close_after_save.clone();
        let structure_trackers = self.structure_trackers.clone();
        let change_trackers = self.change_trackers.clone();
        dialog.connect_response(None, move |_, response| match response {
            "discard" => {
                if changes.browse {
                    change_trackers.with_tab(id, |tracker| tracker.clear());
                }
                if changes.structure {
                    structure_trackers.with_tab(id, |tracker| tracker.clear());
                }
                sender_for_resp.input(AppMsg::CloseDiscardedWorkspaceTab(id));
            }
            "save" => {
                let pending_saves = u32::from(changes.browse) + u32::from(changes.structure) + u32::from(changes.file);
                if pending_saves > 0 {
                    close_after_save.borrow_mut().begin(id, pending_saves);
                }
                tab_view_for_resp.close_page_finish(&page_for_resp, false);
                if changes.browse {
                    sender_for_resp.input(AppMsg::SaveActiveBrowseTabById(id));
                }
                if changes.structure {
                    sender_for_resp.input(AppMsg::SaveActiveStructureTabById(id));
                }
                if changes.file {
                    sender_for_resp.input(AppMsg::SaveEditorFile {
                        tab: id,
                        overwrite: false,
                    });
                }
            }
            _ => tab_view_for_resp.close_page_finish(&page_for_resp, false),
        });
    }

    fn pending_workspace_tab_changes(&self, id: Uuid) -> PendingWorkspaceTabChanges {
        let browse = self
            .change_trackers
            .with_tab_ref(id, |tracker| tracker.has_pending())
            .unwrap_or(false);
        let structure = self
            .structure_trackers
            .with_tab_ref(id, |tracker| tracker.has_pending())
            .unwrap_or(false);
        let file = matches!(
            self.workspace_tabs.borrow().get(&id),
            Some(WorkspaceTab::Editor(slot)) if slot.has_unsaved_file_changes()
        );
        PendingWorkspaceTabChanges {
            browse,
            structure,
            file,
        }
    }

    pub(super) fn confirm_session_teardown(
        &self,
        sender: ComponentSender<Self>,
        action: SessionTeardownAction,
        heading: &str,
        body: &str,
        accept_label: &str,
    ) -> bool {
        let has_open_transaction = self.workspace_tabs.borrow().values().any(|tab| match tab {
            WorkspaceTab::Editor(slot) => {
                relm4::ComponentController::model(&slot.controller).session_transaction_open()
            }
            _ => false,
        });
        if !has_open_transaction {
            return false;
        }

        let dialog = adw::AlertDialog::new(Some(heading), Some(body));
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("confirm", accept_label);
        dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let sender = sender.clone();
        dialog.connect_response(None, move |_, response| {
            let message = if response == "confirm" {
                AppMsg::ConfirmSessionTeardown(action)
            } else {
                AppMsg::CancelSessionTeardown(action)
            };
            sender.input(message);
        });
        dialog.present(Some(&self.window));
        true
    }

    pub(super) fn prepare_window_close(&mut self, sender: ComponentSender<Self>) {
        if !self.confirm_session_teardown(
            sender.clone(),
            SessionTeardownAction::WindowClose,
            &bookie_app_services::tr!("Close with open session transactions?"),
            &bookie_app_services::tr!("Open transactions will be rolled back before the window closes."),
            &bookie_app_services::tr!("Roll Back and Close"),
        ) {
            self.prepare_editor_sessions(SessionTeardownAction::WindowClose, sender);
        }
    }

    pub(super) fn request_workspace_tab_close(&self, id: Uuid) {
        let Some(tab_view) = self.workspace_tab_view.as_ref() else {
            return;
        };
        let page = self.workspace_tabs.borrow().get(&id).map(|tab| match tab {
            WorkspaceTab::Editor(slot) => slot.page.clone(),
            WorkspaceTab::Structure(slot) => slot.page.clone(),
            WorkspaceTab::Table(slot) => slot.page.clone(),
        });
        if let Some(page) = page {
            tab_view.close_page(&page);
        }
    }

    pub(super) fn close_active_workspace_tab(&mut self, sender: ComponentSender<Self>) {
        let Some(tab_view) = self.workspace_tab_view.as_ref() else {
            self.window.close();
            return;
        };
        let Some(page) = tab_view.selected_page() else {
            self.window.close();
            return;
        };
        tab_view.close_page(&page);
        let _ = sender;
    }

    pub(super) fn close_other_workspace_tabs(&mut self, keep_id: Uuid, _sender: ComponentSender<Self>) {
        let Some(tab_view) = self.workspace_tab_view.clone() else {
            return;
        };
        let pages = tab_view.pages();
        let mut page_ids = Vec::with_capacity(pages.n_items() as usize);
        let mut tab_pages = Vec::with_capacity(pages.n_items() as usize);
        for index in 0..pages.n_items() {
            let page = pages.item(index).and_downcast::<adw::TabPage>();
            page_ids.push(page.as_ref().and_then(read_workspace_tab_id));
            tab_pages.push(page);
        }
        for index in close_other_page_indexes(&page_ids, keep_id) {
            if let Some(page) = tab_pages.get(index).and_then(Option::as_ref) {
                tab_view.close_page(page);
            }
        }
    }

    pub(super) fn close_workspace_tabs_to_right(&mut self, anchor_id: Uuid, _sender: ComponentSender<Self>) {
        let Some(tab_view) = self.workspace_tab_view.clone() else {
            return;
        };
        let pages = tab_view.pages();
        let mut page_ids = Vec::with_capacity(pages.n_items() as usize);
        let mut tab_pages = Vec::with_capacity(pages.n_items() as usize);
        for index in 0..pages.n_items() {
            let page = pages.item(index).and_downcast::<adw::TabPage>();
            page_ids.push(page.as_ref().and_then(read_workspace_tab_id));
            tab_pages.push(page);
        }
        for index in close_page_indexes_to_right(&page_ids, anchor_id) {
            if let Some(page) = tab_pages.get(index).and_then(Option::as_ref) {
                tab_view.close_page(page);
            }
        }
    }

    pub(super) fn confirm_close_with_open_transaction(
        &self,
        id: Uuid,
        tab_view: &adw::TabView,
        sender: ComponentSender<Self>,
    ) -> bool {
        let page = match self.workspace_tabs.borrow().get(&id) {
            Some(WorkspaceTab::Editor(slot))
                if relm4::ComponentController::model(&slot.controller).session_transaction_open() =>
            {
                slot.page.clone()
            }
            _ => return false,
        };
        let dialog = adw::AlertDialog::new(
            Some(&bookie_app_services::tr!("Close with an open transaction?")),
            Some(&bookie_app_services::tr!("The transaction is rolled back when the tab closes.")),
        );
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("close", &bookie_app_services::tr!("Roll Back and Close"));
        dialog.set_response_appearance("close", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let tab_view = tab_view.clone();
        dialog.connect_response(None, move |_, response| match response {
            "close" => sender.input(AppMsg::FinishCloseWorkspaceTab(id)),
            _ => tab_view.close_page_finish(&page, false),
        });
        dialog.present(Some(&self.window));
        true
    }
}

#[cfg(test)]
#[path = "workspace_close_tests.rs"]
mod tests;
