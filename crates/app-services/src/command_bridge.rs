use bookie_core::{OperationControl, QueryResult, TableInfo};
use tokio::sync::mpsc;

use crate::database_service::ConnectionOverview;
use crate::driver_error_presentation::DriverFailurePresentation;
use crate::read_session::{ConnectionSummary, ReadSession, list_saved_connections};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Epoch(pub u64);

#[derive(Debug)]
pub enum Command {
    ListSavedConnections {
        epoch: Epoch,
    },
    RunQuery {
        epoch: Epoch,
        sql: String,
    },
    ReadPage {
        epoch: Epoch,
        table: TableInfo,
        offset: u64,
        limit: u64,
    },
    ReadConnectionStatus {
        epoch: Epoch,
    },
}

#[derive(Debug)]
pub enum Event {
    SavedConnectionsListed {
        epoch: Epoch,
        outcome: Result<Vec<ConnectionSummary>, String>,
    },
    QueryCompleted {
        epoch: Epoch,
        outcome: Result<QueryResult, DriverFailurePresentation>,
    },
    PageRead {
        epoch: Epoch,
        outcome: Result<QueryResult, DriverFailurePresentation>,
    },
    ConnectionStatusRead {
        epoch: Epoch,
        outcome: Vec<ConnectionOverview>,
    },
}

impl Event {
    pub fn epoch(&self) -> Epoch {
        match self {
            Self::SavedConnectionsListed { epoch, .. }
            | Self::QueryCompleted { epoch, .. }
            | Self::PageRead { epoch, .. }
            | Self::ConnectionStatusRead { epoch, .. } => *epoch,
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
    Session(&'a ReadSession),
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

pub async fn dispatch(
    context: DispatchContext<'_>,
    command: Command,
    control: &OperationControl,
    sink: &impl EventSink,
) -> Result<(), DispatchError> {
    match (context, command) {
        (DispatchContext::SavedConnections, Command::ListSavedConnections { epoch }) => {
            let outcome = match bookie_core::check_pre_dispatch(control) {
                Ok(()) => list_saved_connections().await,
                Err(error) => Err(error.to_string()),
            };
            sink.try_emit(Event::SavedConnectionsListed { epoch, outcome })?;
        }
        (DispatchContext::Session(session), Command::RunQuery { epoch, sql }) => {
            let outcome = session.run_query(&sql, control).await;
            sink.try_emit(Event::QueryCompleted { epoch, outcome })?;
        }
        (
            DispatchContext::Session(session),
            Command::ReadPage {
                epoch,
                table,
                offset,
                limit,
            },
        ) => {
            let outcome = session.read_page(&table, offset, limit, control).await;
            sink.try_emit(Event::PageRead { epoch, outcome })?;
        }
        (DispatchContext::Session(session), Command::ReadConnectionStatus { epoch }) => {
            sink.try_emit(Event::ConnectionStatusRead {
                epoch,
                outcome: session.overview(),
            })?;
        }
        _ => return Err(DispatchError::InvalidContext),
    }
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
                sql: "SELECT 1".into(),
            },
            &control,
            &sink,
        )
        .await;

        assert_eq!(result, Err(DispatchError::InvalidContext));
        assert!(receiver.try_recv().is_err());
    }
}
