pub use crate::reconnect::{is_permanent_failure, was_cancelled};
use std::sync::Arc;

use bookie_core::{ConnectOptions, Connection, DriverRegistry};
use bookie_storage::SavedConnection;
use bookie_transport::{SshEnvironment, SshRoute, Tunnel};
use uuid::Uuid;

use super::database_service::{CandidateGuardFactory, ConnectionMetadata, DatabaseService, ReconnectParams};
use super::open_failure::OpenFailure;

/// A connection that has authenticated and completed its initial metadata
/// query, but has not replaced the active application connection yet.
/// Keeping preparation separate from activation makes failed switches leave
/// the existing workspace and connection untouched.
pub struct PreparedConnection {
    pub tables: Vec<bookie_core::TableInfo>,
    pub views: Vec<bookie_core::TableInfo>,
    pub driver_id: String,
    id: uuid::Uuid,
    metadata: ConnectionMetadata,
    connection: Arc<dyn Connection>,
    tunnel: Option<Tunnel>,
    read_only: bool,
    params: ReconnectParams,
}

impl std::fmt::Debug for PreparedConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedConnection")
            .field("id", &self.id)
            .field("driver_id", &self.driver_id)
            .field("table_count", &self.tables.len())
            .field("view_count", &self.views.len())
            .finish_non_exhaustive()
    }
}

pub struct ActivatedConnection {
    pub id: Uuid,
    pub tables: Vec<bookie_core::TableInfo>,
    pub views: Vec<bookie_core::TableInfo>,
    pub driver_id: String,
    pub database: String,
}

impl PreparedConnection {
    /// The saved connection this would activate, so a caller can check
    /// `DatabaseService::is_active(id)` before committing to
    /// the switch -- tearing down whatever it currently owns only after
    /// confirming the target isn't already open in another window.
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn new(
        tables: Vec<bookie_core::TableInfo>,
        views: Vec<bookie_core::TableInfo>,
        driver_id: String,
        metadata: ConnectionMetadata,
        connection: Arc<dyn Connection>,
        tunnel: Option<Tunnel>,
        params: ReconnectParams,
    ) -> Self {
        Self {
            tables,
            views,
            driver_id,
            id: metadata.id,
            read_only: metadata.read_only,
            metadata,
            connection,
            tunnel,
            params,
        }
    }

    /// Returns `None` without changing anything when `id` is already active
    /// in another window; the caller should show that error and leave
    /// whatever it currently owns untouched. Callers that must guarantee
    /// success should check [`Self::id`] against
    /// `DatabaseService::is_active` before committing to the
    /// switch (tearing down a previous connection, clearing tabs) so a
    /// refusal here never needs to be rolled back.
    pub fn activate(self, database: &DatabaseService) -> Option<ActivatedConnection> {
        self.activate_with_generation(database).map(|(activated, _)| activated)
    }

    pub(crate) fn activate_with_generation(self, database: &DatabaseService) -> Option<(ActivatedConnection, Uuid)> {
        let id = self.id;
        let database_name = self.params.opts.database.clone();
        let generation = database.activate_shared(
            id,
            self.metadata,
            self.connection,
            self.tunnel,
            self.read_only,
            self.params,
        )?;
        Some((
            ActivatedConnection {
                id,
                tables: self.tables,
                views: self.views,
                driver_id: self.driver_id,
                database: database_name,
            },
            generation,
        ))
    }
}

pub async fn open_saved(
    registry: Arc<DriverRegistry>,
    saved: SavedConnection,
    hidden_schemas: Vec<String>,
    timeout_secs: u32,
    ssh_environment: SshEnvironment,
    cancellation: tokio_util::sync::CancellationToken,
    guard_factory: CandidateGuardFactory,
) -> Result<PreparedConnection, OpenFailure> {
    let driver = registry
        .get(&saved.driver_id)
        .ok_or_else(|| OpenFailure::Other(format!("driver {} not registered", saved.driver_id)))?;
    let id = saved.id;
    let environment = saved.environment;
    let read_only = saved.read_only;

    let ssh_hops = bookie_transport::saved_ssh_route(&saved)
        .await
        .map_err(OpenFailure::from)?;
    let mut opts = bookie_transport::connect_options_for(&saved)
        .await
        .map_err(OpenFailure::from)?;
    opts.application_name = Some("BookiE".into());

    let (conn, tunnel) = bookie_transport::establish_with_cancellation(
        &*driver,
        opts.clone(),
        ssh_hops.clone(),
        &ssh_environment,
        cancellation.clone(),
    )
    .await
    .map_err(OpenFailure::from)?;
    let connection: Arc<dyn Connection> = Arc::from(conn);
    let metadata = ConnectionMetadata {
        id,
        name: saved.name.clone(),
        driver_id: saved.driver_id.clone(),
        environment,
        read_only,
        server_version: None,
        query_timeout_secs: saved.query_timeout_secs,
    };
    let guarded = guard_factory.guard(connection.clone(), &metadata, bookie_policy::Principal::human_gui())?;
    let server_version = match guarded.server_version().await {
        Ok(version) => version,
        Err(error @ bookie_core::DriverError::PolicyDenied(_)) => return Err(OpenFailure::from_driver(&error)),
        Err(_) => None,
    };
    let control = crate::operation_control::bounded_with(timeout_secs, cancellation);
    let tables = crate::catalog::list_tables_excluding(&guarded, &hidden_schemas, &control)
        .await
        .map_err(|error| OpenFailure::from_driver(&error))?;
    let views = crate::catalog::list_views_excluding(&guarded, &hidden_schemas, &control)
        .await
        .map_err(|error| OpenFailure::from_driver(&error))?;
    let metadata = ConnectionMetadata {
        server_version,
        ..metadata
    };
    let params = ReconnectParams {
        driver,
        opts,
        ssh: ssh_hops,
        environment: ssh_environment,
    };
    Ok(PreparedConnection::new(
        tables,
        views,
        saved.driver_id,
        metadata,
        connection,
        tunnel,
        params,
    ))
}

pub struct EstablishFailure {
    pub error: OpenFailure,
    pub permanent: bool,
}

pub async fn establish_classified_with_cancellation(
    driver: &dyn bookie_core::DatabaseDriver,
    opts: ConnectOptions,
    ssh: Option<SshRoute>,
    environment: &SshEnvironment,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<(Box<dyn Connection>, Option<Tunnel>), EstablishFailure> {
    bookie_transport::establish_with_cancellation(driver, opts, ssh, environment, cancellation)
        .await
        .map_err(|error| EstablishFailure {
            permanent: is_permanent_failure(&error),
            error: OpenFailure::from(error),
        })
}
