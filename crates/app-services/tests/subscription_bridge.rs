#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bookie_app_services::command_bridge::{ChannelEventSink, Command, DispatchContext, Epoch, Event, dispatch};
use bookie_app_services::database_service::DatabaseService;
use bookie_app_services::driver_error_presentation::DriverFailurePresentation;
use bookie_app_services::read_session_store::{
    ReadSessionStore, SubscriptionMessage, SubscriptionRequest, SubscriptionSample, SubscriptionSampler,
};
use bookie_core::{AuthMode, ConnectOptions, DriverError, DriverRegistry, Environment, OperationControl};
use bookie_storage::SavedConnection;
use uuid::Uuid;

struct FixedSampler;

impl SubscriptionSampler for FixedSampler {
    fn validate(&self, _: &SubscriptionRequest) -> Result<(), DriverError> {
        Ok(())
    }

    fn sample<'a>(
        &'a self,
        _: ConnectOptions,
        request: SubscriptionRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SubscriptionSample, DriverError>> + Send + 'a>> {
        Box::pin(async move {
            let channel = request.patterns.first().cloned().unwrap_or_default();
            Ok(SubscriptionSample {
                messages: vec![SubscriptionMessage {
                    channel,
                    pattern: Some("events.*".into()),
                    payload: "created".into(),
                    payload_bytes: 7,
                }],
                reached_limit: false,
            })
        })
    }
}

#[tokio::test]
async fn a_subscription_read_returns_plain_sample_data_with_its_epoch() {
    let directory = tempfile::tempdir().unwrap();
    let previous_config = std::env::var_os("XDG_CONFIG_HOME");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", directory.path()) };
    let database_path = directory.path().join("session.db");
    std::fs::File::create(&database_path).unwrap();
    let connection = SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "fixture".into(),
        driver_id: "sqlite".into(),
        host: String::new(),
        port: 0,
        socket_dir: None,
        socket_path: None,
        database: database_path.to_string_lossy().into_owned(),
        username: String::new(),
        use_tls: false,
        tls_mode: None,
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        tls_strict_encryption: false,
        read_only: true,
        auth_mode: AuthMode::Password,
        environment: Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        attached_databases: Vec::new(),
    };
    bookie_storage::save_connections(std::slice::from_ref(&connection))
        .await
        .unwrap();
    let mut registry = DriverRegistry::new();
    registry.register(Arc::new(drivers_sqlite::SqliteDriver));
    let sessions = ReadSessionStore::new(Arc::new(registry), Arc::new(DatabaseService::new_isolated()))
        .with_subscription_sampler(Arc::new(FixedSampler));
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    let opened = sessions.open_saved(connection.id, &control).await.unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let sink = ChannelEventSink::new(sender);
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let cancelled = OperationControl::new(cancellation, None);

    dispatch(
        DispatchContext::Sessions(&sessions),
        Command::SampleSubscription {
            epoch: Epoch(89),
            session_id: opened.session_id,
            request: SubscriptionRequest {
                patterns: vec!["events.*".into()],
                max_messages: 10,
                window: Duration::from_secs(2),
            },
        },
        &cancelled,
        &sink,
    )
    .await
    .unwrap();
    assert!(matches!(
        receiver.try_recv().unwrap(),
        Event::SubscriptionSampled {
            epoch: Epoch(89),
            outcome: Err(DriverFailurePresentation::Cancelled)
        }
    ));

    dispatch(
        DispatchContext::Sessions(&sessions),
        Command::SampleSubscription {
            epoch: Epoch(90),
            session_id: opened.session_id,
            request: SubscriptionRequest {
                patterns: vec!["events.*".into()],
                max_messages: 10,
                window: Duration::from_secs(2),
            },
        },
        &control,
        &sink,
    )
    .await
    .unwrap();

    if let Some(config) = previous_config {
        unsafe { std::env::set_var("XDG_CONFIG_HOME", config) };
    } else {
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
    }
    let Event::SubscriptionSampled { epoch, outcome } = receiver.try_recv().unwrap() else {
        panic!("subscription command returned a different event");
    };
    assert_eq!(epoch, Epoch(90));
    assert_eq!(
        outcome.unwrap(),
        SubscriptionSample {
            messages: vec![SubscriptionMessage {
                channel: "events.*".into(),
                pattern: Some("events.*".into()),
                payload: "created".into(),
                payload_bytes: 7,
            }],
            reached_limit: false,
        }
    );
}
