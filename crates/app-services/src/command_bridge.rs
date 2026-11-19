use bookie_core::{CatalogObject, CatalogObjectKind, ColumnInfo, OperationControl, QueryResult, TableInfo};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::sync::mpsc;

use crate::database_service::ConnectionOverview;
use crate::driver_error_presentation::DriverFailurePresentation;
use crate::read_session::{ConnectionSummary, list_saved_connections};
use crate::read_session_store::{OpenedSession, ReadSessionStore, SessionId, SubscriptionRequest, SubscriptionSample};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Epoch(pub u64);

#[derive(Debug)]
pub enum Command {
    ListSavedConnections {
        epoch: Epoch,
    },
    OpenSavedConnection {
        epoch: Epoch,
        connection_id: uuid::Uuid,
    },
    OpenConnectionFile {
        epoch: Epoch,
        path: PathBuf,
    },
    CloseSession {
        epoch: Epoch,
        session_id: SessionId,
    },
    RunQuery {
        epoch: Epoch,
        session_id: SessionId,
        sql: String,
        params: Vec<bookie_core::Value>,
    },
    ReadPage {
        epoch: Epoch,
        session_id: SessionId,
        table: TableInfo,
        offset: u64,
        limit: u64,
    },
    ReadColumns {
        epoch: Epoch,
        session_id: SessionId,
        table: TableInfo,
    },
    ReadCatalog {
        epoch: Epoch,
        session_id: SessionId,
        kind: CatalogObjectKind,
    },
    ReadDataDictionary {
        epoch: Epoch,
        session_id: SessionId,
        capabilities: bookie_core::DriverCapabilities,
    },
    ReadConnectionStatus {
        epoch: Epoch,
        session_id: SessionId,
    },
    SampleSubscription {
        epoch: Epoch,
        session_id: SessionId,
        request: SubscriptionRequest,
    },
}

#[derive(Debug)]
pub enum Event {
    SavedConnectionsListed {
        epoch: Epoch,
        outcome: Result<Vec<ConnectionSummary>, String>,
    },
    SessionOpened {
        epoch: Epoch,
        outcome: Result<OpenedSession, String>,
    },
    SessionClosed {
        epoch: Epoch,
        closed: bool,
    },
    QueryCompleted {
        epoch: Epoch,
        outcome: Result<QueryResult, DriverFailurePresentation>,
    },
    PageRead {
        epoch: Epoch,
        outcome: Result<QueryResult, DriverFailurePresentation>,
    },
    ColumnsRead {
        epoch: Epoch,
        outcome: Result<Vec<ColumnInfo>, DriverFailurePresentation>,
    },
    CatalogRead {
        epoch: Epoch,
        outcome: Result<Vec<CatalogObject>, DriverFailurePresentation>,
    },
    DataDictionaryRead {
        epoch: Epoch,
        outcome: Result<QueryResult, DriverFailurePresentation>,
    },
    ConnectionStatusRead {
        epoch: Epoch,
        outcome: Vec<ConnectionOverview>,
    },
    SubscriptionSampled {
        epoch: Epoch,
        outcome: Result<SubscriptionSample, DriverFailurePresentation>,
    },
}

impl Event {
    pub fn epoch(&self) -> Epoch {
        match self {
            Self::SavedConnectionsListed { epoch, .. }
            | Self::SessionOpened { epoch, .. }
            | Self::SessionClosed { epoch, .. }
            | Self::QueryCompleted { epoch, .. }
            | Self::PageRead { epoch, .. }
            | Self::ColumnsRead { epoch, .. }
            | Self::CatalogRead { epoch, .. }
            | Self::DataDictionaryRead { epoch, .. }
            | Self::ConnectionStatusRead { epoch, .. }
            | Self::SubscriptionSampled { epoch, .. } => *epoch,
        }
    }

    pub fn into_current(self, current: Epoch) -> Option<Self> {
        (self.epoch() == current).then_some(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSinkError {
    Full,
    Closed,
}

#[derive(Clone, Copy)]
pub enum DispatchContext<'a> {
    SavedConnections,
    Sessions(&'a ReadSessionStore),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchError {
    EventSink(EventSinkError),
    InvalidContext,
}

impl From<EventSinkError> for DispatchError {
    fn from(error: EventSinkError) -> Self {
        Self::EventSink(error)
    }
}

pub trait EventSink {
    fn try_emit(&self, event: Event) -> Result<(), EventSinkError>;
}

pub struct ChannelEventSink {
    sender: mpsc::Sender<Event>,
}

impl ChannelEventSink {
    pub fn new(sender: mpsc::Sender<Event>) -> Self {
        Self { sender }
    }
}

impl EventSink for ChannelEventSink {
    fn try_emit(&self, event: Event) -> Result<(), EventSinkError> {
        self.sender.try_send(event).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => EventSinkError::Full,
            mpsc::error::TrySendError::Closed(_) => EventSinkError::Closed,
        })
    }
}

pub fn dispatch<'a>(
    context: DispatchContext<'a>,
    command: Command,
    control: &'a OperationControl,
    sink: &'a dyn EventSink,
) -> Pin<Box<dyn Future<Output = Result<(), DispatchError>> + 'a>> {
    Box::pin(async move {
        match (context, command) {
            (DispatchContext::SavedConnections, Command::ListSavedConnections { epoch }) => {
                let outcome = match bookie_core::check_pre_dispatch(control) {
                    Ok(()) => list_saved_connections().await,
                    Err(error) => Err(error.to_string()),
                };
                sink.try_emit(Event::SavedConnectionsListed { epoch, outcome })?;
            }
            (DispatchContext::Sessions(sessions), command) => {
                Box::pin(dispatch_session_command(sessions, command, control, sink)).await?;
            }
            _ => return Err(DispatchError::InvalidContext),
        }
        Ok(())
    })
}

async fn dispatch_session_command(
    sessions: &ReadSessionStore,
    command: Command,
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    match command {
        Command::OpenSavedConnection { epoch, connection_id } => {
            Box::pin(dispatch_open_saved(sessions, epoch, connection_id, control, sink)).await?;
        }
        Command::OpenConnectionFile { epoch, path } => {
            Box::pin(dispatch_open_file(sessions, epoch, path, control, sink)).await?;
        }
        Command::CloseSession { epoch, session_id } => {
            sink.try_emit(Event::SessionClosed {
                epoch,
                closed: sessions.close(session_id),
            })?;
        }
        Command::ListSavedConnections { .. } => return Err(DispatchError::InvalidContext),
        read => Box::pin(dispatch_read_command(sessions, read, control, sink)).await?,
    }
    Ok(())
}

async fn dispatch_read_command(
    sessions: &ReadSessionStore,
    command: Command,
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    match command {
        Command::RunQuery {
            epoch,
            session_id,
            sql,
            params,
        } => {
            Box::pin(dispatch_query(
                sessions,
                epoch,
                session_id,
                (sql, params),
                control,
                sink,
            ))
            .await?;
        }
        Command::ReadPage {
            epoch,
            session_id,
            table,
            offset,
            limit,
        } => {
            Box::pin(dispatch_page(
                sessions,
                epoch,
                session_id,
                (table, offset, limit),
                control,
                sink,
            ))
            .await?;
        }
        Command::ReadColumns {
            epoch,
            session_id,
            table,
        } => {
            Box::pin(dispatch_columns(sessions, epoch, session_id, table, control, sink)).await?;
        }
        other => Box::pin(dispatch_metadata_command(sessions, other, control, sink)).await?,
    }
    Ok(())
}

async fn dispatch_metadata_command(
    sessions: &ReadSessionStore,
    command: Command,
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    match command {
        Command::ReadCatalog {
            epoch,
            session_id,
            kind,
        } => {
            Box::pin(dispatch_catalog(sessions, epoch, session_id, kind, control, sink)).await?;
        }
        Command::ReadDataDictionary {
            epoch,
            session_id,
            capabilities,
        } => {
            Box::pin(dispatch_data_dictionary(
                sessions,
                epoch,
                session_id,
                capabilities,
                control,
                sink,
            ))
            .await?;
        }
        Command::ReadConnectionStatus { epoch, session_id } => {
            dispatch_connection_status(sessions, epoch, session_id, sink)?;
        }
        Command::SampleSubscription {
            epoch,
            session_id,
            request,
        } => {
            Box::pin(dispatch_subscription(
                sessions, epoch, session_id, request, control, sink,
            ))
            .await?;
        }
        _ => return Err(DispatchError::InvalidContext),
    }
    Ok(())
}

async fn dispatch_columns(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    session_id: SessionId,
    table: TableInfo,
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let outcome = match sessions.get(session_id) {
        Some(session) => session.read_columns(&table, control).await,
        None => Err(DriverFailurePresentation::Disconnected),
    };
    sink.try_emit(Event::ColumnsRead { epoch, outcome })?;
    Ok(())
}

fn dispatch_connection_status(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    session_id: SessionId,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let outcome = sessions
        .get(session_id)
        .map_or_else(Vec::new, |session| session.overview());
    sink.try_emit(Event::ConnectionStatusRead { epoch, outcome })?;
    Ok(())
}

async fn dispatch_subscription(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    session_id: SessionId,
    request: SubscriptionRequest,
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let outcome = sessions.sample_subscription(session_id, request, control).await;
    sink.try_emit(Event::SubscriptionSampled { epoch, outcome })?;
    Ok(())
}

async fn dispatch_page(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    session_id: SessionId,
    (table, offset, limit): (TableInfo, u64, u64),
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let outcome = match sessions.get(session_id) {
        Some(session) => session.read_page(&table, offset, limit, control).await,
        None => Err(DriverFailurePresentation::Disconnected),
    };
    sink.try_emit(Event::PageRead { epoch, outcome })?;
    Ok(())
}

async fn dispatch_data_dictionary(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    session_id: SessionId,
    capabilities: bookie_core::DriverCapabilities,
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let outcome = match sessions.get(session_id) {
        Some(session) => session.read_data_dictionary(capabilities, control).await,
        None => Err(DriverFailurePresentation::Disconnected),
    };
    sink.try_emit(Event::DataDictionaryRead { epoch, outcome })?;
    Ok(())
}

async fn dispatch_catalog(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    session_id: SessionId,
    kind: CatalogObjectKind,
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let outcome = match sessions.get(session_id) {
        Some(session) => session.read_catalog(kind, control).await,
        None => Err(DriverFailurePresentation::Disconnected),
    };
    sink.try_emit(Event::CatalogRead { epoch, outcome })?;
    Ok(())
}

async fn dispatch_open_saved(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    connection_id: uuid::Uuid,
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let outcome = match bookie_core::check_pre_dispatch(control) {
        Ok(()) => sessions.open_saved(connection_id, control).await,
        Err(error) => Err(error.to_string()),
    };
    emit_opened(sessions, epoch, outcome, sink)
}

async fn dispatch_open_file(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    path: PathBuf,
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let outcome = match bookie_core::check_pre_dispatch(control) {
        Ok(()) => sessions.open_file(&path, control).await,
        Err(error) => Err(error.to_string()),
    };
    emit_opened(sessions, epoch, outcome, sink)
}

fn emit_opened(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    outcome: Result<OpenedSession, String>,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let opened_id = outcome.as_ref().ok().map(|opened| opened.session_id);
    if let Err(error) = sink.try_emit(Event::SessionOpened { epoch, outcome }) {
        if let Some(session_id) = opened_id {
            sessions.close(session_id);
        }
        return Err(error.into());
    }
    Ok(())
}

async fn dispatch_query(
    sessions: &ReadSessionStore,
    epoch: Epoch,
    session_id: SessionId,
    (sql, params): (String, Vec<bookie_core::Value>),
    control: &OperationControl,
    sink: &dyn EventSink,
) -> Result<(), DispatchError> {
    let outcome = match sessions.get(session_id) {
        Some(session) if params.is_empty() => session.run_query(&sql, control).await,
        Some(session) => session.run_query_params(&sql, &params, control).await,
        None => Err(DriverFailurePresentation::Disconnected),
    };
    sink.try_emit(Event::QueryCompleted { epoch, outcome })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_event_cannot_be_applied_to_a_newer_epoch() {
        let event = Event::QueryCompleted {
            epoch: Epoch(3),
            outcome: Err(DriverFailurePresentation::Cancelled),
        };

        assert!(event.into_current(Epoch(4)).is_none());
    }

    #[test]
    fn a_stale_saved_connection_list_cannot_replace_a_newer_request() {
        let event = Event::SavedConnectionsListed {
            epoch: Epoch(3),
            outcome: Ok(vec![crate::read_session::ConnectionSummary {
                id: uuid::Uuid::nil(),
                name: "local".into(),
                driver_id: "sqlite".into(),
                read_only: true,
            }]),
        };

        assert!(event.into_current(Epoch(4)).is_none());
    }

    #[tokio::test]
    async fn a_full_channel_returns_without_waiting_for_capacity() {
        let (sender, _receiver) = mpsc::channel(1);
        let sink = ChannelEventSink::new(sender);
        let event = || Event::QueryCompleted {
            epoch: Epoch(1),
            outcome: Err(DriverFailurePresentation::Cancelled),
        };

        sink.try_emit(event()).unwrap();

        assert_eq!(sink.try_emit(event()), Err(EventSinkError::Full));
    }

    #[tokio::test]
    async fn a_closed_channel_reports_that_events_cannot_be_delivered() {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        let sink = ChannelEventSink::new(sender);
        let event = Event::QueryCompleted {
            epoch: Epoch(1),
            outcome: Err(DriverFailurePresentation::Cancelled),
        };

        assert_eq!(sink.try_emit(event), Err(EventSinkError::Closed));
    }

    #[tokio::test]
    async fn a_session_command_is_refused_without_a_session_context() {
        let (sender, mut receiver) = mpsc::channel(1);
        let sink = ChannelEventSink::new(sender);
        let control = OperationControl::with_timeout(std::time::Duration::from_secs(1));

        let result = dispatch(
            DispatchContext::SavedConnections,
            Command::RunQuery {
                epoch: Epoch(9),
                session_id: SessionId::new(),
                sql: "SELECT 1".into(),
                params: Vec::new(),
            },
            &control,
            &sink,
        )
        .await;

        assert_eq!(result, Err(DispatchError::InvalidContext));
        assert!(receiver.try_recv().is_err());
    }
}
