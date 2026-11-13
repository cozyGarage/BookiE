use relm4::{ComponentSender, gtk::prelude::*};

use super::{App, SessionTeardownAction, types::ConnectionTransition};

pub(super) fn flush_window_edits(app: &mut App) {
    app.grid_close.window_ready.set(true);
    app.window.close();
}

pub(super) fn persist_window_state_for_close(
    app: &mut App,
    width: i32,
    height: i32,
    maximized: bool,
    sender: ComponentSender<App>,
) {
    let mut state = app.window_state.borrow_mut();
    state.width = width;
    state.height = height;
    state.maximized = maximized;
    drop(state);
    app.persist_window_state(sender, true);
}

pub(super) fn window_state_persisted(result: Result<(), String>) {
    if let Err(error) = result {
        tracing::warn!(error = %error, "window state write failed");
    }
}

pub(super) fn window_state_persisted_for_close(app: &mut App, result: Result<(), String>) {
    app.window_close_state_pending.set(false);
    if let Err(error) = result {
        tracing::warn!(error = %error, "window state write failed before close");
    }
    app.window_close_state_saved.set(true);
    app.window.close();
}

pub(super) fn cancel_session_teardown(app: &mut App, action: SessionTeardownAction) {
    match action {
        SessionTeardownAction::WindowClose => app.session_teardown_pending.set(false),
        SessionTeardownAction::ConnectionSwitch => {
            app.switch.transition = ConnectionTransition::Idle;
            app.switch.prepared = None;
            app.switch.saves_pending.clear();
            app.switch.cancel_audit_was_disabled = None;
        }
        SessionTeardownAction::CloseTab(_) | SessionTeardownAction::Disconnect => {}
    }
}
