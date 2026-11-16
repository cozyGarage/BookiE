use std::future::Future;

use bookie_core::{ConnectOptions, DatabaseDriver, DriverError, OperationControl};
use bookie_policy::{AdministrativeOperation, PolicyGuard};
use bookie_storage::SavedConnection;
use bookie_transport::{SshEnvironment, TransportError, connect_options_for, open_tool_endpoint, saved_ssh_route};

fn transport_error(error: TransportError) -> DriverError {
    match error {
        TransportError::Cancelled => DriverError::Cancelled,
        TransportError::Driver(error) => error,
        error => DriverError::Unsupported(error.to_string()),
    }
}

pub async fn with_subscription_endpoint<T, F, Fut>(
    guard: &PolicyGuard,
    driver: &dyn DatabaseDriver,
    saved: &SavedConnection,
    environment: &SshEnvironment,
    control: &OperationControl,
    work: F,
) -> Result<T, DriverError>
where
    F: FnOnce(ConnectOptions) -> Fut,
    Fut: Future<Output = Result<T, DriverError>>,
{
    let options = connect_options_for(saved).await.map_err(transport_error)?;
    let ssh = saved_ssh_route(saved).await.map_err(transport_error)?;
    let token = control.cancellation_token().clone();
    guard
        .run_administrative(AdministrativeOperation::Subscribe, control, async move {
            let (endpoint, _tunnel) = open_tool_endpoint(driver, options, ssh, environment, token)
                .await
                .map_err(transport_error)?;
            work(endpoint).await
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database_backup::tests::{UnusedDriver, guard};

    fn saved(port: u16) -> SavedConnection {
        serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(),
            "name": "cache",
            "driver_id": "redis",
            "host": "127.0.0.1",
            "port": port,
            "database": "",
            "username": "",
            "password": "",
            "auth_mode": "kerberos",
        }))
        .expect("saved connection")
    }

    #[tokio::test]
    async fn the_work_gets_the_saved_connections_endpoint_through_the_guard() {
        let control = bookie_core::OperationControl::new(tokio_util::sync::CancellationToken::new(), None);
        let environment = SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Refuse);
        let seen = with_subscription_endpoint(
            &guard(),
            &UnusedDriver,
            &saved(6390),
            &environment,
            &control,
            |opts| async move { Ok((opts.host, opts.port)) },
        )
        .await
        .expect("allowed and run");
        assert_eq!(seen, ("127.0.0.1".to_string(), 6390));
    }

    #[tokio::test]
    async fn an_error_from_the_work_comes_back_unchanged() {
        let control = bookie_core::OperationControl::new(tokio_util::sync::CancellationToken::new(), None);
        let environment = SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Refuse);
        let error = with_subscription_endpoint(
            &guard(),
            &UnusedDriver,
            &saved(6390),
            &environment,
            &control,
            |_| async { Err::<(), _>(DriverError::Unsupported("no such channel".into())) },
        )
        .await
        .expect_err("fails");
        assert!(matches!(error, DriverError::Unsupported(text) if text == "no such channel"));
    }
}
