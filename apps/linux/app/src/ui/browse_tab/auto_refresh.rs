use relm4::ComponentSender;
use relm4::gtk::glib;
use relm4::gtk::prelude::*;

use super::{BrowseTab, BrowseTabInput};

const AUTO_REFRESH_SECS: u32 = 15;

pub(super) fn allowed(active: bool, dirty: bool, saving: bool, busy: bool, approval: bool, in_flight: bool) -> bool {
    active && !dirty && !saving && !busy && !approval && !in_flight
}

impl BrowseTab {
    fn auto_refresh_eligible(&self) -> bool {
        let active = self
            .connection_id
            .is_some_and(|connection| self.database.is_active(connection));
        let dirty = self
            .change_trackers
            .with_tab_ref(self.tab_id, |tracker| tracker.has_pending())
            .unwrap_or(true);
        allowed(
            active,
            dirty,
            self.save_in_flight(),
            bookie_app_services::busy_jobs::global().active() > 0,
            crate::services::gtk_approval::has_pending(),
            self.auto_refresh_in_flight.get(),
        )
    }

    pub(super) fn refresh_auto_refresh_availability(&self) {
        let available = self
            .connection_id
            .is_some_and(|connection| self.database.is_active(connection));
        self.auto_refresh_button.set_sensitive(available);
    }

    pub(super) fn set_auto_refresh(&mut self, enabled: bool, sender: ComponentSender<Self>) {
        if !enabled {
            if self.auto_refresh_source.is_some() {
                self.stop_auto_refresh();
            }
            return;
        }
        if self.auto_refresh_source.is_some() {
            return;
        }
        let Some(connection) = self
            .connection_id
            .filter(|connection| self.database.is_active(*connection))
        else {
            self.auto_refresh_button.set_active(false);
            self.auto_refresh_button.set_sensitive(false);
            return;
        };
        let database = self.database.clone();
        self.auto_refresh_source = Some(glib::timeout_add_seconds_local(AUTO_REFRESH_SECS, move || {
            if !database.is_active(connection) {
                sender.input(BrowseTabInput::AutoRefreshDisconnected);
                return glib::ControlFlow::Break;
            }
            sender.input(BrowseTabInput::AutoRefreshTick);
            glib::ControlFlow::Continue
        }));
    }

    pub(super) fn auto_refresh_tick(&mut self, sender: &ComponentSender<Self>) {
        if self.auto_refresh_source.is_none() || !self.auto_refresh_eligible() {
            return;
        }
        self.auto_refresh_in_flight.set(true);
        let (queue, revision) = self.edit_flush_snapshot();
        let (reply, result) = tokio::sync::oneshot::channel();
        if queue
            .send(crate::ui::grid::GridMsg::EditsFlushed { revision, reply })
            .is_err()
        {
            self.auto_refresh_in_flight.set(false);
            return;
        }
        let sender = sender.clone();
        relm4::spawn_local(async move {
            let flushed = result.await.is_ok_and(|result| result.is_ok());
            sender.input(BrowseTabInput::AutoRefreshFlushFinished(flushed));
        });
    }

    pub(super) fn auto_refresh_after_flush(&mut self, flushed: bool, sender: &ComponentSender<Self>) {
        if !self.auto_refresh_in_flight.replace(false) || !flushed || self.auto_refresh_source.is_none() {
            return;
        }
        if !self.auto_refresh_eligible() {
            return;
        }
        self.auto_refresh_in_flight.set(true);
        self.capture_focus_for_restore();
        self.auto_refresh_status_label
            .set_text(&bookie_app_services::tr!("Refreshing…"));
        self.emit_auto_fetch_page_and_count(sender);
    }

    pub(super) fn stop_auto_refresh_for_disconnect(&mut self) {
        self.auto_refresh_source.take();
        self.auto_refresh_button.set_active(false);
        self.auto_refresh_button.set_sensitive(false);
        self.auto_refresh_in_flight.set(false);
        self.auto_refresh_status_label.set_text("");
    }

    pub(super) fn stop_auto_refresh_after_failure(&mut self) {
        self.stop_auto_refresh();
        self.auto_refresh_button.set_active(false);
        self.auto_refresh_in_flight.set(false);
        self.refresh_auto_refresh_availability();
    }

    pub(super) fn stop_auto_refresh(&mut self) {
        self.auto_refresh_in_flight.set(false);
        if let Some(source) = self.auto_refresh_source.take() {
            source.remove();
        }
        self.auto_refresh_status_label.set_text("");
    }

    pub(super) fn shutdown_auto_refresh(&mut self) {
        self.auto_refresh_in_flight.set(false);
        if let Some(source) = self.auto_refresh_source.take() {
            source.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn refresh_skips_disconnected_dirty_saving_busy_approval_and_active_request_states() {
        assert!(allowed(true, false, false, false, false, false));
        assert!(!allowed(false, false, false, false, false, false));
        assert!(!allowed(true, true, false, false, false, false));
        assert!(!allowed(true, false, true, false, false, false));
        assert!(!allowed(true, false, false, true, false, false));
        assert!(!allowed(true, false, false, false, true, false));
        assert!(!allowed(true, false, false, false, false, true));
    }
}
