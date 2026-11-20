use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use bookie_core::{ConnectOptions, DriverError, DriverRegistry, OperationControl, TableInfo};
use uuid::Uuid;

use crate::database_service::DatabaseService;
use crate::driver_error_presentation::DriverFailurePresentation;
use crate::driver_failure_message::open_failure_message;
use crate::read_session::ReadSession;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionRequest {
    pub patterns: Vec<String>,
    pub max_messages: usize,
    pub window: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionMessage {
    pub channel: String,
    pub pattern: Option<String>,
    pub payload: String,
    pub payload_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionSample {
    pub messages: Vec<SubscriptionMessage>,
    pub reached_limit: bool,
}

pub trait SubscriptionSampler: Send + Sync {
    fn validate(&self, driver_id: &str, request: &SubscriptionRequest) -> Result<(), DriverError>;

    fn sample(
        &self,
        driver_id: &str,
        options: ConnectOptions,
        request: SubscriptionRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SubscriptionSample, DriverError>> + Send + '_>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(Uuid);

impl SessionId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedSession {
    pub session_id: SessionId,
    pub label: String,
    pub driver_id: String,
    pub tables: Vec<bookie_core::TableInfo>,
    pub supports_snapshot_cursor: bool,
}

pub struct ReadSessionStore {
    registry: Arc<DriverRegistry>,
    database: Arc<DatabaseService>,
    subscription_sampler: Option<Arc<dyn SubscriptionSampler>>,
    sessions: RwLock<HashMap<SessionId, Arc<ReadSession>>>,
}

impl ReadSessionStore {
    pub fn new(registry: Arc<DriverRegistry>, database: Arc<DatabaseService>) -> Self {
        Self {
            registry,
            database,
            subscription_sampler: None,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_subscription_sampler(mut self, sampler: Arc<dyn SubscriptionSampler>) -> Self {
        self.subscription_sampler = Some(sampler);
        self
    }

    pub async fn open_saved(&self, id: Uuid, control: &OperationControl) -> Result<OpenedSession, String> {
        let saved = bookie_storage::load_connections()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|saved| saved.id == id)
            .ok_or_else(|| format!("connection {id} not found"))?;
        let label = saved.name.clone();
        self.open(saved, label, control).await
    }

    pub async fn saved_connection_stamp(id: Uuid) -> Result<String, String> {
        let saved = bookie_storage::load_connections()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|saved| saved.id == id)
            .ok_or_else(|| format!("connection {id} not found"))?;
        serde_json::to_string(&saved).map_err(|error| error.to_string())
    }

    pub async fn open_saved_writable(&self, id: Uuid, control: &OperationControl) -> Result<OpenedSession, String> {
        let saved = bookie_storage::load_connections()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|saved| saved.id == id)
            .ok_or_else(|| format!("connection {id} not found"))?;
        let label = saved.name.clone();
        self.open_writable(saved, label, control).await
    }

    pub async fn apply_changes(
        &self,
        id: SessionId,
        statements: &[(String, Vec<bookie_core::Value>)],
        expect_one: &[usize],
        control: &OperationControl,
    ) -> Result<Vec<u64>, DriverFailurePresentation> {
        let session = self.get(id).ok_or(DriverFailurePresentation::Disconnected)?;
        session.apply_changes(statements, expect_one, control).await
    }

    #[cfg(any(test, feature = "test-support"))]
    pub async fn open_file_writable_for_tests(
        &self,
        path: &std::path::Path,
        read_only: bool,
        control: &OperationControl,
    ) -> Result<OpenedSession, String> {
        let file = crate::open_request::classify_file(path).map_err(|error| format!("{error:?}"))?;
        let mut saved = crate::open_request::file_connection(&file);
        saved.read_only = read_only;
        let label = path.display().to_string();
        self.open_writable(saved, label, control).await
    }

    pub async fn open_file(&self, path: &std::path::Path, control: &OperationControl) -> Result<OpenedSession, String> {
        let file = crate::open_request::classify_file(path).map_err(|error| format!("{error:?}"))?;
        let label = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let mut saved = crate::open_request::file_connection(&file);
        saved.read_only = true;
        self.open(saved, label, control).await
    }

    pub async fn export_tables_to_folder(
        &self,
        id: SessionId,
        tables: Vec<TableInfo>,
        job: crate::export_tables::TablesExportJob,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<Vec<crate::export_tables::TableResult>, String> {
        let session = self.get(id).ok_or("The connection is closed")?;
        let (connection, generation, _) = session.backup_identity();
        let driver_id = session.driver_id.clone();
        let driver = self
            .registry
            .get(&driver_id)
            .ok_or("The database driver is not available")?;
        if !driver.capabilities().supports_snapshot_export {
            return Err("This connection does not support consistent table export.".into());
        }
        let guard = self
            .database
            .guard_for_generation(connection, generation, bookie_policy::Principal::human_gui())
            .ok_or("The connection is closed")?;
        let control = crate::operation_control::bounded_with(job.timeout_secs, cancel.clone());
        let targets = crate::export_tables::prepare_targets(&guard, tables, &control).await?;
        let guard = Arc::new(guard);
        let handle = tokio::runtime::Handle::current();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let csv = bookie_core::export::CsvOptions::default();
            let request = crate::export_tables::TableExportRequest {
                driver_id: &driver_id,
                tables: &targets,
                directory: &job.directory,
                format: bookie_core::export::ResultFormat::Csv,
                extension: "csv",
                csv: &csv,
                max_rows_per_table: job.max_rows_per_table,
                timeout_secs: job.timeout_secs,
            };
            let run: crate::export_pages::PageRunner = Arc::new(move |future| handle.block_on(future));
            crate::export_tables::export_tables(guard, driver.as_ref(), &request, &run, &cancel, |_, _| {})
        })
        .await
        .unwrap_or_else(|_| Err("The export task stopped unexpectedly.".into()))
    }

    pub async fn run_backup_job(
        &self,
        id: SessionId,
        job: crate::database_backup::BackupJob,
        runtime_dir: &std::path::Path,
        control: &OperationControl,
    ) -> Result<(), DriverError> {
        let session = self.get(id).ok_or(DriverError::Disconnected)?;
        let (connection, generation, writable) = session.backup_identity();
        if matches!(job, crate::database_backup::BackupJob::Restore { .. }) && !writable {
            return Err(DriverError::ReadOnly);
        }
        crate::database_backup::ensure_backup_supported(&session.driver_id)?;
        let saved = bookie_storage::load_connections()
            .await
            .map_err(|error| DriverError::Internal(error.to_string()))?
            .into_iter()
            .find(|saved| saved.id == connection)
            .ok_or_else(|| DriverError::Internal("the saved connection is no longer available".into()))?;
        if saved.read_only && matches!(job, crate::database_backup::BackupJob::Restore { .. }) {
            return Err(DriverError::ReadOnly);
        }
        let principal = bookie_policy::Principal::human_gui();
        let guard = self
            .database
            .guard_for_generation(connection, generation, principal.clone())
            .ok_or(DriverError::Disconnected)?;
        let driver = self
            .registry
            .get(&saved.driver_id)
            .ok_or_else(|| DriverError::Unsupported("the database driver is not available".into()))?;
        let environment = self.database.ssh_environment_for_connection(&saved, principal);
        let server_version = self
            .database
            .metadata(connection)
            .and_then(|metadata| metadata.server_version);
        let context = crate::database_backup::SavedBackupContext {
            guard: &guard,
            driver: driver.as_ref(),
            saved: &saved,
            environment: &environment,
            server_version: server_version.as_deref(),
            runtime_dir,
        };
        crate::database_backup::run_saved_backup_job(context, job, control).await
    }

    pub(crate) fn get(&self, id: SessionId) -> Option<Arc<ReadSession>> {
        self.sessions
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&id)
            .cloned()
    }

    pub fn supports_snapshot_cursor(&self, id: SessionId) -> bool {
        self.get(id).is_some_and(|session| session.supports_snapshot_cursor())
    }

    pub async fn open_table_cursor(
        &self,
        id: SessionId,
        table: &TableInfo,
        control: &OperationControl,
    ) -> Result<
        Option<crate::read_session::PostgresTableCursor>,
        crate::driver_error_presentation::DriverFailurePresentation,
    > {
        let session = self
            .get(id)
            .ok_or(crate::driver_error_presentation::DriverFailurePresentation::Disconnected)?;
        session.open_table_cursor(table, control).await
    }

    pub async fn with_subscription<T, F, Fut>(
        &self,
        id: SessionId,
        registry: &DriverRegistry,
        control: &OperationControl,
        work: F,
    ) -> Result<T, crate::driver_error_presentation::DriverFailurePresentation>
    where
        F: FnOnce(ConnectOptions) -> Fut,
        Fut: std::future::Future<Output = Result<T, DriverError>>,
    {
        let session = self
            .get(id)
            .ok_or(crate::driver_error_presentation::DriverFailurePresentation::Disconnected)?;
        session.with_subscription(registry, control, work).await
    }

    pub async fn sample_subscription(
        &self,
        id: SessionId,
        request: SubscriptionRequest,
        control: &OperationControl,
    ) -> Result<SubscriptionSample, DriverFailurePresentation> {
        bookie_core::check_pre_dispatch(control).map_err(|error| crate::driver_error_presentation::classify(&error))?;
        let session = self.get(id).ok_or(DriverFailurePresentation::Disconnected)?;
        let sampler = self
            .subscription_sampler
            .as_ref()
            .ok_or_else(|| DriverFailurePresentation::Unsupported("subscription sampling is unavailable".into()))?;
        let driver_id = session.driver_id.clone();
        sampler
            .validate(&driver_id, &request)
            .map_err(|error| crate::driver_error_presentation::classify(&error))?;
        session
            .with_subscription(&self.registry, control, |options| {
                sampler.sample(&driver_id, options, request)
            })
            .await
    }

    pub fn close(&self, id: SessionId) -> bool {
        self.sessions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id)
            .is_some()
    }

    async fn open(
        &self,
        saved: bookie_storage::SavedConnection,
        label: String,
        control: &OperationControl,
    ) -> Result<OpenedSession, String> {
        let session = Box::pin(ReadSession::open_controlled(
            saved,
            Arc::clone(&self.registry),
            Arc::clone(&self.database),
            control.cancellation_token().clone(),
        ))
        .await
        .map_err(|failure| open_failure_message(&failure))?;
        Ok(self.register(session, label))
    }

    async fn open_writable(
        &self,
        saved: bookie_storage::SavedConnection,
        label: String,
        control: &OperationControl,
    ) -> Result<OpenedSession, String> {
        let session = Box::pin(ReadSession::open_writable_controlled(
            saved,
            Arc::clone(&self.registry),
            Arc::clone(&self.database),
            control.cancellation_token().clone(),
        ))
        .await
        .map_err(|failure| open_failure_message(&failure))?;
        Ok(self.register(session, label))
    }

    fn register(&self, session: ReadSession, label: String) -> OpenedSession {
        let summary = OpenedSession {
            session_id: SessionId::new(),
            label,
            driver_id: session.driver_id.clone(),
            tables: session.tables.clone(),
            supports_snapshot_cursor: session.supports_snapshot_cursor(),
        };
        self.sessions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(summary.session_id, Arc::new(session));
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::DatabaseDriver;

    #[tokio::test]
    async fn a_writable_saved_configuration_cannot_write_through_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("store.db");
        std::fs::File::create(&path).unwrap();
        let writer = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: path.to_string_lossy().into_owned(),
                ..ConnectOptions::default()
            })
            .await
            .unwrap();
        writer
            .execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();
        writer.execute("INSERT INTO items VALUES (1, 'a')").await.unwrap();
        let mut saved = crate::open_request::file_connection(&crate::open_request::classify_file(&path).unwrap());
        saved.read_only = false;
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(drivers_sqlite::SqliteDriver));
        let store = ReadSessionStore::new(Arc::new(registry), Arc::new(DatabaseService::new_isolated()));
        let control = OperationControl::with_timeout(Duration::from_secs(5));

        let opened = store.open(saved, "writable".into(), &control).await.unwrap();

        let session = store.get(opened.session_id).unwrap();
        for sql in [
            "INSERT INTO items VALUES (2, 'x')",
            "UPDATE items SET name = 'x'",
            "DELETE FROM items",
            "CREATE TABLE extra (id INTEGER)",
        ] {
            assert!(session.run_query(sql, &control).await.is_err(), "write accepted: {sql}");
        }
        let rows = writer.query("SELECT COUNT(*) FROM items").await.unwrap();
        assert_eq!(
            rows.rows.first().and_then(|row| row.first()),
            Some(&bookie_core::Value::Int(1))
        );
    }

    #[tokio::test]
    async fn a_bound_query_returns_rows_and_a_bound_write_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bound.db");
        std::fs::File::create(&path).unwrap();
        let writer = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: path.to_string_lossy().into_owned(),
                ..ConnectOptions::default()
            })
            .await
            .unwrap();
        writer
            .execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();
        writer
            .execute("INSERT INTO items VALUES (1, 'a'), (2, 'b')")
            .await
            .unwrap();
        let saved = crate::open_request::file_connection(&crate::open_request::classify_file(&path).unwrap());
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(drivers_sqlite::SqliteDriver));
        let store = ReadSessionStore::new(Arc::new(registry), Arc::new(DatabaseService::new_isolated()));
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let opened = store.open(saved, "bound".into(), &control).await.unwrap();
        let session = store.get(opened.session_id).unwrap();

        let found = session
            .run_query_params(
                "SELECT name FROM items WHERE id = ?",
                &[bookie_core::Value::Int(2)],
                &control,
            )
            .await
            .unwrap();
        assert_eq!(found.rows, vec![vec![bookie_core::Value::Text("b".into())]]);

        let refused = session
            .run_query_params(
                "DELETE FROM items WHERE id = ?",
                &[bookie_core::Value::Int(1)],
                &control,
            )
            .await;
        assert!(refused.is_err(), "a bound write must not bypass the read-only guard");
        let rows = writer.query("SELECT COUNT(*) FROM items").await.unwrap();
        assert_eq!(
            rows.rows.first().and_then(|row| row.first()),
            Some(&bookie_core::Value::Int(2))
        );
    }

    async fn writable_fixture(
        read_only: bool,
    ) -> (tempfile::TempDir, std::path::PathBuf, ReadSessionStore, OpenedSession) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("write.db");
        std::fs::File::create(&path).unwrap();
        let writer = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: path.to_string_lossy().into_owned(),
                ..ConnectOptions::default()
            })
            .await
            .unwrap();
        writer
            .execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();
        writer
            .execute("INSERT INTO items VALUES (1, 'a'), (2, 'b')")
            .await
            .unwrap();
        let mut saved = crate::open_request::file_connection(&crate::open_request::classify_file(&path).unwrap());
        saved.read_only = read_only;
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(drivers_sqlite::SqliteDriver));
        let store = ReadSessionStore::new(Arc::new(registry), Arc::new(DatabaseService::new_isolated()));
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let opened = store.open_writable(saved, "writable".into(), &control).await.unwrap();
        (directory, path, store, opened)
    }

    async fn name_of(path: &std::path::Path, id: i64) -> String {
        let reader = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: path.to_string_lossy().into_owned(),
                ..ConnectOptions::default()
            })
            .await
            .unwrap();
        let rows = reader
            .query(&format!("SELECT name FROM items WHERE id = {id}"))
            .await
            .unwrap();
        match rows.rows.first().and_then(|row| row.first()) {
            Some(bookie_core::Value::Text(text)) => text.clone(),
            other => panic!("unexpected {other:?}"),
        }
    }

    fn rename(id: i64, name: &str) -> Vec<(String, Vec<bookie_core::Value>)> {
        vec![(
            "UPDATE items SET name = ? WHERE id = ?".to_string(),
            vec![bookie_core::Value::Text(name.into()), bookie_core::Value::Int(id)],
        )]
    }

    #[tokio::test]
    async fn a_writable_session_applies_a_batch_in_one_transaction() {
        let (_directory, path, store, opened) = writable_fixture(false).await;
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let affected = store
            .apply_changes(opened.session_id, &rename(1, "changed"), &[0], &control)
            .await
            .expect("the write is applied");
        assert_eq!(affected, vec![1]);
        assert_eq!(name_of(&path, 1).await, "changed");
        assert_eq!(name_of(&path, 2).await, "b");
    }

    #[tokio::test]
    async fn an_overwrite_check_rolls_the_whole_batch_back() {
        let (_directory, path, store, opened) = writable_fixture(false).await;
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let mut batch = rename(1, "changed");
        batch.extend(rename(999, "ghost"));
        let result = store.apply_changes(opened.session_id, &batch, &[0, 1], &control).await;
        assert!(
            format!("{result:?}").contains("RolledBack"),
            "expected a rolled-back save, got {result:?}"
        );
        assert_eq!(name_of(&path, 1).await, "a", "the first update was rolled back");
    }

    #[tokio::test]
    async fn a_read_only_saved_connection_refuses_writes_even_when_opened_writable() {
        let (_directory, path, store, opened) = writable_fixture(true).await;
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let result = store
            .apply_changes(opened.session_id, &rename(1, "changed"), &[0], &control)
            .await;
        assert!(
            matches!(&result, Err(DriverFailurePresentation::PolicyDenied(message)) if message.contains("read-only")),
            "expected the read-only policy denial, got {result:?}"
        );
        assert_eq!(name_of(&path, 1).await, "a");
    }

    #[tokio::test]
    async fn the_ordinary_open_path_stays_read_only_and_refuses_a_batch() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("plain.db");
        std::fs::File::create(&path).unwrap();
        let writer = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: path.to_string_lossy().into_owned(),
                ..ConnectOptions::default()
            })
            .await
            .unwrap();
        writer
            .execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();
        writer.execute("INSERT INTO items VALUES (1, 'a')").await.unwrap();
        let mut saved = crate::open_request::file_connection(&crate::open_request::classify_file(&path).unwrap());
        saved.read_only = false;
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(drivers_sqlite::SqliteDriver));
        let store = ReadSessionStore::new(Arc::new(registry), Arc::new(DatabaseService::new_isolated()));
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let opened = store.open(saved, "plain".into(), &control).await.unwrap();

        let result = store
            .apply_changes(opened.session_id, &rename(1, "changed"), &[0], &control)
            .await;

        assert!(
            matches!(&result, Err(DriverFailurePresentation::Unsupported(message)) if message.contains("opened for reading")),
            "expected the read-session refusal, got {result:?}"
        );
        assert_eq!(name_of(&path, 1).await, "a");
    }

    #[tokio::test]
    async fn a_writable_session_does_not_run_free_form_statements() {
        let (_directory, path, store, opened) = writable_fixture(false).await;
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let session = store.get(opened.session_id).unwrap();

        let write = session.run_query("UPDATE items SET name = 'x'", &control).await;
        let read = session.run_query("SELECT 1", &control).await;
        let bound = session
            .run_query_params("SELECT ?", &[bookie_core::Value::Int(1)], &control)
            .await;

        for result in [write, read, bound] {
            assert!(
                matches!(&result, Err(DriverFailurePresentation::Unsupported(message)) if message.contains("free-form")),
                "expected the free-form refusal, got {result:?}"
            );
        }
        assert_eq!(name_of(&path, 1).await, "a");
    }

    #[tokio::test]
    async fn a_change_that_holds_two_statements_is_refused_before_anything_runs() {
        let (_directory, path, store, opened) = writable_fixture(false).await;
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let two = vec![(
            "UPDATE items SET name = 'x' WHERE id = 1; UPDATE items SET name = 'y'".to_string(),
            Vec::new(),
        )];

        let result = store.apply_changes(opened.session_id, &two, &[0], &control).await;

        assert!(
            matches!(&result, Err(DriverFailurePresentation::Unsupported(message)) if message.contains("single statement")),
            "expected the single-statement refusal, got {result:?}"
        );
        assert_eq!(name_of(&path, 1).await, "a");
        assert_eq!(name_of(&path, 2).await, "b");
    }

    #[tokio::test]
    async fn a_closed_session_cannot_save() {
        let (_directory, path, store, opened) = writable_fixture(false).await;
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        assert!(store.close(opened.session_id));

        let result = store
            .apply_changes(opened.session_id, &rename(1, "changed"), &[0], &control)
            .await;

        assert!(
            matches!(result, Err(DriverFailurePresentation::Disconnected)),
            "{result:?}"
        );
        assert_eq!(name_of(&path, 1).await, "a");
    }

    async fn sqlite_store_with_session(
        read_only: bool,
    ) -> (ReadSessionStore, SessionId, tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("backup.db");
        std::fs::File::create(&path).unwrap();
        let writer = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: path.to_string_lossy().into_owned(),
                ..ConnectOptions::default()
            })
            .await
            .unwrap();
        writer
            .execute("CREATE TABLE items (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(drivers_sqlite::SqliteDriver));
        let store = ReadSessionStore::new(Arc::new(registry), Arc::new(DatabaseService::new_isolated()));
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let opened = store
            .open_file_writable_for_tests(&path, read_only, &control)
            .await
            .unwrap();
        (store, opened.session_id, directory, path)
    }

    fn backup_job(directory: &std::path::Path) -> crate::database_backup::BackupJob {
        crate::database_backup::BackupJob::Backup {
            output: directory.join("out.dump"),
        }
    }

    #[tokio::test]
    async fn a_backup_on_a_closed_session_reports_a_disconnection() {
        let (store, _, directory, _) = sqlite_store_with_session(false).await;
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let result = store
            .run_backup_job(
                SessionId::new(),
                backup_job(directory.path()),
                directory.path(),
                &control,
            )
            .await;
        assert!(matches!(result, Err(DriverError::Disconnected)), "{result:?}");
    }

    #[tokio::test]
    async fn only_postgresql_sessions_can_be_backed_up_and_nothing_is_written_otherwise() {
        let (store, session, directory, _) = sqlite_store_with_session(false).await;
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let job = backup_job(directory.path());
        let result = store
            .run_backup_job(session, job.clone(), directory.path(), &control)
            .await;
        assert!(
            matches!(&result, Err(DriverError::Unsupported(text)) if text.contains("PostgreSQL")),
            "{result:?}"
        );
        let written = matches!(&job, crate::database_backup::BackupJob::Backup { output } if output.exists());
        assert!(!written, "a refused backup leaves no file behind");
    }

    #[tokio::test]
    async fn a_restore_on_a_session_opened_for_reading_only_is_refused_before_anything_else() {
        let (store, _, directory, path) = sqlite_store_with_session(false).await;
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let read_only = store.open_file(&path, &control).await.unwrap();
        let job = crate::database_backup::BackupJob::Restore {
            input: directory.path().join("in.dump"),
        };
        let result = store
            .run_backup_job(read_only.session_id, job, directory.path(), &control)
            .await;
        assert!(matches!(result, Err(DriverError::ReadOnly)), "{result:?}");
    }

    fn export_job(directory: &std::path::Path) -> crate::export_tables::TablesExportJob {
        crate::export_tables::TablesExportJob {
            directory: directory.to_path_buf(),
            max_rows_per_table: Some(10),
            timeout_secs: 5,
        }
    }

    #[tokio::test]
    async fn exporting_tables_on_a_closed_session_is_refused() {
        let (store, _, directory, _) = sqlite_store_with_session(false).await;
        let result = store
            .export_tables_to_folder(
                SessionId::new(),
                vec![TableInfo {
                    schema: None,
                    name: "items".into(),
                }],
                export_job(directory.path()),
                &tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert!(result.unwrap_err().contains("closed"));
    }

    #[tokio::test]
    async fn a_connection_without_a_consistent_snapshot_cannot_export_tables_and_writes_nothing() {
        let (store, session, directory, _) = sqlite_store_with_session(false).await;
        let out = directory.path().join("export");
        std::fs::create_dir_all(&out).unwrap();
        let result = store
            .export_tables_to_folder(
                session,
                vec![TableInfo {
                    schema: None,
                    name: "items".into(),
                }],
                export_job(&out),
                &tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert!(result.unwrap_err().contains("consistent table export"));
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    }
}
