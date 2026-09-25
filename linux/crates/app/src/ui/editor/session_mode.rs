use std::sync::Arc;

use relm4::ComponentSender;
use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use tablepro_core::{Connection, DriverError, OperationControl, QueryResult, Session, Value};
use uuid::Uuid;

use super::{SqlEditor, SqlEditorInput};

pub(crate) type SharedSession = Arc<tokio::sync::Mutex<Option<Box<dyn Session>>>>;

pub struct OpenedSession(pub(crate) SharedSession);

impl std::fmt::Debug for OpenedSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OpenedSession")
    }
}

#[derive(Clone)]
pub(crate) enum StatementTarget {
    Pool(Arc<dyn Connection>),
    Session(SharedSession),
}

impl StatementTarget {
    pub(crate) async fn query(
        &self,
        sql: &str,
        params: &[Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        match self {
            Self::Pool(connection) => connection.query_params_controlled(sql, params, control).await,
            Self::Session(shared) => match shared.lock().await.as_mut() {
                Some(session) => session.query_params_controlled(sql, params, control).await,
                None => Err(DriverError::Unsupported("the session was closed".into())),
            },
        }
    }

    pub(crate) async fn transaction_open(&self) -> Option<bool> {
        match self {
            Self::Pool(_) => None,
            Self::Session(shared) => Some(shared.lock().await.as_ref().is_some_and(|s| s.transaction_open())),
        }
    }
}

pub(crate) struct EditorSession {
    shared: SharedSession,
    connection_id: Uuid,
    transaction_open: bool,
}

pub(crate) fn session_label(transaction_open: bool) -> String {
    if transaction_open {
        crate::tr!("Session · transaction open")
    } else {
        crate::tr!("Session")
    }
}

impl SqlEditor {
    pub(super) fn statement_target(&self, pooled: Arc<dyn Connection>) -> StatementTarget {
        match &self.session {
            Some(session) if Some(session.connection_id) == self.connection_id => {
                StatementTarget::Session(session.shared.clone())
            }
            _ => StatementTarget::Pool(pooled),
        }
    }

    pub(crate) fn session_transaction_open(&self) -> bool {
        self.session.as_ref().is_some_and(|session| session.transaction_open)
    }

    pub(super) fn on_session_toggled(&mut self, enabled: bool, sender: &ComponentSender<Self>) {
        match (enabled, &self.session) {
            (true, None) => self.open_session(sender),
            (false, Some(session)) if session.transaction_open => self.confirm_session_end(sender),
            (false, Some(_)) => self.end_session(false),
            _ => {}
        }
    }

    fn open_session(&mut self, sender: &ComponentSender<Self>) {
        let (Some(connection_id), Some(connection)) = (
            self.connection_id,
            self.connection_id.and_then(|id| self.database.get(id)),
        ) else {
            self.session_button.set_active(false);
            self.status.set_label(&crate::tr!("no active connection"));
            return;
        };
        let sender = sender.clone();
        relm4::spawn(async move {
            let result = connection
                .open_session()
                .await
                .map(|session| OpenedSession(Arc::new(tokio::sync::Mutex::new(Some(session)))))
                .map_err(|error| session_open_message(&error));
            sender.input(SqlEditorInput::SessionOpened { connection_id, result });
        });
    }

    pub(super) fn on_session_opened(&mut self, connection_id: Uuid, result: Result<OpenedSession, String>) {
        match result {
            Ok(OpenedSession(shared))
                if Some(connection_id) == self.connection_id && self.session_button.is_active() =>
            {
                self.session = Some(EditorSession {
                    shared,
                    connection_id,
                    transaction_open: false,
                });
                self.session_button.set_label(&session_label(false));
            }
            Ok(OpenedSession(shared)) => close_detached(shared, false),
            Err(message) => {
                self.session_button.set_active(false);
                self.status.set_label(&message);
            }
        }
    }

    pub(super) fn on_session_state(&mut self, transaction_open: bool) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.transaction_open = transaction_open;
        self.session_button.set_label(&session_label(transaction_open));
        if transaction_open {
            self.session_button.add_css_class("warning");
        } else {
            self.session_button.remove_css_class("warning");
        }
    }

    pub(super) fn end_session(&mut self, commit: bool) {
        self.session_button.set_label(&session_label(false));
        self.session_button.remove_css_class("warning");
        if self.session_button.is_active() {
            self.session_button.set_active(false);
        }
        if let Some(session) = self.session.take() {
            close_detached(session.shared, commit);
        }
    }

    fn confirm_session_end(&self, sender: &ComponentSender<Self>) {
        let dialog = adw::AlertDialog::new(
            Some(&crate::tr!("End the session with an open transaction?")),
            Some(&crate::tr!(
                "Changes made since BEGIN are kept only if you commit them."
            )),
        );
        dialog.add_response("cancel", &crate::tr!("Cancel"));
        dialog.add_response("rollback", &crate::tr!("Roll Back"));
        dialog.add_response("commit", &crate::tr!("Commit"));
        dialog.set_response_appearance("rollback", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let sender = sender.clone();
        let button = self.session_button.clone();
        dialog.connect_response(None, move |_, response| match response {
            "commit" => sender.input(SqlEditorInput::SessionEnd { commit: true }),
            "rollback" => sender.input(SqlEditorInput::SessionEnd { commit: false }),
            _ => button.set_active(true),
        });
        let parent = self.source_view.root().and_downcast::<gtk::Window>();
        dialog.present(parent.as_ref());
    }
}

pub(crate) fn close_detached(shared: SharedSession, commit: bool) {
    relm4::spawn(async move {
        let Some(mut session) = shared.lock().await.take() else {
            return;
        };
        if commit {
            let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
            if let Err(error) = session.query_params_controlled("COMMIT", &[], &control).await {
                tracing::warn!(error = %error, "session commit failed; the transaction is rolled back on close");
            }
        }
        if let Err(error) = session.close().await {
            tracing::warn!(error = %error, "closing a dedicated session failed");
        }
    });
}

fn session_open_message(error: &DriverError) -> String {
    match error {
        DriverError::Unsupported(_) => crate::tr!("This engine does not offer dedicated sessions yet"),
        other => crate::ui::error_text::driver_message(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_button_says_when_a_transaction_is_open() {
        assert_eq!(session_label(false), "Session");
        assert_eq!(session_label(true), "Session · transaction open");
    }

    #[tokio::test]
    async fn a_closed_session_target_refuses_instead_of_falling_back_to_the_pool() {
        let target = StatementTarget::Session(Arc::new(tokio::sync::Mutex::new(None)));
        let control = OperationControl::with_timeout(std::time::Duration::from_secs(1));

        let error = target.query("SELECT 1", &[], &control).await.unwrap_err();

        assert!(matches!(error, DriverError::Unsupported(_)));
        assert_eq!(target.transaction_open().await, Some(false));
    }
}
