use relm4::ComponentSender;
use relm4::adw::prelude::*;
use uuid::Uuid;

use crate::ui::browse_tab::BrowseTabInput;

use super::App;

fn cancel_browse_save_intents(
    close_after_save: &mut bookie_app_services::workspace::CloseAfterSave,
    close_window_after_save: &std::cell::Cell<bool>,
    tab_id: Uuid,
) {
    close_after_save.cancel(&tab_id);
    close_window_after_save.set(false);
}

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
        let toast = crate::ui::plain_text::toast(&msg);
        toast.set_timeout(4);
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
        self.finish_browse_save_failure(tab_id, save_id, message);
    }

    pub(super) fn finish_browse_save_failure(&mut self, tab_id: Uuid, save_id: Uuid, message: String) {
        self.set_row_op_in_flight(false);
        cancel_browse_save_intents(
            &mut self.close_after_save.borrow_mut(),
            &self.close_window_after_save,
            tab_id,
        );
        self.dispatch_to_tab(tab_id, BrowseTabInput::SaveFailed(save_id, message));
        self.connection_switch_save_failed();
    }
}

#[cfg(test)]
mod tests {
    use super::cancel_browse_save_intents;
    use bookie_app_services::workspace::CloseAfterSave;
    use std::cell::Cell;
    use uuid::Uuid;

    #[test]
    fn failed_browse_save_cancels_tab_and_window_close_intents() {
        let tab_id = Uuid::new_v4();
        let mut close_after_save = CloseAfterSave::default();
        close_after_save.begin(tab_id, 2);
        let close_window_after_save = Cell::new(true);

        cancel_browse_save_intents(&mut close_after_save, &close_window_after_save, tab_id);

        assert_eq!(close_after_save.remaining(&tab_id), None);
        assert!(close_after_save.is_empty());
        assert!(!close_window_after_save.get());
    }
}
