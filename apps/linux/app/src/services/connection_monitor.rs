use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use tablepro_core::Connection;
use tablepro_transport::Tunnel;

use super::connection_service;
use super::database_service::{ConnectionHealth, EntryInner, ReconnectParams};
#[cfg(test)]
use tablepro_policy::AuditState;

const PING_INTERVAL: Duration = Duration::from_secs(30);
const TUNNEL_POLL_INTERVAL: Duration = Duration::from_secs(1);
use tablepro_app_services::reconnect::{BACKOFF_INITIAL, next_delay};

pub(super) async fn run(
    inner: Arc<Mutex<EntryInner>>,
    params: ReconnectParams,
    cancel: CancellationToken,
    fault: Arc<Notify>,
) {
    loop {
        let wake = tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(PING_INTERVAL) => Wake::Ping,
            _ = fault.notified() => Wake::Fault,
            _ = wait_for_closed_tunnel(&inner) => Wake::TunnelClosed,
        };

        if matches!(wake, Wake::Fault | Wake::TunnelClosed) {
            if matches!(wake, Wake::TunnelClosed) {
                tracing::warn!("SSH tunnel closed; starting reconnect");
            }
            if reconnect_loop(&inner, &params, &cancel, &fault).await.is_err() {
                return;
            }
            continue;
        }

        let conn = match snapshot_connection(&inner) {
            Some(c) => c,
            None => return,
        };

        let ping_result = tokio::select! {
            _ = cancel.cancelled() => return,
            _ = fault.notified() => Err(PingFailure::Reported),
            _ = wait_for_closed_tunnel(&inner) => Err(PingFailure::TunnelClosed),
            result = conn.ping() => result.map_err(PingFailure::Driver),
        };
        let reconnect = ping_result.is_err();
        match ping_result {
            Ok(()) => {}
            Err(PingFailure::Driver(error)) => {
                tracing::warn!(error = %error, "connection ping failed; starting reconnect");
            }
            Err(PingFailure::Reported) => {
                tracing::warn!("connection reported a transport fault; starting reconnect");
            }
            Err(PingFailure::TunnelClosed) => {
                tracing::warn!("SSH tunnel closed during ping; starting reconnect");
            }
        }
        if reconnect && reconnect_loop(&inner, &params, &cancel, &fault).await.is_err() {
            return;
        }
    }
}

#[derive(Clone, Copy)]
enum Wake {
    Ping,
    Fault,
    TunnelClosed,
}

enum PingFailure {
    Driver(tablepro_core::DriverError),
    Reported,
    TunnelClosed,
}

async fn wait_for_closed_tunnel(inner: &Arc<Mutex<EntryInner>>) {
    wait_until_closed(|| {
        inner
            .lock()
            .ok()
            .and_then(|entry| entry.tunnel.as_ref().map(Tunnel::is_closed))
    })
    .await;
}

async fn wait_until_closed(mut tunnel_is_closed: impl FnMut() -> Option<bool>) {
    loop {
        match tunnel_is_closed() {
            Some(true) => return,
            Some(false) => tokio::time::sleep(TUNNEL_POLL_INTERVAL).await,
            None => std::future::pending().await,
        }
    }
}

fn snapshot_connection(inner: &Arc<Mutex<EntryInner>>) -> Option<Arc<dyn Connection>> {
    inner.lock().ok().map(|g| g.connection.clone())
}

async fn reconnect_loop(
    inner: &Arc<Mutex<EntryInner>>,
    params: &ReconnectParams,
    cancel: &CancellationToken,
    fault: &Arc<Notify>,
) -> Result<(), ()> {
    let mut delay = BACKOFF_INITIAL;
    let mut attempt: u32 = 1;
    set_health(inner, ConnectionHealth::Reconnecting { attempt });
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Err(()),
            _ = tokio::time::sleep(delay) => {}
        }

        match try_reconnect(params, cancel).await {
            Err(failure) if failure.permanent => {
                tracing::warn!(error = %failure.message, attempt, "reconnect cannot succeed; giving up");
                set_health(
                    inner,
                    ConnectionHealth::Failed {
                        reason: failure.message,
                    },
                );
                return Err(());
            }
            Ok((conn, tunnel)) => {
                swap_connection(inner, conn, tunnel, fault);
                set_health(inner, ConnectionHealth::Healthy);
                tracing::info!(attempt, "reconnect succeeded");
                return Ok(());
            }
            Err(failure) => {
                tracing::warn!(error = %failure.message, attempt, delay_secs = delay.as_secs(), "reconnect failed; backing off");
                attempt += 1;
                set_health(inner, ConnectionHealth::Reconnecting { attempt });
                delay = next_delay(delay);
            }
        }
    }
}

async fn try_reconnect(
    params: &ReconnectParams,
    cancel: &CancellationToken,
) -> Result<(Box<dyn Connection>, Option<Tunnel>), connection_service::EstablishFailure> {
    connection_service::establish_classified_with_cancellation(
        params.driver.as_ref(),
        params.opts.clone(),
        params.ssh.clone(),
        &params.environment,
        cancel.clone(),
    )
    .await
}

fn swap_connection(
    inner: &Arc<Mutex<EntryInner>>,
    conn: Box<dyn Connection>,
    tunnel: Option<Tunnel>,
    fault: &Arc<Notify>,
) {
    conn.attach_fault_notify(Arc::clone(fault));
    let arc: Arc<dyn Connection> = Arc::from(conn);
    if let Ok(mut g) = inner.lock() {
        g.connection = arc;
        g.tunnel = tunnel;
        g.audit_state = Arc::new(g.audit_state.new_connection_generation());
    }
}

fn set_health(inner: &Arc<Mutex<EntryInner>>, health: ConnectionHealth) {
    if let Ok(mut g) = inner.lock() {
        g.health = health;
    }
}

#[cfg(test)]
#[path = "connection_monitor_tests.rs"]
mod tests;
