use std::sync::Arc;

use relm4::RelmApp;

use tablepro_core::DriverRegistry;

pub mod config;
mod i18n;
pub mod logging;
mod services;
mod ui;

pub fn run() {
    tablepro_transport::install_crypto_provider();
    i18n::init();

    if let Err(error) = logging::init(config::profile()) {
        glib::g_critical!("bookie", "{error}");
        return;
    }
    if let Err(error) = gtk4::gio::resources_register_include!("tablepro.gresource") {
        tracing::error!(%error, "could not register application resources");
        return;
    }

    // Single-instance gate: belt-and-suspenders flock on top of
    // gtk::Application's DBus-based uniqueness, since the latter
    // silently lets two processes through when DBus is unavailable.
    // A second instance corrupts workspace_state.json via concurrent
    // read-modify-write. Hold the lock through the entire `main`.
    let _instance_lock = match services::single_instance::acquire() {
        Ok(lock) => Some(lock),
        Err(services::single_instance::LockError::AlreadyRunning) => {
            tracing::info!("another BookiE instance is running; exiting");
            return;
        }
        Err(e) => {
            // No XDG runtime / cache / HOME — proceed without the
            // lock. gtk::Application's uniqueness still applies.
            tracing::warn!(error = %e, "single-instance lock unavailable; relying on DBus uniqueness");
            None
        }
    };

    let preferences = services::preferences::PreferencesStore::new();
    let prefs = preferences.load();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    {
        Ok(runtime) => Some(runtime),
        Err(e) => {
            tracing::warn!(error = %e, "history runtime unavailable; feature disabled");
            None
        }
    };
    let history = runtime.as_ref().and_then(|runtime| {
        runtime.block_on(async {
            match tablepro_storage::query_history::HistoryStore::open_default().await {
                Ok(store) => {
                    if let Err(e) = store.prune_older_than(prefs.history_retention_days).await {
                        tracing::warn!(error = %e, "history prune failed");
                    }
                    Some(store)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "history init failed; feature disabled");
                    None
                }
            }
        })
    });

    let registry = Arc::new(build_registry());
    let persistence = services::persistence_stores::PersistenceStores::open();
    let workspace = services::workspace_state::WorkspaceStore::new();
    let database = Arc::new(services::database_service::DatabaseService::new());
    tracing::info!(drivers = registry.len(), "starting tablepro-app");

    let approval_router = services::approval_router::ApprovalRouter::new(
        Arc::new(services::gtk_approval::GtkApprovalSink),
        Arc::new(services::gtk_approval::GtkApprovalSink),
    );
    database.set_approval_sink(Arc::new(approval_router));

    let mcp_bridge = services::mcp_service::start_background(database.clone());

    let app = RelmApp::new(config::APP_ID);
    app.run::<ui::App>(ui::AppInit {
        registry,
        persistence: persistence.clone(),
        workspace,
        history,
        database,
        preferences,
        mcp_bridge,
    });

    persistence.flush();

    // Dropping a tokio runtime cancels its in-flight tasks at once;
    // shutdown_timeout lets the worker threads finish first.
    if let Some(runtime) = runtime {
        runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    }
}

fn build_registry() -> DriverRegistry {
    let mut r = DriverRegistry::new();
    r.register(Arc::new(drivers_clickhouse::ClickhouseDriver));
    #[cfg(feature = "duckdb")]
    r.register(Arc::new(drivers_duckdb::DuckdbDriver));
    r.register(Arc::new(drivers_mongodb::MongodbDriver));
    r.register(Arc::new(drivers_mssql::MssqlDriver));
    r.register(Arc::new(drivers_mysql::MysqlDriver));
    r.register(Arc::new(drivers_postgres::PgDriver));
    r.register(Arc::new(drivers_redis::RedisDriver));
    r.register(Arc::new(drivers_sqlite::SqliteDriver));
    r
}
