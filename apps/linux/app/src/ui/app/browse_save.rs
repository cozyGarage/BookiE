use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw};
use uuid::Uuid;

use crate::ui::browse_tab::BrowseTabInput;

use super::App;

impl App {
    pub(super) fn on_browse_save_completed(
        &mut self,
        tab_id: Uuid,
        save_id: Uuid,
        warning: Option<String>,
        sender: ComponentSender<Self>,
    ) {
        if !self.take_active_browse_save(tab_id, save_id) {
            return;
        }
        self.set_row_op_in_flight(false);
        let msg = warning.unwrap_or_else(|| crate::tr!("Saved"));
        let toast = adw::Toast::builder().title(msg).timeout(4).build();
        self.toast_overlay.add_toast(toast);
        self.dispatch_to_tab(tab_id, BrowseTabInput::SaveCompleted(save_id));
        let drained = self.close_after_save.borrow_mut().complete(&tab_id);
        if drained {
            self.request_workspace_tab_close(tab_id);
        }
        if self.close_window_after_save.get() && self.close_after_save.borrow().is_empty() {
            self.close_window_after_save.set(false);
            self.window.close();
        }
        self.connection_switch_save_succeeded(tab_id, sender);
    }

    pub(super) fn on_browse_save_failed(&mut self, tab_id: Uuid, save_id: Uuid, message: String) {
        if !self.take_active_browse_save(tab_id, save_id) {
            return;
        }
        self.set_row_op_in_flight(false);
        self.close_after_save.borrow_mut().cancel(&tab_id);
        self.close_window_after_save.set(false);
        self.dispatch_to_tab(tab_id, BrowseTabInput::SaveFailed(save_id, message));
        self.connection_switch_save_failed();
    }
}
