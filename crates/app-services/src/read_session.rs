use std::sync::Arc;

use bookie_core::{
    Connection, DriverRegistry, OperationControl, QueryCursorId, QueryCursorPage, QueryResult, Session, TableInfo,
    ViewEditability,
};
use bookie_policy::{AuditedSession, PolicyGuard, Principal};
use bookie_storage::SavedConnection;
use bookie_transport::SshEnvironment;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::connection_service::open_saved;
use crate::database_service::DatabaseService;
use crate::driver_error_presentation::{DriverFailurePresentation, classify};
use crate::open_failure::OpenFailure;

const OPEN_TIMEOUT_SECS: u32 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionSummary {
    pub id: Uuid,
    pub name: String,
    pub driver_id: String,
    pub read_only: bool,
    pub environment: bookie_core::Environment,
    pub color: Option<String>,
}

pub async fn list_saved_connections() -> Result<Vec<ConnectionSummary>, String> {
    let saved = bookie_storage::load_connections()
        .await
        .map_err(|error| error.to_string())?;
    let organization = bookie_storage::load_organization().await.unwrap_or_else(|error| {
        tracing::warn!(%error, "connection colours unavailable");
        bookie_storage::ConnectionOrganizationIndex::default()
    });
    Ok(saved
        .into_iter()
        .map(|saved| ConnectionSummary {
            color: organization
                .get(saved.id)
                .color
                .as_deref()
                .and_then(bookie_storage::connection_color)
                .map(str::to_owned),
            id: saved.id,
            name: saved.name,
            driver_id: saved.driver_id,
            read_only: saved.read_only,
            environment: saved.environment,
        })
        .collect())
}

pub use bookie_storage::{CONNECTION_COLORS, connection_color_rgb};

pub async fn set_connection_color(id: Uuid, color: Option<&str>) -> Result<(), String> {
    let mut organization = bookie_storage::load_organization()
        .await
        .map_err(|error| error.to_string())?;
    let updated = organization.get(id).with_color(color);
    organization.set(id, updated).map_err(|error| error.to_string())?;
    bookie_storage::save_organization(&organization)
        .await
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableStructure {
    pub indexes: Vec<bookie_core::IndexInfo>,
    pub foreign_keys: Vec<bookie_core::ForeignKeyInfo>,
    pub referenced_by: Vec<bookie_core::InboundForeignKeyInfo>,
}

#[derive(Clone, Copy)]
pub enum SessionAccess {
    ReadOnly,
    Writable,
}

pub struct ReadSession {
    database: Arc<DatabaseService>,
    id: Uuid,
    generation: Uuid,
    supports_snapshot_cursor: bool,
    supports_updatable_views: bool,
    writable: bool,
    pub driver_id: String,
    pub tables: Vec<TableInfo>,
    views: Vec<TableInfo>,
}

pub(crate) async fn saved_connection(id: Uuid) -> Result<Option<SavedConnection>, bookie_storage::StorageError> {
    Ok(bookie_storage::load_connections()
        .await?
        .into_iter()
        .find(|saved| saved.id == id))
}

impl ReadSession {
    pub(crate) fn guard(&self) -> Result<PolicyGuard, DriverFailurePresentation> {
        self.database
            .guard_for_generation(self.id, self.generation, Principal::human_gui())
            .ok_or(DriverFailurePresentation::Disconnected)
    }

    pub(crate) fn backup_identity(&self) -> (Uuid, bool) {
        (self.id, self.writable)
    }
}

pub struct PostgresTableCursor {
    session: AuditedSession,
    cursor: QueryCursorId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableSort {
    pub column: String,
    pub ascending: bool,
}

impl PostgresTableCursor {
    pub async fn open(
        guard: &PolicyGuard,
        table: &TableInfo,
        control: &OperationControl,
    ) -> Result<Self, DriverFailurePresentation> {
        Self::open_sorted(guard, table, None, control).await
    }

    pub async fn open_sorted(
        guard: &PolicyGuard,
        table: &TableInfo,
        sort: Option<&TableSort>,
        control: &OperationControl,
    ) -> Result<Self, DriverFailurePresentation> {
        let columns = guard
            .fetch_columns_controlled(table.schema.as_deref(), &table.name, control)
            .await
            .map_err(|error| classify(&error))?;
        let key: Vec<String> = columns
            .iter()
            .filter(|column| column.primary_key)
            .map(|column| column.name.clone())
            .collect();
        let sql = match sort {
            Some(sort) => table_select_sorted(table, &columns, sort).map_err(DriverFailurePresentation::Unsupported)?,
            None => table_select(table, &key),
        };
        let mut session = guard
            .open_session_with_audit()
            .await
            .map_err(|error| classify(&error))?;
        if let Err(error) = session
            .query_params_controlled(crate::snapshot_export::SNAPSHOT_BEGIN, &[], control)
            .await
        {
            let _ = Box::new(session).close().await;
            return Err(classify(&error));
        }
        match open_table_cursor(&mut session, &sql, control).await {
            Ok(cursor) => Ok(Self { session, cursor }),
            Err(error) => {
                let _ = Box::new(session).close().await;
                Err(error)
            }
        }
    }

    pub async fn next_page(
        &mut self,
        control: &OperationControl,
    ) -> Result<QueryCursorPage, DriverFailurePresentation> {
        self.session
            .fetch_query_cursor_page_controlled_with_audit(self.cursor, control)
            .await
            .result
            .map_err(|error| classify(&error))
    }

    pub async fn close(self) -> Result<(), DriverFailurePresentation> {
        let cursor_control = OperationControl::with_timeout(std::time::Duration::from_secs(5));
        let mut session = self.session;
        let cursor_result = session
            .close_query_cursor_controlled_with_audit(self.cursor, &cursor_control)
            .await
            .result;
        let session_result = Box::new(session).close().await;
        if let Err(error) = cursor_result
            && !matches!(error, bookie_core::DriverError::Unsupported(_))
        {
            return Err(classify(&error));
        }
        session_result.map_err(|error| classify(&error))
    }
}

async fn is_base_table(
    guard: &PolicyGuard,
    table: &TableInfo,
    control: &OperationControl,
) -> Result<bool, DriverFailurePresentation> {
    let tables = crate::catalog::list_tables_excluding(guard, &[], control)
        .await
        .map_err(|error| classify(&error))?;
    Ok(tables.contains(table))
}

fn table_select(table: &TableInfo, key: &[String]) -> String {
    let quote = |name: &str| bookie_core::sql_dialect::quote_ident("postgres", name);
    let relation = table.schema.as_deref().map_or_else(
        || quote(&table.name),
        |schema| format!("{}.{}", quote(schema), quote(&table.name)),
    );
    if key.is_empty() {
        return format!("SELECT * FROM {relation}");
    }
    let order = key.iter().map(|name| quote(name)).collect::<Vec<_>>().join(", ");
    format!("SELECT * FROM {relation} ORDER BY {order}")
}

fn table_select_sorted(
    table: &TableInfo,
    columns: &[bookie_core::ColumnInfo],
    sort: &TableSort,
) -> Result<String, String> {
    let mut matching = columns
        .iter()
        .enumerate()
        .filter(|(_, column)| column.name == sort.column);
    let Some((index, _)) = matching.next() else {
        return Err("The requested sort column no longer exists.".into());
    };
    if matching.next().is_some() {
        return Err("The requested sort column is ambiguous.".into());
    }
    let filter = bookie_core::FilterSet::default();
    let target = crate::browse_query::BrowseTarget {
        driver_id: "postgres",
        schema: table.schema.as_deref(),
        table: &table.name,
        columns,
        filter: &filter,
        hidden_columns: None,
        long_text_preview: false,
        supports_reverse_keyset: false,
    };
    target
        .full_select(Some((index, sort.ascending)), false)
        .map(|query| query.sql)
        .map_err(|error| error.to_string())
}

async fn open_table_cursor(
    session: &mut AuditedSession,
    sql: &str,
    control: &OperationControl,
) -> Result<QueryCursorId, DriverFailurePresentation> {
    session
        .open_query_cursor_controlled_with_audit(sql, &[], control)
        .await
        .result
        .map_err(|error| classify(&error))
}

impl ReadSession {
    pub fn supports_snapshot_cursor(&self) -> bool {
        self.supports_snapshot_cursor
    }

    pub async fn open(
        mut saved: SavedConnection,
        registry: Arc<DriverRegistry>,
        database: Arc<DatabaseService>,
        cancellation: CancellationToken,
        access: SessionAccess,
    ) -> Result<Self, OpenFailure> {
        let writable = matches!(access, SessionAccess::Writable);
        check_open_cancellation(&cancellation)?;
        if !writable {
            saved.read_only = true;
        }
        let capabilities = registry.get(&saved.driver_id).map(|driver| driver.capabilities());
        let supports_snapshot_cursor = capabilities.is_some_and(|capabilities| capabilities.supports_snapshot_export);
        let supports_updatable_views = capabilities.is_some_and(|capabilities| capabilities.supports_updatable_views);
        let prepared = open_saved(
            registry,
            saved,
            Vec::new(),
            OPEN_TIMEOUT_SECS,
            SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Refuse),
            cancellation.clone(),
            database.candidate_guard_factory(),
        )
        .await?;
        check_open_cancellation(&cancellation)?;
        let id = prepared.id();
        let views = prepared.views.clone();
        let mut tables = prepared.tables.clone();
        tables.extend(views.iter().cloned());
        let (activated, generation) = prepared
            .activate_with_generation(&database)
            .ok_or_else(|| OpenFailure::Other("the connection is already open in another window".into()))?;
        Ok(Self {
            database,
            id,
            generation,
            supports_snapshot_cursor,
            supports_updatable_views,
            writable,
            driver_id: activated.driver_id,
            tables,
            views,
        })
    }

    pub async fn read_page(
        &self,
        table: &TableInfo,
        offset: u64,
        limit: u64,
        control: &OperationControl,
    ) -> Result<QueryResult, DriverFailurePresentation> {
        if limit == 0 || limit > bookie_core::MAX_QUERY_CURSOR_PAGE_ROWS as u64 {
            return Err(DriverFailurePresentation::Unsupported(format!(
                "Page size must be between 1 and {} rows.",
                bookie_core::MAX_QUERY_CURSOR_PAGE_ROWS
            )));
        }
        let guard = self.guard()?;
        guard
            .fetch_rows_controlled(table.schema.as_deref(), &table.name, offset, limit, control)
            .await
            .map_err(|error| classify(&error))
    }

    pub async fn read_columns(
        &self,
        table: &TableInfo,
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::ColumnInfo>, DriverFailurePresentation> {
        let guard = self.guard()?;
        guard
            .fetch_columns_controlled(table.schema.as_deref(), &table.name, control)
            .await
            .map_err(|error| classify(&error))
    }

    pub async fn read_view_editability(
        &self,
        table: &TableInfo,
        control: &OperationControl,
    ) -> Result<Option<ViewEditability>, DriverFailurePresentation> {
        let known_view = self.views.contains(table);
        if !known_view && self.tables.contains(table) {
            return Ok(None);
        }
        let guard = self.guard()?;
        if !self.supports_updatable_views {
            if !known_view && is_base_table(&guard, table, control).await? {
                return Ok(None);
            }
            return Ok(Some(ViewEditability::NotUpdatable));
        }
        guard
            .fetch_view_editability_controlled(table.schema.as_deref(), &table.name, control)
            .await
            .map(Some)
            .map_err(|error| classify(&error))
    }

    pub async fn read_enum_labels(
        &self,
        type_name: &bookie_core::QualifiedTypeName,
        control: &OperationControl,
    ) -> Result<Vec<String>, DriverFailurePresentation> {
        let guard = self.guard()?;
        guard
            .fetch_enum_labels_controlled(&type_name.schema, &type_name.name, control)
            .await
            .map_err(|error| classify(&error))
    }

    pub async fn server_version(
        &self,
        control: &OperationControl,
    ) -> Result<(Option<String>, std::time::Duration), DriverFailurePresentation> {
        let guard = self.guard()?;
        let started = std::time::Instant::now();
        let version = guard
            .server_version_controlled(control)
            .await
            .map_err(|error| classify(&error))?;
        Ok((version, started.elapsed()))
    }

    pub async fn table_sizes(
        &self,
        control: &OperationControl,
    ) -> Result<crate::table_sizes::SizeIndex, DriverFailurePresentation> {
        let guard = self.guard()?;
        crate::table_sizes::load_table_sizes(&guard, control)
            .await
            .map_err(|error| classify(&error))
    }

    pub async fn read_structure(
        &self,
        table: &TableInfo,
        capabilities: bookie_core::DriverCapabilities,
        control: &OperationControl,
    ) -> Result<TableStructure, DriverFailurePresentation> {
        let guard = self.guard()?;
        let schema = table.schema.as_deref();
        let indexes = if capabilities.supports_index_metadata {
            guard
                .fetch_indexes_controlled(schema, &table.name, control)
                .await
                .map_err(|error| classify(&error))?
        } else {
            Vec::new()
        };
        let foreign_keys = if capabilities.supports_foreign_key_metadata {
            guard
                .fetch_foreign_keys_controlled(schema, &table.name, control)
                .await
                .map_err(|error| classify(&error))?
        } else {
            Vec::new()
        };
        let referenced_by = if capabilities.supports_foreign_key_metadata {
            guard
                .fetch_inbound_foreign_keys_controlled(schema, &table.name, control)
                .await
                .map_err(|error| classify(&error))?
        } else {
            Vec::new()
        };
        Ok(TableStructure {
            indexes,
            foreign_keys,
            referenced_by,
        })
    }

    pub async fn read_catalog(
        &self,
        kind: bookie_core::CatalogObjectKind,
        control: &OperationControl,
    ) -> Result<Vec<bookie_core::CatalogObject>, DriverFailurePresentation> {
        let guard = self.guard()?;
        crate::catalog::list_objects_excluding(&guard, kind, None, &[], control)
            .await
            .map_err(|error| classify(&error))
    }

    pub async fn read_data_dictionary(
        &self,
        capabilities: bookie_core::DriverCapabilities,
        control: &OperationControl,
    ) -> Result<QueryResult, DriverFailurePresentation> {
        let guard = self.guard()?;
        let tables = crate::catalog::list_tables_excluding(&guard, &[], control)
            .await
            .map_err(|error| classify(&error))?;
        crate::data_dictionary::load(&guard, None, &tables, capabilities, control)
            .await
            .map_err(|error| classify(&error))
    }

    pub async fn open_table_cursor(
        &self,
        table: &TableInfo,
        control: &OperationControl,
    ) -> Result<Option<PostgresTableCursor>, DriverFailurePresentation> {
        self.open_table_cursor_sorted(table, None, control).await
    }

    pub async fn open_table_cursor_sorted(
        &self,
        table: &TableInfo,
        sort: Option<&TableSort>,
        control: &OperationControl,
    ) -> Result<Option<PostgresTableCursor>, DriverFailurePresentation> {
        if !self.supports_snapshot_cursor {
            if sort.is_some() {
                return Err(DriverFailurePresentation::Unsupported(
                    "Server-side table sorting is not available for this connection.".into(),
                ));
            }
            return Ok(None);
        }
        let guard = self.guard()?;
        PostgresTableCursor::open_sorted(&guard, table, sort, control)
            .await
            .map(Some)
    }
}

fn check_open_cancellation(cancellation: &CancellationToken) -> Result<(), OpenFailure> {
    let control = OperationControl::new(cancellation.clone(), None);
    bookie_core::check_pre_dispatch(&control).map_err(|error| OpenFailure::from_driver(&error))
}

impl ReadSession {
    fn refuse_free_form_on_a_writable_session(&self) -> Result<(), DriverFailurePresentation> {
        if self.writable {
            return Err(DriverFailurePresentation::Unsupported(
                "a session opened for saving changes does not run free-form statements".into(),
            ));
        }
        Ok(())
    }

    pub async fn apply_changes(
        &self,
        statements: &[(String, Vec<bookie_core::Value>)],
        expect_one: &[usize],
        control: &OperationControl,
    ) -> Result<Vec<u64>, DriverFailurePresentation> {
        if !self.writable {
            return Err(DriverFailurePresentation::Unsupported(
                "this session was opened for reading and cannot save changes".into(),
            ));
        }
        if let Some(index) = statements
            .iter()
            .position(|(sql, _)| bookie_core::sql_lex::split_statements(sql, &self.driver_id).len() != 1)
        {
            return Err(DriverFailurePresentation::Unsupported(format!(
                "change {} is not a single statement",
                index + 1
            )));
        }
        let guard = self.guard()?;
        guard
            .execute_in_transaction_checked_controlled(statements, expect_one, control)
            .await
            .map_err(|error| classify(&error))
    }

    pub async fn run_query_batch(
        &self,
        sql: &str,
        params: &[bookie_core::Value],
        control: &OperationControl,
    ) -> Result<bookie_core::QueryResultBatch, DriverFailurePresentation> {
        self.refuse_free_form_on_a_writable_session()?;
        let guard = self.guard()?;
        guard
            .query_result_sets_controlled(sql, params, control)
            .await
            .map_err(|error| classify(&error))
    }

    pub async fn open_interactive(
        &self,
    ) -> Result<crate::interactive_session::SharedSession, DriverFailurePresentation> {
        let guard = self.guard()?;
        let session = guard
            .open_session_with_audit()
            .await
            .map_err(|error| classify(&error))?;
        Ok(Arc::new(tokio::sync::Mutex::new(Some(
            crate::interactive_session::SessionHandle::Audited(session),
        ))))
    }

    pub async fn run_query(
        &self,
        sql: &str,
        control: &OperationControl,
    ) -> Result<QueryResult, DriverFailurePresentation> {
        self.refuse_free_form_on_a_writable_session()?;
        let guard = self.guard()?;
        guard
            .query_controlled(sql, control)
            .await
            .map_err(|error| classify(&error))
    }

    pub async fn run_query_params(
        &self,
        sql: &str,
        params: &[bookie_core::Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverFailurePresentation> {
        self.refuse_free_form_on_a_writable_session()?;
        let guard = self.guard()?;
        guard
            .query_params_controlled(sql, params, control)
            .await
            .map_err(|error| classify(&error))
    }
}

impl ReadSession {
    pub async fn with_subscription<T, F, Fut>(
        &self,
        registry: &bookie_core::DriverRegistry,
        control: &OperationControl,
        work: F,
    ) -> Result<T, DriverFailurePresentation>
    where
        F: FnOnce(bookie_core::ConnectOptions) -> Fut,
        Fut: std::future::Future<Output = Result<T, bookie_core::DriverError>>,
    {
        let guard = self.guard()?;
        let saved = crate::read_session::saved_connection(self.id)
            .await
            .map_err(|error| DriverFailurePresentation::Unsupported(error.to_string()))?
            .ok_or_else(|| {
                DriverFailurePresentation::Unsupported("only a saved connection can be subscribed to".into())
            })?;
        let driver = registry.get(&self.driver_id).ok_or_else(|| {
            DriverFailurePresentation::Unsupported(format!("driver {} not registered", self.driver_id))
        })?;
        let environment = bookie_transport::SshEnvironment::builtin(bookie_ssh::UnknownHostKey::Refuse);
        crate::database_subscription::with_subscription_endpoint(&guard, &*driver, &saved, &environment, control, work)
            .await
            .map_err(|error| classify(&error))
    }
}

impl ReadSession {
    pub fn overview(&self) -> Vec<crate::database_service::ConnectionOverview> {
        self.database.connection_overview()
    }
}

impl Drop for ReadSession {
    fn drop(&mut self) {
        self.database.close_generation(self.id, self.generation);
    }
}

#[cfg(test)]
mod table_select_tests {
    use super::*;

    fn table(schema: Option<&str>, name: &str) -> TableInfo {
        TableInfo {
            schema: schema.map(str::to_owned),
            name: name.into(),
        }
    }

    fn column(name: &str, primary_key: bool) -> bookie_core::ColumnInfo {
        bookie_core::ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: false,
            primary_key,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    #[test]
    fn sorting_quotes_names_and_appends_unique_key_ties() {
        let columns = vec![
            column("tenant", true),
            column("id", true),
            column("x\"; DROP TABLE t; --", false),
        ];
        let request = TableSort {
            column: columns[2].name.clone(),
            ascending: false,
        };
        let sql = table_select_sorted(&table(Some("s\"chema"), "t"), &columns, &request).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"s\"\"chema\".\"t\" ORDER BY \"x\"\"; DROP TABLE t; --\" DESC NULLS LAST, \"tenant\" ASC, \"id\" ASC"
        );
    }

    #[test]
    fn sorting_rejects_unknown_or_ambiguous_metadata_columns() {
        let columns = vec![column("id", true), column("id", false)];
        for name in ["missing", "id"] {
            let request = TableSort {
                column: name.into(),
                ascending: true,
            };
            assert!(table_select_sorted(&table(None, "t"), &columns, &request).is_err());
        }
    }

    #[test]
    fn a_table_with_a_primary_key_is_read_in_key_order() {
        let sql = table_select(&table(Some("public"), "customers"), &["id".into()]);
        assert_eq!(sql, r#"SELECT * FROM "public"."customers" ORDER BY "id""#);
    }

    #[test]
    fn a_composite_key_orders_by_every_key_column_and_quotes_each() {
        let sql = table_select(&table(None, "t"), &["a b".into(), "c".into()]);
        assert_eq!(sql, r#"SELECT * FROM "t" ORDER BY "a b", "c""#);
    }

    #[test]
    fn a_table_without_a_primary_key_keeps_the_plain_select() {
        assert_eq!(
            table_select(&table(Some("s"), "log"), &[]),
            r#"SELECT * FROM "s"."log""#
        );
    }

    mod interactive {
        use std::time::Duration;

        use bookie_core::{ConnectOptions, DatabaseDriver, DriverRegistry, OperationControl, Session};

        use super::super::*;
        use crate::interactive_session::{SharedSession, commit_session, rollback_and_close};

        async fn fixture() -> (
            tempfile::TempDir,
            std::path::PathBuf,
            Box<dyn bookie_core::Connection>,
            SavedConnection,
        ) {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("interactive.db");
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
            (directory, path, writer, saved)
        }

        fn registry() -> Arc<DriverRegistry> {
            let mut registry = DriverRegistry::new();
            registry.register(Arc::new(drivers_sqlite::SqliteDriver));
            Arc::new(registry)
        }

        async fn name(writer: &dyn bookie_core::Connection) -> bookie_core::Value {
            let rows = writer.query("SELECT name FROM items WHERE id = 1").await.unwrap();
            rows.rows[0][0].clone()
        }

        async fn run(shared: &SharedSession, sql: &str) -> Result<(), bookie_core::DriverError> {
            let control = OperationControl::with_timeout(Duration::from_secs(5));
            let mut guard = shared.lock().await;
            let session = guard.as_mut().expect("open session");
            session.query_params_controlled(sql, &[], &control).await.map(|_| ())
        }

        #[tokio::test]
        async fn a_read_session_transaction_refuses_writes_and_changes_nothing() {
            let (_directory, _path, writer, saved) = fixture().await;
            let session = ReadSession::open(
                saved,
                registry(),
                Arc::new(DatabaseService::new_isolated()),
                CancellationToken::new(),
                SessionAccess::ReadOnly,
            )
            .await
            .unwrap();
            let shared = session.open_interactive().await.unwrap();
            for sql in [
                "UPDATE items SET name = 'x'",
                "DELETE FROM items",
                "INSERT INTO items VALUES (2, 'b')",
            ] {
                assert!(
                    run(&shared, sql).await.is_err(),
                    "write accepted on a read session: {sql}"
                );
            }
            assert_eq!(name(writer.as_ref()).await, bookie_core::Value::Text("a".into()));
            let _ = rollback_and_close(&shared).await;
        }

        #[tokio::test]
        async fn a_writable_transaction_rolls_back_and_commits_through_the_guard() {
            let (_directory, _path, writer, saved) = fixture().await;
            let session = ReadSession::open(
                saved,
                registry(),
                Arc::new(DatabaseService::new_isolated()),
                CancellationToken::new(),
                SessionAccess::Writable,
            )
            .await
            .unwrap();

            let shared = session.open_interactive().await.unwrap();
            run(&shared, "BEGIN").await.unwrap();
            run(&shared, "UPDATE items SET name = 'rolled back' WHERE id = 1")
                .await
                .unwrap();
            rollback_and_close(&shared).await.unwrap();
            assert_eq!(name(writer.as_ref()).await, bookie_core::Value::Text("a".into()));

            let shared = session.open_interactive().await.unwrap();
            run(&shared, "BEGIN").await.unwrap();
            run(&shared, "UPDATE items SET name = 'committed' WHERE id = 1")
                .await
                .unwrap();
            commit_session(&shared).await.unwrap();
            assert_eq!(
                name(writer.as_ref()).await,
                bookie_core::Value::Text("committed".into())
            );
            assert!(
                run(&shared, "BEGIN; UPDATE items SET name = 'hidden' WHERE id = 1")
                    .await
                    .is_err(),
                "transaction control hidden in a batch is refused"
            );
        }
    }
}
