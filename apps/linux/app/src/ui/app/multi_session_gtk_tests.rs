use super::super::ConnectionMessage;
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
    id: uuid::Uuid,
    host: &str,
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
    let saved = bookie_storage::SavedConnection {
        auth_source: None,
        id,
        name: name.into(),
        driver_id: "sqlite".into(),
        host: host.into(),
        port: 0,
        socket_dir: None,
        socket_path: None,
        database: path.to_string_lossy().into_owned(),
        username: String::new(),
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
    crate::services::connection_service::PreparedConnection::new(
        Vec::new(),
        Vec::new(),
        "sqlite".into(),
        ConnectionMetadata {
            id,
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
    .with_dial_identity(bookie_app_services::dial_identity::DialIdentity::new(
        &saved,
        &secrecy::SecretString::new("test-password".into()),
    ))
}

fn open_connection(
    controller: &relm4::Controller<App>,
    prepared: crate::services::connection_service::PreparedConnection,
) -> uuid::Uuid {
    let id = prepared.id();
    controller.emit(AppMsg::Connection(ConnectionMessage::Open));
    spin_until(|| controller.model().switch.transition == super::super::ConnectionTransition::Connecting);
    controller.emit(AppMsg::Connection(ConnectionMessage::Prepared(Box::new(prepared))));
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
    let first_id = uuid::Uuid::new_v4();
    let second_id = uuid::Uuid::new_v4();
    let first = prepared_notes_connection(
        &directory.path().join("first.sqlite"),
        "first",
        first_id,
        "first.host",
        &runtime,
    );
    let second = prepared_notes_connection(
        &directory.path().join("second.sqlite"),
        "second",
        second_id,
        "second.host",
        &runtime,
    );
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

#[test]
#[ignore = "requires an isolated GTK display"]
fn cancelled_sidebar_catalog_response_does_not_restore_entries() {
    let sessions = two_sessions();
    let _entered = sessions.runtime.enter();
    let origin = crate::services::catalog::CatalogOrigin::capture(
        sessions.controller.model().connection_id,
        &sessions.controller.model().database,
    )
    .unwrap();
    let request = tokio_util::sync::CancellationToken::new();
    request.cancel();
    sessions.controller.emit(AppMsg::SidebarCatalogLoaded {
        origin,
        kind: bookie_core::CatalogObjectKind::Sequence,
        request,
        result: Ok(vec![bookie_core::CatalogObject {
            kind: bookie_core::CatalogObjectKind::Sequence,
            schema: Some("public".into()),
            name: "stale".into(),
            detail: None,
        }]),
    });
    spin_for(Duration::from_millis(100));
    assert!(sessions.controller.model().sidebar_catalog.is_empty());
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

fn inject_streamed_page(
    sessions: &TwoSessions,
    connection_id: uuid::Uuid,
) -> (uuid::Uuid, bookie_results::stream_page_budget::StreamPageBudget) {
    use crate::ui::editor::SqlEditorInput;

    let controller = &sessions.controller;
    controller.emit(AppMsg::FocusWorkingSession(connection_id));
    spin_until(|| controller.model().connection_id == Some(connection_id));
    let previous_tabs = controller.model().workspace_tabs.borrow().keys().count();
    controller.emit(AppMsg::NewEditorTab);
    spin_until(|| controller.model().workspace_tabs.borrow().keys().count() == previous_tabs + 1);
    let tab_id = controller.model().selected_workspace_tab_id().unwrap();
    let editor = match controller.model().workspace_tabs.borrow().get(&tab_id).unwrap() {
        super::super::WorkspaceTab::Editor(slot) => slot.controller.sender().clone(),
        _ => panic!("expected editor"),
    };
    let spool = controller
        .model()
        .database
        .stream_result_spool_manager()
        .create()
        .unwrap();
    let page_budget = spool.page_budget();
    let page = Arc::new(bookie_core::QueryResult {
        columns: vec![bookie_core::ColumnInfo {
            name: "payload".into(),
            data_type: "TEXT".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }],
        rows: (0..1024)
            .map(|_| vec![Value::Text("x".repeat(1024))])
            .collect::<Vec<_>>()
            .into(),
        truncated: false,
    });
    sessions.runtime.block_on(spool.append_page(0, page.clone())).unwrap();
    editor
        .send(SqlEditorInput::InjectStreamForTest { page, spool })
        .unwrap();
    spin_until(|| page_budget.used_bytes() > 0);
    (tab_id, page_budget)
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
fn reconnecting_changed_saved_settings_replaces_only_the_target_session() {
    let sessions = two_sessions();
    let _entered = sessions.runtime.enter();
    let original_target = sessions
        .controller
        .model()
        .database
        .identity(sessions.first)
        .expect("first connection identity");
    let other_identity = sessions
        .controller
        .model()
        .database
        .identity(sessions.second)
        .expect("second connection identity");
    let replacement = prepared_notes_connection(
        &sessions._directory.path().join("first-reconnected.sqlite"),
        "first updated",
        sessions.first,
        "first.changed.host",
        &sessions.runtime,
    );

    sessions.controller.emit(AppMsg::Connection(ConnectionMessage::Open));
    spin_until(|| sessions.controller.model().switch.transition == super::super::ConnectionTransition::Connecting);
    sessions
        .controller
        .emit(AppMsg::Connection(ConnectionMessage::Prepared(Box::new(replacement))));
    spin_until(|| {
        sessions.controller.model().switch.transition == super::super::ConnectionTransition::AwaitingReconnect
    });
    sessions.controller.emit(AppMsg::ReconnectSavedConnection(true));

    spin_until(|| {
        sessions
            .controller
            .model()
            .database
            .metadata(sessions.first)
            .is_some_and(|metadata| metadata.name == "first updated")
    });
    let model = sessions.controller.model();
    assert!(model.multi.sessions.is_working(sessions.first));
    assert!(model.multi.sessions.is_working(sessions.second));
    assert!(model.database.is_active(sessions.second));
    assert_ne!(model.database.identity(sessions.first).as_ref(), Some(&original_target));
    assert_eq!(model.database.identity(sessions.second).as_ref(), Some(&other_identity));
    drop(model);
    assert!(is_empty(&titles(
        &sessions.controller,
        &sessions.runtime,
        sessions.second
    )));
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn reconnect_releases_only_the_target_sessions_streamed_pages() {
    let sessions = two_sessions();
    let _entered = sessions.runtime.enter();
    let (target_tab, target_budget) = inject_streamed_page(&sessions, sessions.first);
    assert!(target_budget.used_bytes() > 0);

    let replacement = prepared_notes_connection(
        &sessions._directory.path().join("first-reconnected.sqlite"),
        "first updated",
        sessions.first,
        "first.changed.host",
        &sessions.runtime,
    );
    sessions.controller.emit(AppMsg::Connection(ConnectionMessage::Open));
    spin_until(|| sessions.controller.model().switch.transition == super::super::ConnectionTransition::Connecting);
    sessions
        .controller
        .emit(AppMsg::Connection(ConnectionMessage::Prepared(Box::new(replacement))));
    spin_until(|| {
        sessions.controller.model().switch.transition == super::super::ConnectionTransition::AwaitingReconnect
    });
    sessions.controller.emit(AppMsg::ReconnectSavedConnection(true));
    spin_until(|| {
        !sessions
            .controller
            .model()
            .workspace_tabs
            .borrow()
            .contains_key(&target_tab)
    });

    spin_until(|| target_budget.used_bytes() == 0);
    assert!(sessions.controller.model().multi.sessions.is_working(sessions.first));
    assert!(sessions.controller.model().multi.sessions.is_working(sessions.second));

    let (other_tab, other_budget) = inject_streamed_page(&sessions, sessions.second);
    assert!(other_budget.used_bytes() > 0);
    assert!(
        sessions
            .controller
            .model()
            .workspace_tabs
            .borrow()
            .contains_key(&other_tab)
    );
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

struct WindowPair {
    controller: relm4::Controller<App>,
    primary: uuid::Uuid,
    secondary: uuid::Uuid,
    _directory: tempfile::TempDir,
}

fn primary_and_secondary_window() -> WindowPair {
    relm4::adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    let application = relm4::main_application();
    application.register(None::<&gtk4::gio::Cancellable>).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let controller = launch(directory.path());
    controller.widget().set_application(Some(&application));
    controller.widget().present();
    controller.emit(AppMsg::NewWindow);
    spin_until(|| controller.model().extra_windows.len() == 1);
    assert_eq!(application.windows().len(), 2);
    let key = controller.model().extra_windows[0].0;
    let primary = uuid::Uuid::new_v4();
    let secondary = uuid::Uuid::new_v4();
    crate::services::open_sessions::record_window(controller.widget().as_ptr() as usize, vec![primary]);
    crate::services::open_sessions::record_window(key, vec![secondary]);
    WindowPair {
        controller,
        primary,
        secondary,
        _directory: directory,
    }
}

fn recorded_groups() -> Vec<Vec<uuid::Uuid>> {
    crate::services::open_sessions::record_window(usize::MAX / 2, Vec::new())
        .record_windows()
        .to_vec()
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn closing_a_secondary_window_forgets_only_its_own_group() {
    let pair = primary_and_secondary_window();
    let window = pair.controller.model().extra_windows[0].1.widget().clone();
    spin_until(|| window.is_mapped());
    let expected = vec![vec![pair.primary]];
    window.close();
    spin_until(|| {
        if recorded_groups() == expected {
            return true;
        }
        window.emit_by_name::<bool>("close-request", &[]);
        false
    });
    window.destroy();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn destroying_windows_without_a_close_request_keeps_every_group() {
    let pair = primary_and_secondary_window();
    let window = pair.controller.model().extra_windows[0].1.widget().clone();
    window.destroy();
    spin_for(Duration::from_millis(300));
    assert_eq!(recorded_groups(), vec![vec![pair.primary], vec![pair.secondary]]);
}
