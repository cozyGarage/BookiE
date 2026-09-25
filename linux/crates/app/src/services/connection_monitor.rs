use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use tablepro_core::Connection;
use tablepro_transport::Tunnel;

use super::connection_service;
use super::database_service::{ConnectionHealth, EntryInner, ReconnectParams};

const PING_INTERVAL: Duration = Duration::from_secs(30);
const BACKOFF_INITIAL: Duration = Duration::from_secs(5);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

pub(super) async fn run(
    inner: Arc<Mutex<EntryInner>>,
    params: ReconnectParams,
    cancel: CancellationToken,
    fault: Arc<Notify>,
) {
    loop {
        // A reported fault skips the ping: the driver stopped
        // mid-protocol, so a successful ping would not prove the
        // connection is usable.
        let faulted = tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(PING_INTERVAL) => false,
            _ = fault.notified() => true,
        };

        if faulted {
            if reconnect_loop(&inner, &params, &cancel).await.is_err() {
                return;
            }
            continue;
        }

        let conn = match snapshot_connection(&inner) {
            Some(c) => c,
            None => return,
        };

        if let Err(e) = conn.ping().await {
            tracing::warn!(error = %e, "connection ping failed; starting reconnect");
            if reconnect_loop(&inner, &params, &cancel).await.is_err() {
                return;
            }
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
) -> Result<(), ()> {
    let mut delay = BACKOFF_INITIAL;
    let mut attempt: u32 = 1;
    set_health(inner, ConnectionHealth::Reconnecting { attempt });
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Err(()),
            _ = tokio::time::sleep(delay) => {}
        }

        match try_reconnect(params).await {
            Ok((conn, tunnel)) => {
                swap_connection(inner, conn, tunnel);
                set_health(inner, ConnectionHealth::Healthy);
                tracing::info!(attempt, "reconnect succeeded");
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(error = %e, attempt, delay_secs = delay.as_secs(), "reconnect failed; backing off");
                attempt += 1;
                set_health(inner, ConnectionHealth::Reconnecting { attempt });
                delay = next_delay(delay);
            }
        }
    }
}

fn next_delay(prev: Duration) -> Duration {
    std::cmp::min(prev.saturating_mul(2), BACKOFF_MAX)
}

async fn try_reconnect(params: &ReconnectParams) -> Result<(Box<dyn Connection>, Option<Tunnel>), String> {
    connection_service::establish(
        params.driver.as_ref(),
        params.opts.clone(),
        params.ssh.clone(),
        &params.environment,
    )
    .await
}

fn swap_connection(inner: &Arc<Mutex<EntryInner>>, conn: Box<dyn Connection>, tunnel: Option<Tunnel>) {
    let arc: Arc<dyn Connection> = Arc::from(conn);
    if let Ok(mut g) = inner.lock() {
        g.connection = arc;
        g.tunnel = tunnel;
    }
}

fn set_health(inner: &Arc<Mutex<EntryInner>>, health: ConnectionHealth) {
    if let Ok(mut g) = inner.lock() {
        g.health = health;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use tablepro_core::{ConnectOptions, DatabaseDriver, DriverError, ExecResult, QueryResult, TableInfo};

    use super::*;

    struct CountingDriver {
        attempts: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl DatabaseDriver for CountingDriver {
        fn id(&self) -> &'static str {
            "counting"
        }

        fn display_name(&self) -> &'static str {
            "Counting"
        }

        fn default_port(&self) -> u16 {
            0
        }

        async fn connect(&self, _opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Err(DriverError::ConnectionRefused)
        }
    }

    struct IdleConn;

    #[async_trait]
    impl Connection for IdleConn {
        async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
            Ok(vec![])
        }

        async fn fetch_columns(&self, _: Option<&str>, _: &str) -> Result<Vec<tablepro_core::ColumnInfo>, DriverError> {
            Ok(vec![])
        }

        async fn fetch_rows(&self, _: Option<&str>, _: &str, _: u64, _: u64) -> Result<QueryResult, DriverError> {
            Err(DriverError::ConnectionRefused)
        }

        async fn query(&self, _: &str) -> Result<QueryResult, DriverError> {
            Err(DriverError::ConnectionRefused)
        }

        async fn execute(&self, _: &str) -> Result<ExecResult, DriverError> {
            Err(DriverError::ConnectionRefused)
        }

        async fn execute_params(&self, _: &str, _: &[tablepro_core::Value]) -> Result<ExecResult, DriverError> {
            Err(DriverError::ConnectionRefused)
        }

        async fn execute_in_transaction(
            &self,
            _: &[(String, Vec<tablepro_core::Value>)],
        ) -> Result<Vec<u64>, DriverError> {
            Err(DriverError::ConnectionRefused)
        }

        async fn ping(&self) -> Result<(), DriverError> {
            Ok(())
        }

        async fn close(self: Box<Self>) -> Result<(), DriverError> {
            Ok(())
        }
    }

    /// A reported fault must not wait for the ping interval, and must
    /// not be dismissed by a connection that still answers a ping.
    #[tokio::test(start_paused = true)]
    async fn a_reported_fault_starts_a_reconnect_before_the_next_ping() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(Mutex::new(EntryInner {
            connection: Arc::new(IdleConn) as Arc<dyn Connection>,
            tunnel: None,
            health: ConnectionHealth::Healthy,
        }));
        let params = ReconnectParams {
            driver: Arc::new(CountingDriver {
                attempts: attempts.clone(),
            }),
            opts: ConnectOptions::default(),
            ssh: None,
            environment: tablepro_transport::SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Learn),
        };
        let cancel = CancellationToken::new();
        let fault = Arc::new(Notify::new());
        let monitor = tokio::spawn(run(inner.clone(), params, cancel.clone(), fault.clone()));

        fault.notify_one();
        tokio::time::sleep(BACKOFF_INITIAL + Duration::from_secs(1)).await;

        assert!(
            attempts.load(Ordering::SeqCst) >= 1,
            "the fault must trigger a reconnect"
        );
        assert!(matches!(
            inner.lock().expect("entry lock").health,
            ConnectionHealth::Reconnecting { .. }
        ));
        cancel.cancel();
        let _ = monitor.await;
    }

    #[test]
    fn backoff_doubles_until_capped() {
        let mut d = BACKOFF_INITIAL;
        assert_eq!(d, Duration::from_secs(5));
        d = next_delay(d);
        assert_eq!(d, Duration::from_secs(10));
        d = next_delay(d);
        assert_eq!(d, Duration::from_secs(20));
        d = next_delay(d);
        assert_eq!(d, Duration::from_secs(40));
        d = next_delay(d);
        assert_eq!(d, BACKOFF_MAX);
        d = next_delay(d);
        assert_eq!(d, BACKOFF_MAX);
    }

    #[test]
    fn backoff_saturates_without_overflow() {
        assert_eq!(next_delay(Duration::MAX), BACKOFF_MAX);
    }
}
