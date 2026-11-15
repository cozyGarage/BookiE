use super::*;
use bookie_core::TlsConfig;
use secrecy::SecretString;

fn options() -> ConnectOptions {
    ConnectOptions {
        host: "db.example".into(),
        port: 1521,
        database: "FREEPDB1".into(),
        username: "u".into(),
        password: SecretString::new("p".to_string().into()),
        tls: TlsConfig::disabled(),
        ..ConnectOptions::default()
    }
}

#[test]
fn plaintext_options_build_a_config_and_a_tls_request_is_refused_not_downgraded() {
    assert!(connect_config(&options()).is_ok());
    for mode in [
        TlsMode::Prefer,
        TlsMode::Require,
        TlsMode::VerifyCa,
        TlsMode::VerifyFull,
    ] {
        let mut secured = options();
        secured.tls.mode = mode;
        assert!(
            matches!(connect_config(&secured), Err(DriverError::Unsupported(_))),
            "{mode:?}"
        );
    }
}

#[test]
fn a_missing_service_name_is_refused() {
    let mut missing = options();
    missing.database = "  ".into();
    assert!(matches!(connect_config(&missing), Err(DriverError::Unsupported(_))));
}

#[test]
fn the_connect_deadline_is_applied_to_the_oracle_transport() {
    let mut options = options();
    options.connect_timeout_secs = Some(3);
    assert_eq!(
        connect_config(&options)
            .expect("valid Oracle options")
            .transport_connect_timeout(),
        Some(std::time::Duration::from_secs(3))
    );
}

#[test]
fn a_missing_or_zero_connect_deadline_uses_the_shared_transport_default() {
    for connect_timeout_secs in [None, Some(0)] {
        let mut options = options();
        options.connect_timeout_secs = connect_timeout_secs;
        assert_eq!(
            connect_config(&options)
                .expect("valid Oracle options")
                .transport_connect_timeout(),
            Some(std::time::Duration::from_secs(30))
        );
    }
}

#[test]
fn the_driver_declares_itself_experimental_on_the_oracle_port() {
    let capabilities = OracleDriver.capabilities();
    assert_eq!(capabilities.maturity, DriverMaturity::Experimental);
    assert_eq!(OracleDriver.default_port(), 1521);
    assert!(!OracleDriver.id().is_empty());
}

#[test]
fn browse_filters_stay_off_until_bound_queries_exist() {
    let capabilities = OracleDriver.capabilities();
    assert!(
        !capabilities.supports_browse_filters,
        "Oracle has no query_params path; advertising filters would open a UI that cannot run"
    );
}

#[tokio::test]
async fn a_cancelled_blocking_call_retires_the_session_before_it_can_be_reused() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio_util::sync::CancellationToken;

    let lifecycle = Arc::new(OracleLifecycle::default());
    let started = Arc::new(AtomicBool::new(false));
    let started_in_worker = Arc::clone(&started);
    let (started_sender, started_receiver) = tokio::sync::oneshot::channel();
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let token = CancellationToken::new();
    let control = OperationControl::new(token.clone(), None);
    let work = run_blocking(Arc::clone(&lifecycle), Some(&control), move || {
        started_in_worker.store(true, Ordering::Release);
        let _ = started_sender.send(());
        let _ = release_receiver.recv();
        Ok(())
    });
    tokio::pin!(work);

    let result = tokio::select! {
        _ = started_receiver => {
            token.cancel();
            work.await
        }
        result = &mut work => result,
    };

    assert!(started.load(Ordering::Acquire));
    assert!(matches!(
        result,
        Err(DriverError::OperationOutcomeUnknown { source }) if matches!(*source, DriverError::Cancelled)
    ));
    assert!(lifecycle.is_retired());
    let _ = release_sender.send(());
    let next_started = Arc::clone(&started);
    let next = run_blocking(
        Arc::clone(&lifecycle),
        Some(&OperationControl::with_timeout(std::time::Duration::from_secs(1))),
        move || {
            next_started.store(false, Ordering::Release);
            Ok(())
        },
    )
    .await;

    assert!(matches!(next, Err(DriverError::Disconnected)));
    assert!(started.load(Ordering::Acquire));
}

#[tokio::test]
async fn a_timed_out_blocking_call_retires_the_session() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let lifecycle = Arc::new(OracleLifecycle::default());
    let started = Arc::new(AtomicBool::new(false));
    let started_in_worker = Arc::clone(&started);
    let control = OperationControl::with_timeout(std::time::Duration::from_millis(100));
    let work = run_blocking(Arc::clone(&lifecycle), Some(&control), move || {
        started_in_worker.store(true, Ordering::Release);
        std::thread::sleep(std::time::Duration::from_millis(250));
        Ok(())
    });

    let result = work.await;

    assert!(started.load(Ordering::Acquire));
    assert!(matches!(
        result,
        Err(DriverError::OperationOutcomeUnknown { source }) if matches!(*source, DriverError::TimedOut)
    ));
    assert!(lifecycle.is_retired());
}

#[tokio::test]
async fn a_pre_dispatch_cancellation_does_not_retire_the_session() {
    use tokio_util::sync::CancellationToken;

    let lifecycle = Arc::new(OracleLifecycle::default());
    let token = CancellationToken::new();
    token.cancel();
    let result = run_blocking(
        Arc::clone(&lifecycle),
        Some(&OperationControl::new(token, None)),
        || Ok(()),
    )
    .await;

    assert!(matches!(result, Err(DriverError::Cancelled)));
    assert!(!lifecycle.is_retired());
}

#[tokio::test]
async fn a_retired_session_wakes_a_late_attached_connection_monitor() {
    let lifecycle = OracleLifecycle::default();
    let notify = Arc::new(tokio::sync::Notify::new());

    lifecycle.retire();
    lifecycle.attach_fault_notify(Arc::clone(&notify));

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), notify.notified())
            .await
            .is_ok()
    );
}
