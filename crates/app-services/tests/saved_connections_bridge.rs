#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_app_services::command_bridge::{ChannelEventSink, Command, DispatchContext, Epoch, Event, dispatch};
use bookie_core::{AuthMode, Environment, OperationControl};
use bookie_storage::SavedConnection;
use uuid::Uuid;

#[tokio::test]
async fn listing_saved_connections_emits_only_the_current_summary_fields() {
    let directory = tempfile::tempdir().unwrap();
    let previous_config = std::env::var_os("XDG_CONFIG_HOME");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", directory.path()) };
    let connection = SavedConnection {
        id: Uuid::new_v4(),
        name: "fixture".into(),
        driver_id: "sqlite".into(),
        host: String::new(),
        port: 0,
        socket_dir: None,
        socket_path: None,
        database: ":memory:".into(),
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
        auth_source: None,
        attached_databases: Vec::new(),
    };
    bookie_storage::save_connections(std::slice::from_ref(&connection))
        .await
        .unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let sink = ChannelEventSink::new(sender);
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(5));

    dispatch(
        DispatchContext::SavedConnections,
        Command::ListSavedConnections { epoch: Epoch(48) },
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
    let Event::SavedConnectionsListed { epoch, outcome } = receiver.try_recv().unwrap() else {
        panic!("saved-connection command returned a different event");
    };
    assert_eq!(epoch, Epoch(48));
    assert_eq!(
        outcome.unwrap(),
        vec![bookie_app_services::read_session::ConnectionSummary {
            id: connection.id,
            name: connection.name,
            driver_id: connection.driver_id,
            read_only: connection.read_only,
        }]
    );
}
