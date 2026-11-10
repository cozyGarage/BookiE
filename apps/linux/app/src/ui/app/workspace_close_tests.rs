use super::*;
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

fn source_view(widget: &gtk4::Widget) -> Option<sourceview5::View> {
    if let Some(view) = widget.downcast_ref::<sourceview5::View>() {
        return Some(view.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(view) = source_view(&current) {
            return Some(view);
        }
    }
    None
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn forced_dirty_tab_confirmation_dismissal_preserves_the_file_and_tab() {
    adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk4::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("query.sql");
    std::fs::write(&path, "SELECT 1;").unwrap();
    let controller = launch(directory.path());
    controller.widget().present();
    controller.emit(AppMsg::SqlFileOpened(
        bookie_core::text_file::read_text_file(&path, 1024).unwrap(),
    ));
    spin_until(|| controller.model().workspace_tabs.borrow().values().count() == 1);
    let id = controller.model().selected_workspace_tab_id().unwrap();
    let editor = match controller.model().workspace_tabs.borrow().get(&id).unwrap() {
        WorkspaceTab::Editor(slot) => source_view(slot.controller.widget().upcast_ref()).unwrap(),
        _ => panic!("expected editor"),
    };
    editor.buffer().set_text("SELECT 2;");
    spin_until(|| controller.model().pending_workspace_tab_changes(id).file);
    controller.emit(AppMsg::CloseActiveWorkspaceTab);
    spin_until(|| controller.widget().visible_dialog().is_some());
    controller.widget().visible_dialog().unwrap().force_close();
    spin_until(|| controller.widget().visible_dialog().is_none());
    assert!(controller.model().workspace_tabs.borrow().contains_key(&id));
    assert!(controller.model().pending_workspace_tab_changes(id).file);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "SELECT 1;");
    assert_eq!(
        editor
            .buffer()
            .text(&editor.buffer().start_iter(), &editor.buffer().end_iter(), true),
        "SELECT 2;"
    );
    respond(&controller, "Cancel");
    assert!(controller.model().workspace_tabs.borrow().contains_key(&id));
    assert!(controller.model().pending_workspace_tab_changes(id).file);
    respond(&controller, "Discard");
    spin_until(|| !controller.model().workspace_tabs.borrow().contains_key(&id));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "SELECT 1;");
}

fn respond(controller: &relm4::Controller<App>, label: &str) {
    controller.emit(AppMsg::CloseActiveWorkspaceTab);
    spin_until(|| controller.widget().visible_dialog().is_some());
    let dialog = controller.widget().visible_dialog().unwrap();
    response_button(dialog.upcast_ref(), label).unwrap().emit_clicked();
    spin_until(|| controller.widget().visible_dialog().is_none());
}

fn response_button(widget: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    if let Some(button) = widget.downcast_ref::<gtk4::Button>()
        && button.label().as_deref() == Some(label)
    {
        return Some(button.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(button) = response_button(&current, label) {
            return Some(button);
        }
    }
    None
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn force_dismissing_connection_deletion_does_not_abort() {
    adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk4::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let saved = bookie_storage::SavedConnection {
        id: Uuid::new_v4(),
        name: "synthetic".into(),
        driver_id: "postgres".into(),
        host: "localhost".into(),
        port: 5432,
        database: "test".into(),
        username: "test".into(),
        auth_source: None,
        socket_dir: None,
        socket_path: None,
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
    runtime
        .block_on(bookie_storage::save_connections(std::slice::from_ref(&saved)))
        .unwrap();
    let controller = launch(directory.path());
    controller.widget().present();
    controller.emit(AppMsg::DeleteConnection(saved.id));
    spin_until(|| controller.widget().visible_dialog().is_some());
    controller.widget().visible_dialog().unwrap().force_close();
    spin_until(|| controller.widget().visible_dialog().is_none());
    let retained = runtime.block_on(bookie_storage::load_connections()).unwrap();
    assert!(
        retained
            .iter()
            .any(|entry| entry.id == saved.id && entry.name == saved.name)
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn browse_save_dispatch_executes_and_audits_a_guarded_transaction() {
    use crate::services::change_tracker::StatementSource;
    use bookie_core::Value;

    adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk4::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let controller = launch(directory.path());
    controller.widget().present();
    controller.emit(AppMsg::OpenConnect);
    spin_until(|| controller.model().switch.transition == super::super::ConnectionTransition::Connecting);
    let prepared = prepared_sqlite_connection(false, None);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    controller.emit(AppMsg::ConnectionPrepared(Box::new(prepared)));
    spin_until(|| controller.model().window_connection().is_some());
    let connection = controller.model().window_connection().unwrap();
    runtime
        .block_on(connection.execute("CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT)"))
        .unwrap();
    let tab_id = uuid::Uuid::new_v4();
    let save_id = uuid::Uuid::new_v4();
    controller.emit(AppMsg::ExecuteBrowseTransaction {
        tab_id,
        save_id,
        statements: vec![(
            "INSERT INTO notes (id, title) VALUES (?, ?)".into(),
            vec![Value::Int(1), Value::Text("saved".into())],
        )],
        sources: vec![StatementSource::Insert { draft_id: 1 }],
    });
    spin_until(|| {
        runtime
            .block_on(connection.query("SELECT title FROM notes WHERE id = 1"))
            .is_ok_and(|result| result.rows == vec![vec![Value::Text("saved".into())]])
    });
    spin_until(|| controller.model().in_flight_saves.get() == 0);
    let journal = controller.model().database.audit_journal_path_for_test();
    let contents = std::fs::read_to_string(journal).unwrap();
    let events: Vec<serde_json::Value> = contents
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(events.iter().any(|record| {
        let event = &record["event"];
        event["operation_class"] == "mutation" && event["phase"] == "outcome" && event["terminal_status"] == "succeeded"
    }));
}

fn prepared_sqlite_connection(
    read_only: bool,
    path: Option<&std::path::Path>,
) -> crate::services::connection_service::PreparedConnection {
    use crate::services::database_service::{ConnectionMetadata, ReconnectParams};
    use bookie_core::{ConnectOptions, DatabaseDriver, Environment};

    let driver: Arc<dyn DatabaseDriver> = Arc::new(drivers_sqlite::SqliteDriver);
    let options = ConnectOptions {
        database: path.map_or_else(|| ":memory:".into(), |path| path.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let connection = runtime.block_on(driver.connect(options.clone())).unwrap();
    if read_only {
        runtime
            .block_on(connection.execute("CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT)"))
            .unwrap();
    }
    let id = uuid::Uuid::new_v4();
    crate::services::connection_service::PreparedConnection::new(
        Vec::new(),
        Vec::new(),
        "sqlite".into(),
        ConnectionMetadata {
            id,
            name: "switch target".into(),
            driver_id: "sqlite".into(),
            environment: Environment::Local,
            read_only,
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

#[test]
#[ignore = "requires an isolated GTK display"]
fn browse_save_dispatch_on_read_only_connection_is_denied_and_audited() {
    use crate::services::change_tracker::StatementSource;
    use bookie_core::Value;

    adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk4::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let controller = launch(directory.path());
    controller.widget().present();
    controller.emit(AppMsg::OpenConnect);
    spin_until(|| controller.model().switch.transition == super::super::ConnectionTransition::Connecting);
    std::fs::File::create(directory.path().join("notes.sqlite")).unwrap();
    let prepared = prepared_sqlite_connection(true, Some(&directory.path().join("notes.sqlite")));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    controller.emit(AppMsg::ConnectionPrepared(Box::new(prepared)));
    spin_until(|| controller.model().window_connection().is_some());
    let connection = controller.model().window_connection().unwrap();
    let tab_id = uuid::Uuid::new_v4();
    let save_id = uuid::Uuid::new_v4();
    controller.emit(AppMsg::ExecuteBrowseTransaction {
        tab_id,
        save_id,
        statements: vec![(
            "INSERT INTO notes (id, title) VALUES (?, ?)".into(),
            vec![Value::Int(1), Value::Text("blocked".into())],
        )],
        sources: vec![StatementSource::Insert { draft_id: 1 }],
    });
    let journal = controller.model().database.audit_journal_path_for_test();
    spin_until(|| {
        std::fs::read_to_string(&journal).is_ok_and(|contents| {
            contents.lines().any(|line| {
                serde_json::from_str::<serde_json::Value>(line).is_ok_and(|record| {
                    let event = &record["event"];
                    event["operation_class"] == "mutation"
                        && event["phase"] == "outcome"
                        && event["terminal_status"] == "denied"
                })
            })
        })
    });
    spin_until(|| controller.model().in_flight_saves.get() == 0);
    let rows = runtime
        .block_on(connection.query("SELECT title FROM notes WHERE id = 1"))
        .unwrap()
        .rows;
    assert!(rows.is_empty());
    let contents = std::fs::read_to_string(journal).unwrap();
    let events: Vec<serde_json::Value> = contents
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(events.iter().any(|record| {
        let event = &record["event"];
        event["operation_class"] == "mutation" && event["phase"] == "outcome" && event["terminal_status"] == "denied"
    }));
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn browse_grid_edit_save_reaches_guarded_connection_and_audit() {
    use crate::ui::browse_tab::BrowseTabInput;
    use bookie_core::Value;

    adw::init().unwrap();
    gtk4::gio::resources_register_include!("bookie.gresource").unwrap();
    relm4::main_application()
        .register(None::<&gtk4::gio::Cancellable>)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let controller = launch(directory.path());
    controller.widget().present();
    controller.emit(AppMsg::OpenConnect);
    spin_until(|| controller.model().switch.transition == super::super::ConnectionTransition::Connecting);
    let prepared = prepared_sqlite_connection(false, None);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    controller.emit(AppMsg::ConnectionPrepared(Box::new(prepared)));
    spin_until(|| controller.model().window_connection().is_some());
    let connection = controller.model().window_connection().unwrap();
    runtime
        .block_on(connection.execute("CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT)"))
        .unwrap();
    runtime
        .block_on(connection.execute("INSERT INTO notes VALUES (1, 'before')"))
        .unwrap();
    controller.emit(AppMsg::SelectTable {
        schema: None,
        name: "notes".into(),
        open_mode: super::super::OpenMode::NewTab,
    });
    spin_until(|| {
        controller.model().workspace_tabs.borrow().values().any(|tab| {
            tab.browse_controller()
                .is_some_and(|browse| browse.model().row_cells_at(0).is_some())
        })
    });
    let id = controller
        .model()
        .workspace_tabs
        .borrow()
        .iter()
        .find_map(|(id, tab)| tab.browse_controller().map(|_| *id))
        .unwrap();
    {
        let model = controller.model();
        let tabs = model.workspace_tabs.borrow();
        let browse = tabs.get(&id).unwrap().browse_controller().unwrap();
        browse.emit(BrowseTabInput::GridCellEdited {
            row_position: 0,
            col_index: 1,
            new_value: "changed".into(),
            value: None,
            row_key: vec![Value::Int(1)],
        });
    }
    spin_until(|| {
        controller
            .model()
            .change_trackers
            .with_tab_ref(id, |tracker| tracker.pending_count())
            .unwrap_or(0)
            == 1
    });
    controller.emit(AppMsg::SaveActiveBrowseTabById(id));
    spin_until(|| {
        runtime
            .block_on(connection.query("SELECT title FROM notes WHERE id = 1"))
            .is_ok_and(|result| result.rows == vec![vec![Value::Text("changed".into())]])
    });
    spin_until(|| {
        controller
            .model()
            .change_trackers
            .with_tab_ref(id, |tracker| tracker.pending_count())
            .unwrap_or(1)
            == 0
    });
    let contents = std::fs::read_to_string(controller.model().database.audit_journal_path_for_test()).unwrap();
    assert!(contents.lines().any(
        |line| serde_json::from_str::<serde_json::Value>(line).is_ok_and(|record| {
            let event = &record["event"];
            event["operation_class"] == "mutation"
                && event["phase"] == "outcome"
                && event["terminal_status"] == "succeeded"
        })
    ));
}
