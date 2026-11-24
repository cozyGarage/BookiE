use std::sync::Arc;

use bookie_core::{Connection, DriverError, OperationControl, Session, SessionCommitOutcome, TableInfo};
use relm4::ComponentSender;
use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use uuid::Uuid;

use crate::services::database_service::ConnectionIdentity;
pub(crate) use bookie_app_services::interactive_session::*;
use bookie_policy::{Audited, PolicyGuard, Principal};

use super::{SqlEditor, SqlEditorInput};

mod toolbar;

fn session_end_dialog(button: &gtk::ToggleButton, on_end: impl Fn(bool) + 'static) -> adw::AlertDialog {
    let dialog = adw::AlertDialog::new(
        Some(&bookie_app_services::tr!("End the session with an open transaction?")),
        Some(&bookie_app_services::tr!(
            "Changes made since BEGIN are kept only if you commit them."
        )),
    );
    dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
    dialog.add_response("rollback", &bookie_app_services::tr!("Roll Back"));
    dialog.add_response("commit", &bookie_app_services::tr!("Commit"));
    dialog.set_response_appearance("rollback", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let button = button.clone();
    dialog.connect_response(None, move |_, response| match response {
        "commit" => on_end(true),
        "rollback" => on_end(false),
        _ => button.set_active(true),
    });
    dialog
}

impl SqlEditor {
    pub(super) fn on_session_message(&mut self, msg: SqlEditorInput, sender: &ComponentSender<Self>) {
        match msg {
            SqlEditorInput::SessionToggled(enabled) => self.on_session_toggle(enabled, sender),
            SqlEditorInput::SessionOpened {
                connection_id,
                connection_identity,
                session_id,
                result,
            } => self.on_session_opened(connection_id, connection_identity, session_id, result, sender),
            SqlEditorInput::ConnectionIdentityChanged => self.on_connection_identity_notification(sender),
            SqlEditorInput::SessionState {
                session_id,
                transaction_open,
                usable,
            } => self.on_session_state(session_id, transaction_open, usable, sender),
            SqlEditorInput::SessionEnd { session_id, commit } if commit => self.commit_and_end(session_id, sender),
            SqlEditorInput::SessionEnd { session_id, .. } => {
                if session_callback_matches(session_id, self.session.as_ref().map(|session| session.id)) {
                    self.rollback_and_end(session_id, sender);
                }
            }
            SqlEditorInput::SessionCommitFinished { session_id, result } => {
                self.on_session_commit_finished(session_id, result, sender);
            }
            SqlEditorInput::SessionRollbackFinished { session_id, result } => {
                self.on_session_rollback_finished(session_id, result);
            }
            SqlEditorInput::TemporaryTablesLoaded { session_id, result } => {
                self.on_temporary_tables_loaded(session_id, result, sender);
            }
            SqlEditorInput::PrepareForTeardown(reply) => self.prepare_for_teardown(reply, sender),
            SqlEditorInput::SessionTeardownFinished(result) => self.finish_session_teardown(result),
            _ => {}
        }
    }

    pub(super) fn statement_target(
        &self,
        pooled: Arc<dyn Connection>,
        guard: Option<Arc<PolicyGuard>>,
    ) -> StatementTarget {
        match &self.session {
            Some(session)
                if Some(session.connection_id) == self.connection_id
                    && session.connection_identity.as_ref().is_some_and(|identity| {
                        self.database.identity(session.connection_id).as_ref() == Some(identity)
                    }) =>
            {
                StatementTarget::Session {
                    id: session.id,
                    shared: session.shared.clone(),
                }
            }
            Some(session) if Some(session.connection_id) == self.connection_id => {
                StatementTarget::Retired { id: session.id }
            }
            _ if retired_connection_matches(self.retired_session_connection_id, self.connection_id) => {
                StatementTarget::Retired {
                    id: self
                        .retired_session_connection_id
                        .map_or(Uuid::nil(), |(_, session)| session),
                }
            }
            _ => guard
                .map(StatementTarget::GuardedPool)
                .unwrap_or(StatementTarget::Pool(pooled)),
        }
    }

    pub(crate) fn session_transaction_open(&self) -> bool {
        self.session.as_ref().is_some_and(|session| session.transaction_open)
    }

    pub(super) fn on_session_toggled(&mut self, enabled: bool, sender: &ComponentSender<Self>) {
        if self.session_teardown_active {
            return;
        }
        if self.session_ending() {
            return;
        }
        match (enabled, &self.session) {
            (true, None) => self.open_session(sender),
            (false, None) => self.opening_session_id = None,
            (false, Some(session)) if session.transaction_open => self.confirm_session_end(sender),
            (false, Some(_)) => self.end_session(sender),
            _ => {}
        }
    }

    fn open_session(&mut self, sender: &ComponentSender<Self>) {
        let (Some(connection_id), Some((guard, connection_identity))) = (
            self.connection_id,
            self.connection_id
                .and_then(|id| self.database.guard_with_identity(id, Principal::human_gui())),
        ) else {
            self.opening_session_id = None;
            self.session_button.set_active(false);
            self.status.set_label(&bookie_app_services::tr!("no active connection"));
            return;
        };
        let session_id = Uuid::new_v4();
        self.opening_session_id = Some(session_id);
        self.session_open_in_flight = true;
        self.refresh_run_button();
        let sender = sender.clone();
        relm4::spawn(async move {
            let result = guard
                .open_session_with_audit()
                .await
                .map(|session| OpenedSession(Arc::new(tokio::sync::Mutex::new(Some(SessionHandle::Audited(session))))))
                .map_err(|error| session_open_message(&error));
            sender.input(SqlEditorInput::SessionOpened {
                connection_id,
                connection_identity,
                session_id,
                result,
            });
        });
    }

    pub(super) fn on_session_opened(
        &mut self,
        connection_id: Uuid,
        connection_identity: ConnectionIdentity,
        session_id: Uuid,
        result: Result<OpenedSession, String>,
        sender: &ComponentSender<Self>,
    ) {
        self.session_open_in_flight = false;
        let is_current = open_callback_matches(session_id, self.opening_session_id, connection_id, self.connection_id)
            && self.database.identity(connection_id).as_ref() == Some(&connection_identity);
        if !is_current {
            if let Ok(OpenedSession(shared)) = result {
                self.close_session_for_teardown(shared, sender);
            } else {
                self.try_start_session_teardown(sender);
            }
            self.refresh_run_button();
            return;
        }
        self.opening_session_id = None;
        match result {
            Ok(OpenedSession(shared)) if self.session_button.is_active() => {
                self.clear_catalog_changes();
                self.retired_session_connection_id = None;
                self.session = Some(EditorSession {
                    id: session_id,
                    shared,
                    connection_id,
                    connection_identity: Some(connection_identity),
                    transaction_open: false,
                });
                self.transaction_clock.reset(&self.session_button);
            }
            Ok(OpenedSession(shared)) => self.close_session_for_teardown(shared, sender),
            Err(message) => {
                self.session_button.set_active(false);
                self.status.set_label(&message);
            }
        }
        self.refresh_run_button();
        self.try_start_session_teardown(sender);
        self.schema_index.borrow_mut().invalidate_columns();
        (self.completion_refresh)();
    }

    pub(super) fn prepare_for_teardown(
        &mut self,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
        sender: &ComponentSender<Self>,
    ) {
        self.session_teardown_waiters.push(reply);
        if !self.session_teardown_active {
            self.session_teardown_active = true;
            self.opening_session_id = None;
            self.session_button.set_sensitive(false);
            self.run_button.set_sensitive(false);
            self.status
                .set_label(&bookie_app_services::tr!("Rolling back and closing session…"));
            if let Some(token) = self.cancel_token.take() {
                token.cancel();
            }
        }
        self.try_start_session_teardown(sender);
    }

    pub(super) fn try_start_session_teardown(&mut self, sender: &ComponentSender<Self>) {
        if !can_start_session_close(
            self.session_teardown_active,
            self.session_teardown_running,
            self.session_open_in_flight,
            !self.run_generation.active.is_empty(),
        ) {
            return;
        }
        self.clear_catalog_changes();
        if let Some(session) = self.session.take() {
            self.close_session_for_teardown(session.shared, sender);
        } else {
            self.finish_session_teardown(Ok(()));
        }
    }

    fn close_session_for_teardown(&mut self, shared: SharedSession, sender: &ComponentSender<Self>) {
        self.session_teardown_active = true;
        self.session_teardown_running = true;
        self.session_button.set_sensitive(false);
        self.run_button.set_sensitive(false);
        self.status.set_label(&bookie_app_services::tr!("Closing session…"));
        self.transaction_clock.reset(&self.session_button);
        self.session_button.remove_css_class("warning");
        if self.session_button.is_active() {
            self.session_button.set_active(false);
        }
        let sender = sender.clone();
        relm4::spawn(async move {
            let result = close_for_teardown(shared)
                .await
                .map_err(|error| crate::services::error_text::driver_message(&error));
            sender.input(super::SqlEditorInput::SessionTeardownFinished(result));
        });
    }

    pub(super) fn finish_session_teardown(&mut self, result: Result<(), String>) {
        self.session_teardown_active = false;
        self.session_teardown_running = false;
        self.session_button.set_sensitive(true);
        self.refresh_run_button();
        match &result {
            Err(message) => self.status.set_label(message),
            Ok(()) => {
                let status = self.status.label();
                if status == bookie_app_services::tr!("Closing session…")
                    || status == bookie_app_services::tr!("Rolling back and closing session…")
                {
                    self.status.set_label("");
                }
            }
        }
        for reply in self.session_teardown_waiters.drain(..) {
            let _ = reply.send(result.clone());
        }
    }

    pub(super) fn on_connection_identity_changed(&mut self, sender: &ComponentSender<Self>) {
        let Some((session_id, connection_id, connection_identity)) = self
            .session
            .as_ref()
            .map(|session| (session.id, session.connection_id, session.connection_identity.clone()))
        else {
            return;
        };
        if connection_identity
            .as_ref()
            .is_some_and(|identity| self.database.identity(connection_id).as_ref() == Some(identity))
        {
            return;
        }
        self.on_session_state(session_id, false, false, sender);
    }

    pub(super) fn on_session_state(
        &mut self,
        session_id: Uuid,
        transaction_open: bool,
        usable: bool,
        sender: &ComponentSender<Self>,
    ) {
        if self.ending_session_id == Some(session_id) {
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !apply_transaction_state(session, session_id, transaction_open) {
            return;
        }
        if !usable {
            let Some(session) = self.session.take() else {
                return;
            };
            let transaction_was_open = self.transaction_clock.is_open();
            self.retired_session_connection_id = Some((session.connection_id, session.id));
            self.clear_catalog_changes();
            self.close_session_for_teardown(session.shared, sender);
            self.status
                .set_label(&super::session_age::retired_session_message(transaction_was_open));
            return;
        }
        self.transaction_clock
            .set_open(transaction_open, std::time::Instant::now());
        self.transaction_clock.show(&self.session_button);
        if transaction_open {
            self.session_button.add_css_class("warning");
        } else {
            self.session_button.remove_css_class("warning");
        }
    }

    pub(super) fn end_session(&mut self, sender: &ComponentSender<Self>) {
        if self.session.is_some() {
            let _ = sender.output(super::SqlEditorOutput::TemporaryTablesChanged(Ok(Vec::new())));
        }
        self.clear_catalog_changes();
        self.opening_session_id = None;
        self.transaction_clock.reset(&self.session_button);
        self.session_button.remove_css_class("warning");
        if let Some(session) = self.session.take() {
            if self.ending_session_id == Some(session.id) {
                self.ending_session_id = None;
            }
            self.close_session_for_teardown(session.shared, sender);
        } else if self.session_button.is_active() {
            self.session_button.set_active(false);
        }
        self.schema_index.borrow_mut().invalidate_columns();
        (self.completion_refresh)();
    }

    pub(super) fn rollback_and_end(&mut self, session_id: Uuid, sender: &ComponentSender<Self>) {
        let Some(session) = self
            .session
            .as_ref()
            .filter(|session| session_callback_matches(session_id, Some(session.id)))
        else {
            return;
        };
        if self.ending_session_id == Some(session_id) {
            return;
        }
        self.ending_session_id = Some(session_id);
        self.session_teardown_active = true;
        self.session_teardown_running = true;
        self.session_button.set_sensitive(false);
        self.run_button.set_sensitive(false);
        self.status
            .set_label(&bookie_app_services::tr!("Rolling back and closing session…"));
        let shared = session.shared.clone();
        let sender = sender.clone();
        relm4::spawn(async move {
            let result = rollback_and_close(&shared)
                .await
                .map_err(|error| crate::services::error_text::driver_message(&error));
            sender.input(super::SqlEditorInput::SessionRollbackFinished { session_id, result });
        });
    }

    pub(super) fn on_session_rollback_finished(&mut self, session_id: Uuid, result: Result<(), String>) {
        if !commit_callback_matches(
            session_id,
            self.session.as_ref().map(|session| session.id),
            self.ending_session_id,
        ) {
            return;
        }
        self.clear_catalog_changes();
        if let Some(session) = self.session.take()
            && result.is_err()
        {
            self.retired_session_connection_id = Some((session.connection_id, session.id));
        }
        self.ending_session_id = None;
        self.transaction_clock.reset(&self.session_button);
        self.session_button.remove_css_class("warning");
        if self.session_button.is_active() {
            self.session_button.set_active(false);
        }
        if result.is_ok() {
            self.clear_run_marks();
            self.status.set_label(&bookie_app_services::tr!("Session rolled back"));
        }
        self.finish_session_teardown(result);
    }

    pub(super) fn session_ending(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| self.ending_session_id == Some(session.id))
    }

    pub(super) fn commit_and_end(&mut self, session_id: Uuid, sender: &ComponentSender<Self>) {
        let Some(session) = self
            .session
            .as_ref()
            .filter(|session| session_callback_matches(session_id, Some(session.id)))
        else {
            return;
        };
        if self.ending_session_id == Some(session_id) {
            return;
        }
        self.ending_session_id = Some(session_id);
        self.session_button.set_sensitive(false);
        self.status.set_label(&bookie_app_services::tr!("Committing session…"));
        let shared = session.shared.clone();
        let sender = sender.clone();
        relm4::spawn(async move {
            let result = commit_session(&shared)
                .await
                .map_err(|error| crate::services::error_text::driver_message(&error));
            sender.input(SqlEditorInput::SessionCommitFinished { session_id, result });
        });
    }

    pub(super) fn on_session_commit_finished(
        &mut self,
        session_id: Uuid,
        result: Result<SessionCommitOutcome, String>,
        sender: &ComponentSender<Self>,
    ) {
        if !commit_callback_matches(
            session_id,
            self.session.as_ref().map(|session| session.id),
            self.ending_session_id,
        ) {
            return;
        }
        if self.catalog_changes.transaction_finished(&result) {
            self.publish_catalog_change(sender);
        }
        self.ending_session_id = None;
        self.session_button.set_sensitive(true);
        match result {
            Ok(outcome) => {
                self.end_session(sender);
                let message = match outcome {
                    SessionCommitOutcome::Committed => bookie_app_services::tr!("Session committed"),
                    SessionCommitOutcome::RolledBack => {
                        self.clear_run_marks();
                        bookie_app_services::tr!("Session rolled back")
                    }
                };
                self.status.set_label(&message);
            }
            Err(message) => {
                self.session_button.set_active(true);
                self.status.set_label(&message);
            }
        }
    }

    fn clear_catalog_changes(&mut self) {
        self.catalog_changes = Default::default();
        self.catalog_origin = None;
    }

    fn publish_catalog_change(&self, sender: &ComponentSender<Self>) {
        if let Some(origin) = self.catalog_origin.as_ref()
            && origin.owns_window(self.connection_id, &self.database)
        {
            let _ = sender.output(super::SqlEditorOutput::CatalogChanged(origin.clone()));
        }
        self.refresh_temporary_tables(sender);
    }

    pub(super) fn refresh_temporary_tables(&self, sender: &ComponentSender<Self>) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let session_id = session.id;
        let shared = session.shared.clone();
        let sender = sender.clone();
        relm4::spawn(async move {
            let control = OperationControl::with_timeout(std::time::Duration::from_secs(10));
            let result = {
                let mut session = shared.lock().await;
                match session.as_mut() {
                    Some(session) => session
                        .list_temporary_tables_controlled(&control)
                        .await
                        .map_err(|error| crate::services::error_text::driver_message(&error)),
                    None => Ok(Vec::new()),
                }
            };
            sender.input(super::SqlEditorInput::TemporaryTablesLoaded { session_id, result });
        });
    }

    pub(super) fn on_temporary_tables_loaded(
        &self,
        session_id: Uuid,
        result: Result<Vec<TableInfo>, String>,
        sender: &ComponentSender<Self>,
    ) {
        if !session_callback_matches(session_id, self.session.as_ref().map(|session| session.id)) {
            return;
        }
        match result {
            Ok(tables) => {
                let _ = sender.output(super::SqlEditorOutput::TemporaryTablesChanged(Ok(tables)));
            }
            Err(body) => {
                let _ = sender.output(super::SqlEditorOutput::TemporaryTablesChanged(Err(body.clone())));
                let notice =
                    bookie_app_services::tr!("The last successfully loaded list remains visible and may be outdated.");
                let _ = sender.output(super::SqlEditorOutput::ShowAlert {
                    title: bookie_app_services::tr!("Temporary table refresh failed").to_string(),
                    body: format!("{notice}\n\n{body}"),
                });
            }
        }
    }

    fn confirm_session_end(&self, sender: &ComponentSender<Self>) {
        let Some(session_id) = self.session.as_ref().map(|session| session.id) else {
            return;
        };
        self.session_button.set_active(true);
        let sender = sender.clone();
        let dialog = session_end_dialog(&self.session_button, move |commit| {
            sender.input(SqlEditorInput::SessionEnd { session_id, commit })
        });
        let parent = self.source_view.root().and_downcast::<gtk::Window>();
        dialog.present(parent.as_ref());
    }
}

pub(crate) fn close_detached(shared: SharedSession) {
    relm4::spawn(close_detached_session(shared));
}

fn session_open_message(error: &DriverError) -> String {
    match error {
        DriverError::Unsupported(_) => bookie_app_services::tr!("This engine does not offer dedicated sessions yet"),
        other => crate::services::error_text::driver_message(other),
    }
}

#[cfg(test)]
#[path = "session_mode/tests.rs"]
mod tests;
