use std::sync::Arc;

use tablepro_core::{ConnectOptions, Connection};

use super::{CandidateBootstrap, OpenedCandidate};
use crate::services::connection_service;

pub(super) async fn open_candidate(
    driver: &dyn tablepro_core::DatabaseDriver,
    opts: ConnectOptions,
    ssh: Option<tablepro_transport::SshRoute>,
    ssh_environment: &tablepro_transport::SshEnvironment,
    timeout_secs: u32,
    cancellation: tokio_util::sync::CancellationToken,
    bootstrap: CandidateBootstrap<'_>,
) -> Result<OpenedCandidate, String> {
    let (conn, tunnel) =
        tablepro_transport::establish_with_cancellation(driver, opts, ssh, ssh_environment, cancellation)
            .await
            .map_err(connection_service::message)?;
    let conn: Arc<dyn tablepro_core::Connection> = Arc::from(conn);
    let guarded = bootstrap
        .guard_factory
        .guard(
            conn.clone(),
            bootstrap.metadata,
            tablepro_policy::Principal::human_gui(),
        )
        .map_err(|error| crate::services::error_text::driver_message(&error))?;
    let server_version = match guarded.server_version().await {
        Ok(version) => version,
        Err(tablepro_core::DriverError::PolicyDenied(message)) => return Err(message),
        Err(_) => None,
    };
    let control = crate::services::operation_control::bounded(timeout_secs);
    let tables = guarded
        .list_tables_controlled(&control)
        .await
        .map_err(|error| crate::services::error_text::driver_message(&error))?;
    let views = guarded
        .list_views_controlled(&control)
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
