use super::connect_failure::ConnectFailure;
use super::*;

pub(super) struct ConnectRequest {
    pub driver: Arc<dyn bookie_core::DatabaseDriver>,
    pub driver_id: String,
    pub bound_id: Option<Uuid>,
    pub hidden_schemas: Vec<String>,
    pub label: String,
    pub opts: ConnectOptions,
    pub ssh: Option<SshInputs>,
    pub ssh_environment: bookie_transport::SshEnvironment,
    pub audit_factory: bookie_transport::TransportAuditFactory,
    pub guard_factory: crate::services::database_service::CandidateGuardFactory,
    pub workspace: crate::services::workspace_state::WorkspaceStore,
    pub read_only: bool,
    pub environment: Environment,
    pub timeout_secs: u32,
    pub query_timeout_secs: Option<u32>,
    pub cancel: tokio_util::sync::CancellationToken,
}

pub(super) struct OpenedCandidate {
    pub conn: Arc<dyn bookie_core::Connection>,
    pub tunnel: Option<bookie_transport::Tunnel>,
    pub server_version: Option<String>,
    pub tables: Vec<bookie_core::TableInfo>,
    pub views: Vec<bookie_core::TableInfo>,
}

pub(super) struct CandidateBootstrap<'a> {
    pub guard_factory: &'a crate::services::database_service::CandidateGuardFactory,
    pub metadata: &'a crate::services::database_service::ConnectionMetadata,
    pub hidden_schemas: &'a [String],
}

pub(super) async fn open_candidate(
    driver: &dyn bookie_core::DatabaseDriver,
    opts: ConnectOptions,
    ssh: Option<bookie_transport::SshRoute>,
    ssh_environment: &bookie_transport::SshEnvironment,
    timeout_secs: u32,
    cancellation: tokio_util::sync::CancellationToken,
    bootstrap: CandidateBootstrap<'_>,
) -> Result<OpenedCandidate, ConnectFailure> {
    let (conn, tunnel) =
        bookie_transport::establish_with_cancellation(driver, opts, ssh, ssh_environment, cancellation.clone())
            .await
            .map_err(ConnectFailure::from_transport)?;
    let conn: Arc<dyn bookie_core::Connection> = Arc::from(conn);
    let guarded = bootstrap
        .guard_factory
        .guard(conn.clone(), bootstrap.metadata, bookie_policy::Principal::human_gui())
        .map_err(|failure| crate::services::error_text::open_failure_message(&failure))?;
    let control = crate::services::operation_control::bounded_with(timeout_secs, cancellation);
    let server_version = match guarded.server_version_controlled(&control).await {
        Ok(version) => version,
        Err(bookie_core::DriverError::PolicyDenied(message)) => return Err(message.into()),
        Err(error @ (bookie_core::DriverError::Cancelled | bookie_core::DriverError::TimedOut)) => {
            return Err(crate::services::error_text::driver_message(&error).into());
        }
        Err(bookie_core::DriverError::OperationOutcomeUnknown { source })
            if matches!(
                *source,
                bookie_core::DriverError::Cancelled | bookie_core::DriverError::TimedOut
            ) =>
        {
            return Err(crate::services::error_text::driver_message(&source).into());
        }
        Err(_) => None,
    };
    let tables = crate::services::catalog::list_tables_excluding(&guarded, bootstrap.hidden_schemas, &control)
        .await
        .map_err(|error| crate::services::error_text::driver_message(&error))?;
    let views = crate::services::catalog::list_views_excluding(&guarded, bootstrap.hidden_schemas, &control)
        .await
        .map_err(|error| crate::services::error_text::driver_message(&error))?;
    Ok(OpenedCandidate {
        conn,
        tunnel,
        server_version,
        tables,
        views,
    })
}
