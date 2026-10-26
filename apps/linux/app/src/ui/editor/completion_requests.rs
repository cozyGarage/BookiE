use relm4::ComponentSender;
use std::sync::Arc;

use crate::services::database_service::ConnectionIdentity;
use tablepro_core::Connection;

use super::{SchemaRequest, SqlEditor, SqlEditorInput};

impl SqlEditor {
    pub(super) fn on_session_toggle(&mut self, enabled: bool, sender: &ComponentSender<Self>) {
        self.schema_index.borrow_mut().invalidate_columns();
        self.on_session_toggled(enabled, sender);
    }

    pub(super) fn on_connection_identity_notification(&mut self, sender: &ComponentSender<Self>) {
        let changed = self
            .connection_id
            .and_then(|id| self.database.identity(id))
            .is_some_and(|identity| self.schema_index.borrow_mut().sync_connection(&identity));
        if changed {
            self.known_lexing.forget();
            (self.completion_refresh)();
        }
        self.on_connection_identity_changed(sender);
    }

    pub(super) fn refresh_completion_catalog(&mut self, snapshot: super::SchemaIndex) {
        self.schema_index.borrow_mut().refresh_editor_catalog(snapshot);
        (self.completion_refresh)();
    }

    pub(super) fn fetch_completion_columns(&mut self, tables: Vec<String>, sender: &ComponentSender<Self>) {
        if self.session_ending() || self.session_teardown_active {
            return;
        }
        let Some(connection_id) = self.connection_id else {
            return;
        };
        let Some((connection, identity)) = self.database.get_with_identity(connection_id) else {
            return;
        };
        if self
            .retired_session_connection_id
            .is_some_and(|(retired_id, _)| retired_id == connection_id)
            || self.session.as_ref().is_some_and(|session| {
                session.connection_id == connection_id && session.connection_identity.as_ref() != Some(&identity)
            })
        {
            return;
        }
        let (pending, token, grammar) = {
            let mut index = self.schema_index.borrow_mut();
            if !index.belongs_to_connection(&identity) {
                return;
            }
            let pending = index.request_missing_columns(tables);
            (pending, index.column_request_token(), index.grammar)
        };
        if pending.is_empty() {
            return;
        }
        let session = self.completion_session(connection_id, &identity);
        let context = CompletionFetch {
            connection,
            identity,
            session,
            use_session: self.metadata().is_some_and(|metadata| metadata.driver_id == "postgres"),
            grammar,
            timeout_secs: crate::services::operation_control::timeout_for(
                &self.preferences,
                &self.database,
                self.connection_id,
            ),
            token,
        };
        dispatch_fetch(pending, context, sender);
    }

    pub(super) fn complete_completion_columns(
        &mut self,
        identity: &ConnectionIdentity,
        request: &SchemaRequest,
        columns: Result<Vec<String>, ()>,
    ) {
        let Some(current) = self.connection_id.and_then(|id| self.database.identity(id)) else {
            return;
        };
        if &current != identity {
            return;
        }
        let mut index = self.schema_index.borrow_mut();
        if index.belongs_to_connection(identity) {
            index.complete_column_request(request, columns);
        }
    }

    fn completion_session(
        &self,
        connection_id: uuid::Uuid,
        identity: &ConnectionIdentity,
    ) -> Option<super::session_mode::SharedSession> {
        self.session.as_ref().and_then(|session| {
            (session.connection_id == connection_id && session.connection_identity.as_ref() == Some(identity))
                .then(|| session.shared.clone())
        })
    }
}

struct CompletionFetch {
    connection: Arc<dyn Connection>,
    identity: ConnectionIdentity,
    session: Option<super::session_mode::SharedSession>,
    use_session: bool,
    grammar: Option<tablepro_core::sql_syntax::SqlGrammar>,
    timeout_secs: u32,
    token: tokio_util::sync::CancellationToken,
}

fn dispatch_fetch(pending: Vec<SchemaRequest>, context: CompletionFetch, sender: &ComponentSender<SqlEditor>) {
    let sender = sender.clone();
    let result_sender = sender.clone();
    sender.command(move |_, shutdown| {
        shutdown
            .register(async move {
                for request in pending {
                    if context.token.is_cancelled() {
                        break;
                    }
                    let (request, columns) = match (context.use_session, context.session.as_ref()) {
                        (true, Some(session)) => {
                            tablepro_app_services::completion::fetch_session_column_names(
                                session.clone(),
                                request,
                                context.grammar,
                                context.timeout_secs,
                                context.token.clone(),
                            )
                            .await
                        }
                        _ => {
                            tablepro_app_services::completion::fetch_column_names(
                                context.connection.clone(),
                                request,
                                context.grammar,
                                context.timeout_secs,
                                context.token.clone(),
                            )
                            .await
                        }
                    };
                    result_sender.input(SqlEditorInput::CompletionColumnsFetched {
                        identity: context.identity.clone(),
                        request,
                        columns,
                    });
                }
            })
            .drop_on_shutdown()
    });
}
