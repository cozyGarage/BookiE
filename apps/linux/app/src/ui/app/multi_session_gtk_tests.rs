use super::*;
use bookie_core::Value;
use relm4::{Component, ComponentController};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn spin_until(mut condition: impl FnMut() -> bool) {
    let context = gtk4::glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() && Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(condition());
}

fn spin_for(duration: Duration) {
    let context = gtk4::glib::MainContext::default();
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn launch(directory: &std::path::Path) -> relm4::Controller<App> {
    use crate::services::{database_service, persistence_stores, preferences, workspace_state};
    App::builder()
        .launch(super::super::AppInit {
            registry: Arc::new(bookie_core::DriverRegistry::new()),
            persistence: persistence_stores::PersistenceStores {
                column_widths: None,
                filter_settings: None,
                highlight_settings: None,
                csv_import_mappings: None,
                named_filter_presets: None,
            },
            workspace: workspace_state::WorkspaceStore::new(None, directory.join("drafts")),
            history: None,
            database: Arc::new(database_service::DatabaseService::new_isolated()),
            preferences: preferences::PreferencesStore::new(),
            mcp_bridge: None,
        })
        .detach()
}

fn prepared_notes_connection(
    path: &std::path::Path,
    name: &str,
    runtime: &tokio::runtime::Runtime,
) -> crate::services::connection_service::PreparedConnection {
    use crate::services::database_service::{ConnectionMetadata, ReconnectParams};
    use bookie_core::{ConnectOptions, DatabaseDriver};

    let driver: Arc<dyn DatabaseDriver> = Arc::new(drivers_sqlite::SqliteDriver);
    let options = ConnectOptions {
        database: path.to_string_lossy().into_owned(),
        ..Default::default()
    };
    std::fs::File::create(path).unwrap();
    let connection = runtime.block_on(driver.connect(options.clone())).unwrap();
    runtime
        .block_on(connection.execute("CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT)"))
        .unwrap();
    crate::services::connection_service::PreparedConnection::new(
        Vec::new(),
        Vec::new(),
        "sqlite".into(),
        ConnectionMetadata {
            id: uuid::Uuid::new_v4(),
            name: name.into(),
            driver_id: "sqlite".into(),
            environment: bookie_core::Environment::Local,
            read_only: false,
            server_version: None,
            query_timeout_secs: None,
        },
        connection.into(),
        None,
        ReconnectParams {
            driver,
            opts: options,
            ssh: None,
            environment: bookie_transport::SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Learn),
        },
    )
}

fn open_connection(
    controller: &relm4::Controller<App>,
    prepared: crate::services::connection_service::PreparedConnection,
) -> uuid::Uuid {
    let id = prepared.id();
    controller.emit(AppMsg::OpenConnect);
    spin_until(|| controller.model().switch.transition == super::super::ConnectionTransition::Connecting);
    controller.emit(AppMsg::ConnectionPrepared(Box::new(prepared)));
    spin_until(|| controller.model().multi.sessions.is_working(id));
    id
}

fn titles(
    controller: &relm4::Controller<App>,
    runtime: &tokio::runtime::Runtime,
    connection: uuid::Uuid,
) -> bookie_core::QueryResult {
    let conn = controller.model().database.get(connection).unwrap();
    runtime.block_on(conn.query("SELECT title FROM notes")).unwrap()
}

fn is_empty(result: &bookie_core::QueryResult) -> bool {
    result.rows == Vec::<Vec<Value>>::new()
}

struct TwoSessions {
    controller: relm4::Controller<App>,
    runtime: tokio::runtime::Runtime,
    first: uuid::Uuid,
    second: uuid::Uuid,
    _directory: tempfile::TempDir,
}

fn two_sessions() -> TwoSessions {
    relm4::adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk4::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let controller = launch(directory.path());
    controller.widget().present();
    let _entered = runtime.enter();
    let first = prepared_notes_connection(&directory.path().join("first.sqlite"), "first", &runtime);
    let second = prepared_notes_connection(&directory.path().join("second.sqlite"), "second", &runtime);
    let first = open_connection(&controller, first);
    let second = open_connection(&controller, second);
    drop(_entered);
    TwoSessions {
        controller,
        runtime,
        first,
        second,
        _directory: directory,
    }
}

fn insert_from_tab(sessions: &TwoSessions, tab_id: uuid::Uuid, title: &str) {
    use crate::services::change_tracker::StatementSource;
    sessions.controller.emit(AppMsg::ExecuteBrowseTransaction {
        tab_id,
        save_id: uuid::Uuid::new_v4(),
        statements: vec![(
            "INSERT INTO notes (id, title) VALUES (?, ?)".into(),
            vec![Value::Int(1), Value::Text(title.into())],
        )],
        sources: vec![StatementSource::Insert { draft_id: 1 }],
    });
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_save_from_a_background_tab_reaches_its_own_connection_and_not_the_front_one() {
    let sessions = two_sessions();
    let _entered = sessions.runtime.enter();
    assert_eq!(sessions.controller.model().connection_id, Some(sessions.second));
    let tab_id = uuid::Uuid::new_v4();
    sessions.controller.emit(AppMsg::AttachWorkspaceTab {
        tab_id,
        connection_id: Some(sessions.first),
    });
    spin_until(|| {
        sessions
            .controller
            .model()
            .multi
            .sessions
            .connection_for_tab(tab_id)
            .is_some()
    });
    insert_from_tab(&sessions, tab_id, "for-first");
    spin_until(|| !is_empty(&titles(&sessions.controller, &sessions.runtime, sessions.first)));
    assert!(
        titles(&sessions.controller, &sessions.runtime, sessions.first).rows
            == vec![vec![Value::Text("for-first".into())]]
    );
    assert!(is_empty(&titles(
        &sessions.controller,
        &sessions.runtime,
        sessions.second
    )));
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_save_from_a_tab_without_a_recorded_connection_writes_nowhere() {
    let sessions = two_sessions();
    let _entered = sessions.runtime.enter();
    insert_from_tab(&sessions, uuid::Uuid::new_v4(), "stray");
    spin_for(Duration::from_millis(500));
    assert!(is_empty(&titles(
        &sessions.controller,
        &sessions.runtime,
        sessions.first
    )));
    assert!(is_empty(&titles(
        &sessions.controller,
        &sessions.runtime,
        sessions.second
    )));
    assert_eq!(sessions.controller.model().in_flight_saves.get(), 0);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_failed_connect_keeps_both_sessions_in_the_same_window() {
    let sessions = two_sessions();
    let saved = bookie_storage::SavedConnection {
        auth_source: None,
        id: uuid::Uuid::new_v4(),
        name: "broken".into(),
        driver_id: "missing-test-driver".into(),
        host: "localhost".into(),
        port: 5432,
        socket_dir: None,
        socket_path: None,
        database: "test".into(),
        username: "test".into(),
        use_tls: false,
        tls_mode: Some(bookie_core::TlsMode::Disabled),
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: false,
        auth_mode: bookie_core::AuthMode::Password,
        environment: bookie_core::Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        attached_databases: Vec::new(),
        tls_strict_encryption: false,
    };
    sessions.controller.emit(AppMsg::OpenSaved(saved));
    spin_until(|| sessions.controller.widget().visible_dialog().is_some());
    let model = sessions.controller.model();
    assert_eq!(model.multi.sessions.working_order(), &[sessions.first, sessions.second]);
    assert!(model.extra_windows.is_empty());
    assert!(model.connected);
}
