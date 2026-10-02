use std::sync::Arc;

use relm4::ComponentSender;
use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use tablepro_core::{Connection, DriverError, OperationControl, QueryResult, Session, Value};
use uuid::Uuid;

use super::{SqlEditor, SqlEditorInput};

pub(crate) type SharedSession = Arc<tokio::sync::Mutex<Option<Box<dyn Session>>>>;

pub struct OpenedSession(pub(crate) SharedSession);

pub(crate) async fn close_for_teardown(shared: SharedSession) -> Result<(), DriverError> {
    let session = shared.lock().await.take();
    match session {
        Some(session) => session.close().await,
        None => Ok(()),
    }
}

impl std::fmt::Debug for OpenedSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OpenedSession")
    }
}

#[derive(Clone)]
pub(crate) enum StatementTarget {
    Pool(Arc<dyn Connection>),
    Session { id: Uuid, shared: SharedSession },
    Retired { id: Uuid },
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
                Some(session) if session.is_usable() => session.query_params_controlled(sql, params, control).await,
                Some(_) => Err(retired_session_error()),
                None => Err(DriverError::Unsupported("the session was closed".into())),
            },
            Self::Retired { .. } => Err(retired_session_error()),
        }
    }

    pub(crate) async fn session_state(&self) -> Option<(Uuid, bool, bool)> {
        match self {
            Self::Pool(_) => None,
            Self::Retired { id } => Some((*id, false, false)),
            Self::Session { id, shared } => {
                let session = shared.lock().await;
                Some(match session.as_ref() {
                    Some(session) => (*id, session.transaction_open(), session.is_usable()),
                    None => (*id, false, false),
                })
            }
        }
    }
}

fn retired_session_error() -> DriverError {
    DriverError::Unsupported("the editor session was retired; reopen Session before running more SQL".into())
}

fn retired_connection_matches(retired: Option<(Uuid, Uuid)>, active: Option<Uuid>) -> bool {
    active.is_some_and(|active| retired.is_some_and(|(connection, _)| connection == active))
}

pub(crate) struct EditorSession {
    pub(super) id: Uuid,
    pub(super) shared: SharedSession,
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

fn can_start_session_close(
    teardown_active: bool,
    close_running: bool,
    open_running: bool,
    query_running: bool,
) -> bool {
    teardown_active && !close_running && !open_running && !query_running
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
            _ if retired_connection_matches(self.retired_session_connection_id, self.connection_id) => {
                StatementTarget::Retired {
                    id: self
                        .retired_session_connection_id
                        .map_or(Uuid::nil(), |(_, session)| session),
                }
            }
            _ => StatementTarget::Pool(pooled),
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
        self.session_open_in_flight = true;
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
        sender: &ComponentSender<Self>,
    ) {
        self.session_open_in_flight = false;
        let is_current = open_callback_matches(session_id, self.opening_session_id, connection_id, self.connection_id);
        if !is_current {
            if let Ok(OpenedSession(shared)) = result {
                self.close_session_for_teardown(shared, sender);
            } else {
                self.try_start_session_teardown(sender);
            }
            return;
        }
        self.opening_session_id = None;
        match result {
            Ok(OpenedSession(shared)) if self.session_button.is_active() => {
                self.retired_session_connection_id = None;
                self.session = Some(EditorSession {
                    id: session_id,
                    shared,
                    connection_id,
                    transaction_open: false,
                });
                self.session_button.set_label(&session_label(false));
            }
            Ok(OpenedSession(shared)) => self.close_session_for_teardown(shared, sender),
            Err(message) => {
                self.session_button.set_active(false);
                self.status.set_label(&message);
            }
        }
        self.try_start_session_teardown(sender);
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
            self.status.set_label(&crate::tr!("Rolling back and closing session…"));
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
        self.status.set_label(&crate::tr!("Closing session…"));
        self.session_button.set_label(&session_label(false));
        self.session_button.remove_css_class("warning");
        if self.session_button.is_active() {
            self.session_button.set_active(false);
        }
        let sender = sender.clone();
        relm4::spawn(async move {
            let result = close_for_teardown(shared)
                .await
                .map_err(|error| crate::ui::error_text::driver_message(&error));
            sender.input(super::SqlEditorInput::SessionTeardownFinished(result));
        });
    }

    pub(super) fn finish_session_teardown(&mut self, result: Result<(), String>) {
        self.session_teardown_active = false;
        self.session_teardown_running = false;
        self.session_button.set_sensitive(true);
        self.run_button.set_sensitive(self.run_generation.active.is_empty());
        if let Err(message) = &result {
            self.status.set_label(message);
        }
        for reply in self.session_teardown_waiters.drain(..) {
            let _ = reply.send(result.clone());
        }
    }

    pub(super) fn on_session_state(
        &mut self,
        session_id: Uuid,
        transaction_open: bool,
        usable: bool,
        sender: &ComponentSender<Self>,
    ) {
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
            self.retired_session_connection_id = Some((session.connection_id, session.id));
            self.close_session_for_teardown(session.shared, sender);
            self.status.set_label(&crate::tr!(
                "This editor session was retired after a connection failure. Reopen Session before running more SQL."
            ));
            return;
        }
        self.session_button.set_label(&session_label(transaction_open));
        if transaction_open {
            self.session_button.add_css_class("warning");
        } else {
            self.session_button.remove_css_class("warning");
        }
    }

    pub(super) fn end_session(&mut self, sender: &ComponentSender<Self>) {
        self.opening_session_id = None;
        self.session_button.set_label(&session_label(false));
        self.session_button.remove_css_class("warning");
        if let Some(session) = self.session.take() {
            if self.ending_session_id == Some(session.id) {
                self.ending_session_id = None;
            }
            self.close_session_for_teardown(session.shared, sender);
        } else if self.session_button.is_active() {
            self.session_button.set_active(false);
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

    pub(super) fn on_session_commit_finished(
        &mut self,
        session_id: Uuid,
        result: Result<(), String>,
        sender: &ComponentSender<Self>,
    ) {
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
                self.end_session(sender);
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[test]
    fn a_running_query_or_open_session_delays_teardown() {
        assert!(!can_start_session_close(true, false, false, true));
        assert!(!can_start_session_close(true, false, true, false));
        assert!(!can_start_session_close(true, true, false, false));
        assert!(can_start_session_close(true, false, false, false));
    }

    struct FailFirstCommit(bool);

    struct DelayedClose {
        started: tokio::sync::oneshot::Sender<()>,
        release: tokio::sync::oneshot::Receiver<()>,
    }

    struct CountedClose {
        calls: Arc<AtomicUsize>,
        result: Result<(), DriverError>,
    }

    struct HealthSession {
        usable: Arc<AtomicBool>,
        queries: Arc<AtomicUsize>,
        fail_query: bool,
    }

    #[async_trait::async_trait]
    impl Session for DelayedClose {
        async fn query_params_controlled(
            &mut self,
            _: &str,
            _: &[Value],
            _: &OperationControl,
        ) -> Result<QueryResult, DriverError> {
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
            let _ = self.started.send(());
            let _ = self.release.await;
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl Session for CountedClose {
        async fn query_params_controlled(
            &mut self,
            _: &str,
            _: &[Value],
            _: &OperationControl,
        ) -> Result<QueryResult, DriverError> {
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
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result
        }
    }

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

    #[async_trait::async_trait]
    impl Session for HealthSession {
        async fn query_params_controlled(
            &mut self,
            _: &str,
            _: &[Value],
            _: &OperationControl,
        ) -> Result<QueryResult, DriverError> {
            self.queries.fetch_add(1, Ordering::SeqCst);
            if self.fail_query {
                Err(DriverError::Unsupported("ordinary statement error".into()))
            } else {
                Ok(QueryResult {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    truncated: false,
                })
            }
        }

        fn is_usable(&self) -> bool {
            self.usable.load(Ordering::SeqCst)
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

    #[tokio::test]
    async fn teardown_waits_for_delayed_session_close_before_completing() {
        let (started, did_start) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let shared: SharedSession = Arc::new(tokio::sync::Mutex::new(Some(Box::new(DelayedClose {
            started,
            release: wait,
        }))));
        let closing = tokio::spawn(close_for_teardown(shared.clone()));

        did_start.await.expect("close started");
        assert!(shared.lock().await.is_none());
        assert!(!closing.is_finished(), "teardown must wait for close to return");
        release.send(()).expect("release close");
        closing.await.expect("close task").expect("close succeeds");
    }

    #[tokio::test]
    async fn teardown_propagates_close_errors_and_takes_the_session_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let shared: SharedSession = Arc::new(tokio::sync::Mutex::new(Some(Box::new(CountedClose {
            calls: calls.clone(),
            result: Err(DriverError::TimedOut),
        }))));

        assert!(matches!(
            close_for_teardown(shared.clone()).await,
            Err(DriverError::TimedOut)
        ));
        close_for_teardown(shared)
            .await
            .expect("repeated cleanup is idempotent");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
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
        assert_eq!(target.session_state().await, Some((id, false, false)));
    }

    #[tokio::test]
    async fn a_retired_session_reports_its_identity_and_never_dispatches_again() {
        let id = Uuid::new_v4();
        let queries = Arc::new(AtomicUsize::new(0));
        let target = StatementTarget::Session {
            id,
            shared: Arc::new(tokio::sync::Mutex::new(Some(Box::new(HealthSession {
                usable: Arc::new(AtomicBool::new(false)),
                queries: queries.clone(),
                fail_query: false,
            })))),
        };
        let control = OperationControl::with_timeout(std::time::Duration::from_secs(1));

        assert_eq!(target.session_state().await, Some((id, false, false)));
        assert!(matches!(
            target.query("SELECT 1", &[], &control).await,
            Err(DriverError::Unsupported(_))
        ));
        assert_eq!(queries.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_ordinary_statement_error_keeps_a_usable_session() {
        let id = Uuid::new_v4();
        let queries = Arc::new(AtomicUsize::new(0));
        let target = StatementTarget::Session {
            id,
            shared: Arc::new(tokio::sync::Mutex::new(Some(Box::new(HealthSession {
                usable: Arc::new(AtomicBool::new(true)),
                queries: queries.clone(),
                fail_query: true,
            })))),
        };
        let control = OperationControl::with_timeout(std::time::Duration::from_secs(1));

        assert!(target.query("SELECT invalid", &[], &control).await.is_err());
        assert_eq!(target.session_state().await, Some((id, false, true)));
        assert_eq!(queries.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_retired_session_only_blocks_the_connection_that_lost_it() {
        let retired_connection = Uuid::new_v4();
        assert!(retired_connection_matches(
            Some((retired_connection, Uuid::new_v4())),
            Some(retired_connection)
        ));
        assert!(!retired_connection_matches(
            Some((retired_connection, Uuid::new_v4())),
            Some(Uuid::new_v4())
        ));
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
