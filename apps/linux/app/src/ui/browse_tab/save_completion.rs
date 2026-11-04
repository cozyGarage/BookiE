use relm4::ComponentSender;
use relm4::gtk::prelude::WidgetExt;
use uuid::Uuid;

use super::{BrowseTab, BrowseTabOutput};

impl BrowseTab {
    pub(super) fn handle_save_completed(&mut self, save_id: Uuid, sender: ComponentSender<Self>) {
        if self.save_in_flight.get() != Some(save_id) {
            return;
        }
        self.save_in_flight.set(None);
        self.change_trackers.with_tab(self.tab_id, |tracker| tracker.clear());
        self.refresh_pending_bar(0);
        self.save_button.set_sensitive(true);
        self.discard_button.set_sensitive(true);
        self.emit_fetch_page_and_count(&sender);
    }

    pub(super) fn handle_save_failed(&mut self, save_id: Uuid, message: String, sender: ComponentSender<Self>) {
        if self.save_in_flight.get() != Some(save_id) {
            return;
        }
        self.save_in_flight.set(None);
        self.save_button.set_sensitive(true);
        self.discard_button.set_sensitive(true);
        let _ = sender.output(BrowseTabOutput::ShowSelectionAlert {
            title: crate::tr!("Save failed"),
            body: message,
        });
    }
}
