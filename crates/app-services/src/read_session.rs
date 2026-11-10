use std::sync::Arc;

use bookie_core::{Connection, DriverRegistry, OperationControl, QueryResult, TableInfo};
use bookie_policy::Principal;
use bookie_storage::SavedConnection;
use bookie_transport::SshEnvironment;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::connection_service::open_saved;
use crate::database_service::DatabaseService;
use crate::driver_error_presentation::{DriverFailurePresentation, classify};
use crate::open_failure::OpenFailure;

const OPEN_TIMEOUT_SECS: u32 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionSummary {
    pub id: Uuid,
    pub name: String,
    pub driver_id: String,
    pub read_only: bool,
}

pub async fn list_saved_connections() -> Result<Vec<ConnectionSummary>, String> {
    let saved = bookie_storage::load_connections()
        .await
        .map_err(|error| error.to_string())?;
    Ok(saved
        .into_iter()
        .map(|saved| ConnectionSummary {
            id: saved.id,
            name: saved.name,
            driver_id: saved.driver_id,
            read_only: saved.read_only,
        })
        .collect())
}

pub struct ReadSession {
    database: Arc<DatabaseService>,
    id: Uuid,
    generation: Uuid,
    pub driver_id: String,
    pub tables: Vec<TableInfo>,
}

impl ReadSession {
    pub async fn open_saved(id: Uuid, registry: Arc<DriverRegistry>) -> Result<Self, OpenFailure> {
        Self::open_saved_controlled(id, registry, CancellationToken::new()).await
    }

    pub async fn open_saved_controlled(
        id: Uuid,
        registry: Arc<DriverRegistry>,
        cancellation: CancellationToken,
    ) -> Result<Self, OpenFailure> {
        check_open_cancellation(&cancellation)?;
        let saved = bookie_storage::load_connections()
            .await
            .map_err(|error| OpenFailure::Other(error.to_string()))?
            .into_iter()
            .find(|saved| saved.id == id)
            .ok_or_else(|| OpenFailure::Other(format!("connection {id} not found")))?;
        check_open_cancellation(&cancellation)?;
        Self::open_controlled(saved, registry, Arc::new(DatabaseService::new()), cancellation).await
    }

    pub async fn open_file(path: &std::path::Path, registry: Arc<DriverRegistry>) -> Result<Self, OpenFailure> {
        Self::open_file_controlled(path, registry, CancellationToken::new()).await
    }

    pub async fn open_file_controlled(
        path: &std::path::Path,
        registry: Arc<DriverRegistry>,
        cancellation: CancellationToken,
    ) -> Result<Self, OpenFailure> {
        check_open_cancellation(&cancellation)?;
        let file =
            crate::open_request::classify_file(path).map_err(|error| OpenFailure::Other(format!("{error:?}")))?;
        let mut saved = crate::open_request::file_connection(&file);
        saved.read_only = true;
        Self::open_controlled(saved, registry, Arc::new(DatabaseService::new()), cancellation).await
    }

    pub async fn open(
        saved: SavedConnection,
        registry: Arc<DriverRegistry>,
        database: Arc<DatabaseService>,
    ) -> Result<Self, OpenFailure> {
        Self::open_controlled(saved, registry, database, CancellationToken::new()).await
    }

    pub async fn open_controlled(
        saved: SavedConnection,
        registry: Arc<DriverRegistry>,
        database: Arc<DatabaseService>,
        cancellation: CancellationToken,
    ) -> Result<Self, OpenFailure> {
        check_open_cancellation(&cancellation)?;
        let prepared = open_saved(
            registry,
            saved,
            Vec::new(),
            OPEN_TIMEOUT_SECS,
            SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Refuse),
            cancellation.clone(),
            database.candidate_guard_factory(),
        )
        .await?;
        check_open_cancellation(&cancellation)?;
        let id = prepared.id();
        let mut tables = prepared.tables.clone();
        tables.extend(prepared.views.iter().cloned());
        let (activated, generation) = prepared
            .activate_with_generation(&database)
            .ok_or_else(|| OpenFailure::Other("the connection is already open in another window".into()))?;
        Ok(Self {
            database,
            id,
            generation,
            driver_id: activated.driver_id,
            tables,
        })
    }

    pub async fn read_page(
        &self,
        table: &TableInfo,
        offset: u64,
        limit: u64,
        control: &OperationControl,
    ) -> Result<QueryResult, DriverFailurePresentation> {
        if limit == 0 || limit > bookie_core::MAX_QUERY_CURSOR_PAGE_ROWS as u64 {
            return Err(DriverFailurePresentation::Unsupported(format!(
                "Page size must be between 1 and {} rows.",
                bookie_core::MAX_QUERY_CURSOR_PAGE_ROWS
            )));
        }
        let guard = self
            .database
            .guard_for_generation(self.id, self.generation, Principal::human_gui())
            .ok_or(DriverFailurePresentation::Disconnected)?;
        guard
            .fetch_rows_controlled(table.schema.as_deref(), &table.name, offset, limit, control)
            .await
            .map_err(|error| classify(&error))
    }
}

fn check_open_cancellation(cancellation: &CancellationToken) -> Result<(), OpenFailure> {
    let control = OperationControl::new(cancellation.clone(), None);
    bookie_core::check_pre_dispatch(&control).map_err(|error| OpenFailure::from_driver(&error))
}

impl ReadSession {
    pub async fn run_query(
        &self,
        sql: &str,
        control: &OperationControl,
    ) -> Result<QueryResult, DriverFailurePresentation> {
        let guard = self
            .database
            .guard(self.id, Principal::human_gui())
            .ok_or(DriverFailurePresentation::Disconnected)?;
        guard
            .query_controlled(sql, control)
            .await
            .map_err(|error| classify(&error))
    }
}

impl ReadSession {
    pub async fn with_subscription<T, F, Fut>(
        &self,
        registry: &bookie_core::DriverRegistry,
        control: &OperationControl,
        work: F,
    ) -> Result<T, DriverFailurePresentation>
    where
        F: FnOnce(bookie_core::ConnectOptions) -> Fut,
        Fut: std::future::Future<Output = Result<T, bookie_core::DriverError>>,
    {
        let guard = self
            .database
            .guard(self.id, Principal::human_gui())
            .ok_or(DriverFailurePresentation::Disconnected)?;
        let saved = bookie_storage::load_connections()
            .await
            .map_err(|error| DriverFailurePresentation::Unsupported(error.to_string()))?
            .into_iter()
            .find(|saved| saved.id == self.id)
            .ok_or_else(|| {
                DriverFailurePresentation::Unsupported("only a saved connection can be subscribed to".into())
            })?;
        let driver = registry.get(&self.driver_id).ok_or_else(|| {
            DriverFailurePresentation::Unsupported(format!("driver {} not registered", self.driver_id))
        })?;
        let environment = bookie_transport::SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Refuse);
        crate::database_subscription::with_subscription_endpoint(&guard, &*driver, &saved, &environment, control, work)
            .await
            .map_err(|error| classify(&error))
    }
}

impl Drop for ReadSession {
    fn drop(&mut self) {
        self.database.close_generation(self.id, self.generation);
    }
}
