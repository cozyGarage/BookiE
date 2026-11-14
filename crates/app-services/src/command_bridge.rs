use bookie_core::{OperationControl, QueryResult, TableInfo};
use tokio::sync::mpsc;

use crate::driver_error_presentation::DriverFailurePresentation;
use crate::read_session::ReadSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Epoch(pub u64);

#[derive(Debug)]
pub enum Command {
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
}

#[derive(Debug)]
pub enum Event {
    QueryCompleted {
        epoch: Epoch,
        outcome: Result<QueryResult, DriverFailurePresentation>,
    },
    PageRead {
        epoch: Epoch,
        outcome: Result<QueryResult, DriverFailurePresentation>,
    },
}

impl Event {
    pub fn epoch(&self) -> Epoch {
        match self {
            Self::QueryCompleted { epoch, .. } | Self::PageRead { epoch, .. } => *epoch,
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
    session: &ReadSession,
    command: Command,
    control: &OperationControl,
    sink: &impl EventSink,
) -> Result<(), EventSinkError> {
    match command {
        Command::RunQuery { epoch, sql } => {
            let outcome = session.run_query(&sql, control).await;
            sink.try_emit(Event::QueryCompleted { epoch, outcome })
        }
        Command::ReadPage {
            epoch,
            table,
            offset,
            limit,
        } => {
            let outcome = session.read_page(&table, offset, limit, control).await;
            sink.try_emit(Event::PageRead { epoch, outcome })
        }
    }
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
}
