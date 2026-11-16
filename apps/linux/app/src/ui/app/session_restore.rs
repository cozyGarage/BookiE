use std::collections::VecDeque;

use bookie_storage::SavedConnection;
use relm4::ComponentSender;
use relm4::adw::prelude::*;
use relm4::prelude::*;
use uuid::Uuid;

use super::{App, AppInit, AppMsg};

pub(super) fn open_extra_window(app: &mut App, sender: ComponentSender<App>) {
    let ctrl = App::builder()
        .launch(AppInit {
            registry: app.registry.clone(),
            persistence: app.persistence.clone(),
            workspace: app.workspace.clone(),
            history: app.history.clone(),
            database: app.database.clone(),
            preferences: app.preferences.clone(),
            mcp_bridge: app.mcp_bridge.clone(),
        })
        .detach();
    if let Some(application) = app.window.application() {
        ctrl.widget().set_application(Some(&application));
    }
    ctrl.widget().present();
    let key = ctrl.widget().as_ptr() as usize;
    let destroy_sender = sender.clone();
    forget_group_on_close(ctrl.widget().upcast_ref(), key, sender);
    ctrl.widget().connect_destroy(move |_| {
        destroy_sender.input(AppMsg::ExtraWindowClosed(key));
    });
    app.extra_windows.push((key, ctrl));
}

pub(super) fn next_restore_target(queue: &mut VecDeque<Uuid>, can_open: impl Fn(Uuid) -> bool) -> Option<Uuid> {
    while let Some(id) = queue.pop_front() {
        if can_open(id) {
            return Some(id);
        }
    }
    None
}

pub(super) fn forget_group_on_close(window: &relm4::gtk::Window, key: usize, sender: ComponentSender<App>) {
    window.connect_close_request(move |_| {
        sender.input(AppMsg::ExtraWindowDismissed(key));
        relm4::gtk::glib::Propagation::Proceed
    });
}

impl App {
    pub(super) fn begin_session_restore(&mut self, connections: &[SavedConnection], sender: ComponentSender<Self>) {
        if !crate::services::window_state::take_session_restore_turn() {
            self.begin_group_restore(sender);
            return;
        }
        let last = self.window_state.borrow().last_connection_id;
        let available: Vec<Uuid> = connections.iter().map(|saved| saved.id).collect();
        let stored = crate::services::open_sessions::stored();
        let plan = bookie_app_services::window_sessions::startup_restore_plan(stored.as_deref(), last, &available);
        let windows_to_open = plan.others.len();
        let window = self.window.as_ptr() as usize;
        crate::services::open_sessions::record_window(window, plan.first.clone());
        if let Some(pending) = crate::services::open_sessions::queue_groups(plan.others) {
            self.save_open_sessions(pending, sender.clone());
        }
        for _ in 0..windows_to_open {
            sender.input(AppMsg::NewWindow);
        }
        self.multi.restore_queue = plan.first.into();
        self.continue_session_restore(sender);
    }

    fn begin_group_restore(&mut self, sender: ComponentSender<Self>) {
        let Some(keys) = crate::services::open_sessions::take_group() else {
            return;
        };
        self.multi.restore_queue = keys.into();
        self.continue_session_restore(sender);
    }

    pub(super) fn continue_session_restore(&mut self, sender: ComponentSender<Self>) {
        let sessions = &self.multi.sessions;
        let saved = &self.saved_connections;
        let target = next_restore_target(&mut self.multi.restore_queue, |id| {
            !sessions.is_working(id) && saved.iter().any(|connection| connection.id == id)
        });
        let Some(saved) = target.and_then(|id| saved.iter().find(|connection| connection.id == id).cloned()) else {
            return;
        };
        self.on_open_saved(saved, sender);
    }

    pub(super) fn stop_session_restore(&mut self) {
        self.multi.restore_queue.clear();
    }

    pub(super) fn persist_open_sessions(&self, sender: ComponentSender<Self>) {
        let window = self.window.as_ptr() as usize;
        let pending = crate::services::open_sessions::record_window(window, self.multi.sessions.restore_keys());
        self.save_open_sessions(pending, sender);
    }

    pub(super) fn forget_open_sessions_window(&self, window: usize, sender: ComponentSender<Self>) {
        let pending = crate::services::open_sessions::record_window(window, Vec::new());
        self.save_open_sessions(pending, sender);
    }

    fn save_open_sessions(&self, pending: crate::services::open_sessions::PendingSave, sender: ComponentSender<Self>) {
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let result = tokio::task::spawn_blocking(move || crate::services::open_sessions::save(&pending))
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|result| result);
                    if let Err(error) = result {
                        tracing::warn!(error = %error, "open sessions write failed");
                    }
                })
                .drop_on_shutdown()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_opens_each_remaining_key_once_in_order() {
        let [first, second] = [Uuid::new_v4(), Uuid::new_v4()];
        let mut queue = VecDeque::from([first, second]);
        assert_eq!(next_restore_target(&mut queue, |_| true), Some(first));
        assert_eq!(next_restore_target(&mut queue, |_| true), Some(second));
        assert_eq!(next_restore_target(&mut queue, |_| true), None);
    }

    #[test]
    fn restore_skips_keys_that_are_already_open_or_no_longer_saved() {
        let [open, deleted, wanted] = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let mut queue = VecDeque::from([open, deleted, wanted]);
        assert_eq!(next_restore_target(&mut queue, |id| id == wanted), Some(wanted));
        assert!(queue.is_empty());
    }
}
