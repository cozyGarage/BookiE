use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use bookie_core::{ConnectOptions, DriverError, DriverRegistry, OperationControl, TableInfo};
use uuid::Uuid;

use crate::database_service::DatabaseService;
use crate::read_session::ReadSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(Uuid);

impl SessionId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedSession {
    pub session_id: SessionId,
    pub label: String,
    pub driver_id: String,
    pub tables: Vec<bookie_core::TableInfo>,
    pub supports_snapshot_cursor: bool,
}

pub struct ReadSessionStore {
    registry: Arc<DriverRegistry>,
    database: Arc<DatabaseService>,
    sessions: RwLock<HashMap<SessionId, Arc<ReadSession>>>,
}

impl ReadSessionStore {
    pub fn new(registry: Arc<DriverRegistry>, database: Arc<DatabaseService>) -> Self {
        Self {
            registry,
            database,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn open_saved(&self, id: Uuid, control: &OperationControl) -> Result<OpenedSession, String> {
        let saved = bookie_storage::load_connections()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|saved| saved.id == id)
            .ok_or_else(|| format!("connection {id} not found"))?;
        let label = saved.name.clone();
        self.open(saved, label, control).await
    }

    pub async fn open_file(&self, path: &std::path::Path, control: &OperationControl) -> Result<OpenedSession, String> {
        let file = crate::open_request::classify_file(path).map_err(|error| format!("{error:?}"))?;
        let label = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let mut saved = crate::open_request::file_connection(&file);
        saved.read_only = true;
        self.open(saved, label, control).await
    }

    pub(crate) fn get(&self, id: SessionId) -> Option<Arc<ReadSession>> {
        self.sessions
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&id)
            .cloned()
    }

    pub fn supports_snapshot_cursor(&self, id: SessionId) -> bool {
        self.get(id).is_some_and(|session| session.supports_snapshot_cursor())
    }

    pub async fn open_table_cursor(
        &self,
        id: SessionId,
        table: &TableInfo,
        control: &OperationControl,
    ) -> Result<
        Option<crate::read_session::PostgresTableCursor>,
        crate::driver_error_presentation::DriverFailurePresentation,
    > {
        let session = self
            .get(id)
            .ok_or(crate::driver_error_presentation::DriverFailurePresentation::Disconnected)?;
        session.open_table_cursor(table, control).await
    }

    pub async fn with_subscription<T, F, Fut>(
        &self,
        id: SessionId,
        registry: &DriverRegistry,
        control: &OperationControl,
        work: F,
    ) -> Result<T, crate::driver_error_presentation::DriverFailurePresentation>
    where
        F: FnOnce(ConnectOptions) -> Fut,
        Fut: std::future::Future<Output = Result<T, DriverError>>,
    {
        let session = self
            .get(id)
            .ok_or(crate::driver_error_presentation::DriverFailurePresentation::Disconnected)?;
        session.with_subscription(registry, control, work).await
    }

    pub fn close(&self, id: SessionId) -> bool {
        self.sessions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id)
            .is_some()
    }

    async fn open(
        &self,
        saved: bookie_storage::SavedConnection,
        label: String,
        control: &OperationControl,
    ) -> Result<OpenedSession, String> {
        let session = Box::pin(ReadSession::open_controlled(
            saved,
            Arc::clone(&self.registry),
            Arc::clone(&self.database),
            control.cancellation_token().clone(),
        ))
        .await
        .map_err(|failure| format!("{failure:?}"))?;
        let summary = OpenedSession {
            session_id: SessionId::new(),
            label,
            driver_id: session.driver_id.clone(),
            tables: session.tables.clone(),
            supports_snapshot_cursor: session.supports_snapshot_cursor(),
        };
        self.sessions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(summary.session_id, Arc::new(session));
        Ok(summary)
    }
}
