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
    Session { id: Uuid, shared: SharedSession },
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
            Self::Session { shared, .. } => match shared.lock().await.as_mut() {
                Some(session) => session.query_params_controlled(sql, params, control).await,
                None => Err(DriverError::Unsupported("the session was closed".into())),
            },
        }
    }

    pub(crate) async fn transaction_state(&self) -> Option<(Uuid, bool)> {
        match self {
            Self::Pool(_) => None,
            Self::Session { id, shared } => {
                Some((*id, shared.lock().await.as_ref().is_some_and(|s| s.transaction_open())))
            }
        }
    }
}

pub(crate) struct EditorSession {
    pub(super) id: Uuid,
    shared: SharedSession,
    connection_id: Uuid,
    transaction_open: bool,
}

pub(super) fn session_callback_matches(callback_id: Uuid, current_id: Option<Uuid>) -> bool {
    current_id == Some(callback_id)
}

fn open_callback_matches(
    callback_id: Uuid,
    pending_id: Option<Uuid>,
    callback_connection_id: Uuid,
    current_connection_id: Option<Uuid>,
) -> bool {
    session_callback_matches(callback_id, pending_id) && Some(callback_connection_id) == current_connection_id
}

fn commit_callback_matches(callback_id: Uuid, current_id: Option<Uuid>, ending_id: Option<Uuid>) -> bool {
    session_callback_matches(callback_id, current_id) && Some(callback_id) == ending_id
}

fn apply_transaction_state(session: &mut EditorSession, callback_id: Uuid, transaction_open: bool) -> bool {
    if !session_callback_matches(callback_id, Some(session.id)) {
        return false;
    }
    session.transaction_open = transaction_open;
    true
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
            Some(session) if Some(session.connection_id) == self.connection_id => StatementTarget::Session {
                id: session.id,
                shared: session.shared.clone(),
            },
            _ => StatementTarget::Pool(pooled),
        }
    }

    pub(crate) fn session_transaction_open(&self) -> bool {
        self.session.as_ref().is_some_and(|session| session.transaction_open)
    }

    pub(super) fn on_session_toggled(&mut self, enabled: bool, sender: &ComponentSender<Self>) {
        if self.session_ending() {
            return;
        }
        match (enabled, &self.session) {
            (true, None) => self.open_session(sender),
            (false, None) => self.opening_session_id = None,
            (false, Some(session)) if session.transaction_open => self.confirm_session_end(sender),
            (false, Some(_)) => self.end_session(),
            _ => {}
        }
    }

    fn open_session(&mut self, sender: &ComponentSender<Self>) {
        let (Some(connection_id), Some(connection)) = (
            self.connection_id,
            self.connection_id.and_then(|id| self.database.get(id)),
        ) else {
            self.opening_session_id = None;
            self.session_button.set_active(false);
            self.status.set_label(&crate::tr!("no active connection"));
            return;
        };
        let session_id = Uuid::new_v4();
        self.opening_session_id = Some(session_id);
        let sender = sender.clone();
        relm4::spawn(async move {
            let result = connection
                .open_session()
                .await
                .map(|session| OpenedSession(Arc::new(tokio::sync::Mutex::new(Some(session)))))
                .map_err(|error| session_open_message(&error));
            sender.input(SqlEditorInput::SessionOpened {
                connection_id,
                session_id,
                result,
            });
        });
    }

    pub(super) fn on_session_opened(
        &mut self,
        connection_id: Uuid,
        session_id: Uuid,
        result: Result<OpenedSession, String>,
    ) {
        let is_current = open_callback_matches(session_id, self.opening_session_id, connection_id, self.connection_id);
        if !is_current {
            if let Ok(OpenedSession(shared)) = result {
                close_detached(shared);
            }
            return;
        }
        self.opening_session_id = None;
        match result {
            Ok(OpenedSession(shared)) if self.session_button.is_active() => {
                self.session = Some(EditorSession {
                    id: session_id,
                    shared,
                    connection_id,
                    transaction_open: false,
                });
                self.session_button.set_label(&session_label(false));
            }
            Ok(OpenedSession(shared)) => close_detached(shared),
            Err(message) => {
                self.session_button.set_active(false);
                self.status.set_label(&message);
            }
        }
    }

    pub(super) fn on_session_state(&mut self, session_id: Uuid, transaction_open: bool) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !apply_transaction_state(session, session_id, transaction_open) {
            return;
        }
        self.session_button.set_label(&session_label(transaction_open));
        if transaction_open {
            self.session_button.add_css_class("warning");
        } else {
            self.session_button.remove_css_class("warning");
        }
    }

    pub(super) fn end_session(&mut self) {
        self.opening_session_id = None;
        self.session_button.set_label(&session_label(false));
        self.session_button.remove_css_class("warning");
        if self.session_button.is_active() {
            self.session_button.set_active(false);
        }
        if let Some(session) = self.session.take() {
            if self.ending_session_id == Some(session.id) {
                self.ending_session_id = None;
            }
            close_detached(session.shared);
        }
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
        self.status.set_label(&crate::tr!("Committing session…"));
        let shared = session.shared.clone();
        let sender = sender.clone();
        relm4::spawn(async move {
            let result = commit_session(&shared)
                .await
                .map_err(|error| crate::ui::error_text::driver_message(&error));
            sender.input(SqlEditorInput::SessionCommitFinished { session_id, result });
        });
    }

    pub(super) fn on_session_commit_finished(&mut self, session_id: Uuid, result: Result<(), String>) {
        if !commit_callback_matches(
            session_id,
            self.session.as_ref().map(|session| session.id),
            self.ending_session_id,
        ) {
            return;
        }
        self.ending_session_id = None;
        self.session_button.set_sensitive(true);
        match result {
            Ok(()) => {
                self.end_session();
                self.status.set_label(&crate::tr!("Session committed"));
            }
            Err(message) => {
                self.session_button.set_active(true);
                self.status.set_label(&message);
            }
        }
    }

    fn confirm_session_end(&self, sender: &ComponentSender<Self>) {
        let Some(session_id) = self.session.as_ref().map(|session| session.id) else {
            return;
        };
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
            "commit" => sender.input(SqlEditorInput::SessionEnd {
                session_id,
                commit: true,
            }),
            "rollback" => sender.input(SqlEditorInput::SessionEnd {
                session_id,
                commit: false,
            }),
            _ => button.set_active(true),
        });
        let parent = self.source_view.root().and_downcast::<gtk::Window>();
        dialog.present(parent.as_ref());
    }
}

async fn commit_session(shared: &SharedSession) -> Result<(), DriverError> {
    let mut guard = shared.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| DriverError::Unsupported("the session was closed".into()))?;
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    session.query_params_controlled("COMMIT", &[], &control).await?;
    Ok(())
}

pub(crate) fn close_detached(shared: SharedSession) {
    relm4::spawn(async move {
        let Some(session) = shared.lock().await.take() else {
            return;
        };
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

    struct FailFirstCommit(bool);

    #[async_trait::async_trait]
    impl Session for FailFirstCommit {
        async fn query_params_controlled(
            &mut self,
            sql: &str,
            _params: &[Value],
            _control: &OperationControl,
        ) -> Result<QueryResult, DriverError> {
            assert_eq!(sql, "COMMIT");
            if self.0 {
                self.0 = false;
                return Err(DriverError::TimedOut);
            }
            Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                truncated: false,
            })
        }

        fn is_usable(&self) -> bool {
            true
        }

        async fn close(self: Box<Self>) -> Result<(), DriverError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_failed_commit_leaves_the_session_available_for_retry() {
        let shared: SharedSession = Arc::new(tokio::sync::Mutex::new(Some(Box::new(FailFirstCommit(true)))));
        assert!(matches!(commit_session(&shared).await, Err(DriverError::TimedOut)));
        assert!(shared.lock().await.is_some());
        commit_session(&shared).await.expect("retry commit");
    }

    #[test]
    fn the_session_button_says_when_a_transaction_is_open() {
        assert_eq!(session_label(false), "Session");
        assert_eq!(session_label(true), "Session · transaction open");
    }

    #[tokio::test]
    async fn a_closed_session_target_refuses_instead_of_falling_back_to_the_pool() {
        let id = Uuid::new_v4();
        let target = StatementTarget::Session {
            id,
            shared: Arc::new(tokio::sync::Mutex::new(None)),
        };
        let control = OperationControl::with_timeout(std::time::Duration::from_secs(1));

        let error = target.query("SELECT 1", &[], &control).await.unwrap_err();

        assert!(matches!(error, DriverError::Unsupported(_)));
        assert_eq!(target.transaction_state().await, Some((id, false)));
    }

    #[test]
    fn callbacks_from_an_older_editor_session_do_not_match_the_current_session() {
        let old = Uuid::new_v4();
        let current = Uuid::new_v4();
        assert!(!session_callback_matches(old, Some(current)));
        assert!(!session_callback_matches(old, None));
        assert!(session_callback_matches(current, Some(current)));
        let connection = Uuid::new_v4();
        assert!(!open_callback_matches(old, Some(current), connection, Some(connection)));
        assert!(!open_callback_matches(current, Some(current), connection, None));
        assert!(open_callback_matches(
            current,
            Some(current),
            connection,
            Some(connection)
        ));
        assert!(!commit_callback_matches(old, Some(current), Some(old)));
        assert!(!commit_callback_matches(current, Some(current), None));
        assert!(commit_callback_matches(current, Some(current), Some(current)));
    }

    #[test]
    fn a_late_transaction_state_does_not_change_a_replacement_session() {
        let old_id = Uuid::new_v4();
        let current_id = Uuid::new_v4();
        let mut session = EditorSession {
            id: current_id,
            shared: Arc::new(tokio::sync::Mutex::new(None)),
            connection_id: Uuid::new_v4(),
            transaction_open: false,
        };

        assert!(!apply_transaction_state(&mut session, old_id, true));
        assert!(!session.transaction_open);
        assert!(apply_transaction_state(&mut session, current_id, true));
        assert!(session.transaction_open);
    }
}
