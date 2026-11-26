use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bookie_app_services::command_bridge::{ChannelEventSink, Command, DispatchContext, Epoch, Event, dispatch};
use bookie_app_services::database_service::ConnectionOverview;
use bookie_app_services::driver_failure_message::driver_failure_message;
use bookie_app_services::er_diagram::read::SchemaRead;
use bookie_app_services::read_session::ConnectionSummary;
use bookie_app_services::read_session_store::{
    OpenedSession, ReadSessionStore, SessionId, SubscriptionRequest, SubscriptionSample, SubscriptionSampler,
};
use bookie_app_services::value_search::{
    SearchLimits, SearchOptions, SearchReport, SearchRequest, SearchSource, TableTarget, run_search,
};
use bookie_core::{ColumnInfo, DriverError, OperationControl, QueryResult, QueryResultBudget, QueryRows, TableInfo};
use tokio_util::sync::CancellationToken;

use crate::model::{Cell, ColumnMeta, Diff, FindResult, PAGE_ROWS, Predicate};
use crate::shell::table_label;
use crate::source::{ResultSource, ViewAccess, filter_buffered_rows, find_buffered_rows};

pub fn saved_connections() -> Result<Vec<ConnectionSummary>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|error| error.to_string())?;
    let epoch = Epoch(1);
    let current_epoch = AtomicU64::new(epoch.0);
    let control = OperationControl::with_timeout(Duration::from_secs(5));
    runtime.block_on(bridge_saved_connections(&control, epoch, &current_epoch))
}

#[derive(Clone, Debug)]
pub enum Origin {
    Saved(String),
    File(std::path::PathBuf),
}

#[derive(Clone, Debug)]
pub struct QueryFailure {
    pub cancelled: bool,
    pub message: String,
    pub position: Option<usize>,
    pub hint: Option<String>,
}

impl From<&str> for QueryFailure {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}

impl From<String> for QueryFailure {
    fn from(message: String) -> Self {
        Self {
            cancelled: false,
            message,
            position: None,
            hint: None,
        }
    }
}

impl From<bookie_app_services::driver_error_presentation::DriverFailurePresentation> for QueryFailure {
    fn from(failure: bookie_app_services::driver_error_presentation::DriverFailurePresentation) -> Self {
        let position = match &failure {
            bookie_app_services::driver_error_presentation::DriverFailurePresentation::Query { position, .. } => {
                *position
            }
            _ => None,
        };
        let hint = match &failure {
            bookie_app_services::driver_error_presentation::DriverFailurePresentation::Query { hint, .. } => {
                hint.clone()
            }
            _ => None,
        };
        Self {
            cancelled: matches!(
                failure,
                bookie_app_services::driver_error_presentation::DriverFailurePresentation::Cancelled
            ),
            message: driver_failure_message(&failure),
            position,
            hint,
        }
    }
}

pub const ROW_CAP: u64 = 100_000;

const PAGE_TIMEOUT: Duration = Duration::from_secs(30);

fn table_count_sql(driver_id: &str, table: &TableInfo) -> String {
    let quote = |identifier| bookie_core::sql_dialect::quote_ident(driver_id, identifier);
    let target = match table.schema.as_deref() {
        Some(schema) => format!("{}.{}", quote(schema), quote(&table.name)),
        None => quote(&table.name),
    };
    let count = if driver_id == "mssql" {
        "COUNT_BIG(*)"
    } else {
        "COUNT(*)"
    };
    format!("SELECT {count} FROM {target}")
}

pub struct LiveSession {
    runtime: tokio::runtime::Runtime,
    sessions: ReadSessionStore,
    session_id: SessionId,
    epoch: AtomicU64,
    pub driver_id: String,
    pub label: String,
    pub tables: Vec<TableInfo>,
    pub saved: Option<(uuid::Uuid, String)>,
    pub writable: Option<uuid::Uuid>,
    pub stamp: Option<String>,
    supports_snapshot_cursor: bool,
    column_cache: std::sync::Mutex<ColumnCache>,
}

type ColumnKey = (Option<String>, String);

#[derive(Default)]
struct ColumnCache {
    generation: u64,
    tables: std::collections::HashMap<ColumnKey, (std::time::Instant, Vec<ColumnInfo>)>,
}

impl ColumnCache {
    fn get(&self, key: &ColumnKey) -> Option<&(std::time::Instant, Vec<ColumnInfo>)> {
        self.tables.get(key)
    }

    fn forget(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.tables.clear();
    }

    fn remember(&mut self, read_generation: u64, key: ColumnKey, columns: &[ColumnInfo]) {
        if read_generation == self.generation {
            self.tables.insert(key, (std::time::Instant::now(), columns.to_vec()));
        }
    }
}

const COLUMN_CACHE_TTL: Duration = Duration::from_secs(30);

impl LiveSession {
    pub fn cache_identity(&self) -> String {
        self.saved
            .as_ref()
            .map_or_else(|| format!("{:?}", self.session_id), |(id, _)| id.to_string())
    }

    pub fn database_service(&self) -> &Arc<bookie_app_services::database_service::DatabaseService> {
        self.sessions.database_service()
    }

    pub fn supports_table_row_count(&self) -> bool {
        matches!(
            self.driver_id.as_str(),
            "cassandra" | "clickhouse" | "duckdb" | "mssql" | "mysql" | "oracle" | "postgres" | "sqlite"
        )
    }

    pub fn database_label(&self) -> &str {
        if self.driver_id == "sqlite" {
            "main"
        } else {
            &self.label
        }
    }

    pub fn open(origin: &Origin) -> Result<Arc<LiveSession>, String> {
        Self::open_controlled(origin, &CancellationToken::new())
    }

    pub fn open_controlled(origin: &Origin, cancellation: &CancellationToken) -> Result<Arc<LiveSession>, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let (sessions, opened, saved, writable, stamp) =
            runtime.block_on(open_session(origin, cancellation.clone()))?;
        let writable = saved.as_ref().filter(|_| writable).map(|(id, _)| *id);
        Ok(Arc::new(LiveSession {
            driver_id: opened.driver_id.clone(),
            runtime,
            sessions,
            session_id: opened.session_id,
            epoch: AtomicU64::new(0),
            column_cache: std::sync::Mutex::new(ColumnCache::default()),
            label: opened.label,
            tables: opened.tables,
            saved,
            writable,
            stamp,
            supports_snapshot_cursor: opened.supports_snapshot_cursor,
        }))
    }

    pub fn record_history(
        self: &Arc<Self>,
        sql: &str,
        started_at: std::time::SystemTime,
        elapsed: Duration,
        rows: Option<i64>,
        error: Option<String>,
    ) {
        let Some((connection_id, connection_name)) = self.saved.clone() else {
            return;
        };
        let statement = bookie_app_services::statement_history::Statement {
            query: sql.to_owned(),
            driver_id: self.driver_id.clone(),
            connection_id,
            connection_name,
            started_at,
            duration_ms: i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
            rows,
            error,
            cancelled: false,
            source: bookie_app_services::statement_history::Source::Editor,
        };
        self.runtime.spawn(async move {
            if let Err(error) = bookie_app_services::statement_history::record(statement).await {
                tracing::warn!(%error, "history record failed");
            }
        });
    }

    pub fn with_driver_id(self: &Arc<Self>, driver_id: &str) -> Arc<Self> {
        let registry = Arc::new(drivers_registry::default_registry());
        let sessions = ReadSessionStore::new(registry, crate::host_key::database_service());
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().build() else {
            return self.clone();
        };
        Arc::new(LiveSession {
            runtime,
            sessions,
            session_id: self.session_id,
            epoch: AtomicU64::new(0),
            column_cache: std::sync::Mutex::new(ColumnCache::default()),
            driver_id: driver_id.to_string(),
            label: self.label.clone(),
            tables: self.tables.clone(),
            saved: self.saved.clone(),
            writable: self.writable,
            stamp: self.stamp.clone(),
            supports_snapshot_cursor: self.supports_snapshot_cursor,
        })
    }

    pub fn run_query(self: &Arc<Self>, sql: &str, max_rows: u64) -> Result<LiveTable, String> {
        self.run_query_controlled(sql, max_rows, &CancellationToken::new())
    }

    pub fn run_query_controlled(
        self: &Arc<Self>,
        sql: &str,
        max_rows: u64,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        self.run_query_bound(sql, &[], max_rows, cancellation)
    }

    pub fn run_query_bound(
        self: &Arc<Self>,
        sql: &str,
        params: &[bookie_core::Value],
        max_rows: u64,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        self.run_query_bound_detailed(sql, params, max_rows, cancellation)
            .map_err(|error| error.message)
    }

    fn name_the_read_path(&self, mut failure: QueryFailure) -> QueryFailure {
        if self.writable.is_none() {
            return failure;
        }
        if !failure.message.starts_with("Blocked by policy:") {
            return failure;
        }
        if failure.message.contains("marked read-only") {
            failure.message =
                "Query tabs only read. To change data, press Begin transaction, run the statement, then Commit".into();
        } else if failure.message.contains("could not be parsed (typo?)") {
            failure.message = "This statement could not be parsed (typo?)".into();
        }
        failure
    }

    pub fn run_query_bound_detailed(
        self: &Arc<Self>,
        sql: &str,
        params: &[bookie_core::Value],
        max_rows: u64,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, QueryFailure> {
        if max_rows == 0 || max_rows > ROW_CAP {
            return Err(format!("Row limit must be between 1 and {ROW_CAP}").into());
        }
        let epoch = self.next_epoch();
        let started = std::time::Instant::now();
        let (result, messages) = self
            .runtime
            .block_on(query_pages(
                &self.sessions,
                self.session_id,
                (sql, params),
                max_rows,
                cancellation,
                epoch,
                &self.epoch,
            ))
            .map_err(|failure| self.name_the_read_path(failure))?;
        Ok(LiveTable {
            session: self.clone(),
            columns: result.columns.iter().map(column_meta).collect(),
            estimated_retained_bytes: result.estimated_row_bytes(),
            result,
            snapshot_consistent: false,
            messages,
            executed_sql: Some(sql.to_owned()),
            load_elapsed: started.elapsed(),
            table: None,
            view_access: ViewAccess::Editable,
        })
    }

    pub fn open_editor_session(
        &self,
        control: &OperationControl,
    ) -> Result<bookie_app_services::interactive_session::SharedSession, QueryFailure> {
        self.runtime.block_on(async {
            tokio::select! {
                biased;
                _ = control.cancellation_token().cancelled() => Err(QueryFailure::from(bookie_app_services::driver_error_presentation::DriverFailurePresentation::Cancelled)),
                _ = tokio::time::sleep(PAGE_TIMEOUT) => Err(QueryFailure::from("Opening the query session timed out".to_owned())),
                result = self.sessions.open_interactive(self.session_id) => result.map_err(QueryFailure::from),
            }
        })
    }

    pub fn close_editor_session(
        &self,
        shared: bookie_app_services::interactive_session::SharedSession,
    ) -> Result<(), QueryFailure> {
        self.runtime.block_on(async {
            tokio::time::timeout(
                PAGE_TIMEOUT,
                bookie_app_services::interactive_session::close_for_teardown(shared),
            )
            .await
            .map_err(|_| QueryFailure::from("Closing the query session timed out".to_owned()))?
            .map_err(|error| QueryFailure::from(bookie_app_services::driver_error_presentation::classify(&error)))
        })
    }

    pub fn run_editor_batch(
        self: &Arc<Self>,
        shared: Option<&bookie_app_services::interactive_session::SharedSession>,
        sql: &str,
        params: &[bookie_core::Value],
        control: &OperationControl,
        budget: &mut QueryResultBudget,
    ) -> Result<Vec<LiveTable>, QueryFailure> {
        let started = std::time::Instant::now();
        let mut batch = self
            .runtime
            .block_on(async {
                if let Some(shared) = shared {
                    let mut handle = shared.lock().await;
                    let session = handle
                        .as_mut()
                        .ok_or_else(|| QueryFailure::from("The query session closed".to_owned()))?;
                    session
                        .query_result_sets_with_audit(sql, params, control)
                        .await
                        .result
                        .map_err(|error| {
                            QueryFailure::from(bookie_app_services::driver_error_presentation::classify(&error))
                        })
                } else {
                    let epoch = Epoch(0);
                    bridge_query_detailed(
                        &self.sessions,
                        self.session_id,
                        (sql, params),
                        control,
                        epoch,
                        &AtomicU64::new(0),
                    )
                    .await
                }
            })
            .map_err(|error| self.name_the_read_path(error))?;
        let mut messages = std::mem::take(&mut batch.messages);
        if batch.result_sets.is_empty() {
            batch.result_sets.push(QueryResult {
                columns: Vec::new(),
                rows: QueryRows::new(),
                truncated: batch.truncated,
            });
        }
        let result = batch
            .result_sets
            .into_iter()
            .map(|mut result| {
                result.truncated = bound_query_result(&mut result, ROW_CAP as usize, budget) || batch.truncated;
                LiveTable {
                    session: self.clone(),
                    columns: result.columns.iter().map(column_meta).collect(),
                    estimated_retained_bytes: result.estimated_row_bytes(),
                    result,
                    snapshot_consistent: false,
                    messages: std::mem::take(&mut messages),
                    executed_sql: Some(sql.to_owned()),
                    load_elapsed: started.elapsed(),
                    table: None,
                    view_access: ViewAccess::Editable,
                }
            })
            .collect();
        Ok(result)
    }

    pub fn table_from_result(
        self: &Arc<Self>,
        result: QueryResult,
        sql: &str,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        let started = std::time::Instant::now();
        let result = bounded_result(result, ROW_CAP, cancellation)?;
        Ok(LiveTable {
            session: self.clone(),
            columns: result.columns.iter().map(column_meta).collect(),
            estimated_retained_bytes: result.estimated_row_bytes(),
            result,
            messages: Default::default(),
            snapshot_consistent: false,
            executed_sql: Some(sql.to_owned()),
            load_elapsed: started.elapsed(),
            table: None,
            view_access: ViewAccess::Editable,
        })
    }

    pub fn data_dictionary_controlled(
        self: &Arc<Self>,
        capabilities: bookie_core::DriverCapabilities,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        let epoch = self.next_epoch();
        let started = std::time::Instant::now();
        let result = self.runtime.block_on(dictionary_pages(
            &self.sessions,
            self.session_id,
            capabilities,
            cancellation,
            epoch,
            &self.epoch,
        ))?;
        Ok(LiveTable {
            session: self.clone(),
            columns: result.columns.iter().map(column_meta).collect(),
            estimated_retained_bytes: result.estimated_row_bytes(),
            result,
            snapshot_consistent: false,
            messages: Default::default(),
            executed_sql: None,
            load_elapsed: started.elapsed(),
            table: None,
            view_access: ViewAccess::Editable,
        })
    }

    pub fn overview(&self) -> Result<Vec<ConnectionOverview>, String> {
        let epoch = Epoch(self.epoch.load(Ordering::Relaxed));
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let event = self.runtime.block_on(dispatch_read(
            DispatchContext::Sessions(&self.sessions),
            Command::ReadConnectionStatus {
                epoch,
                session_id: self.session_id,
            },
            &control,
            epoch,
            &self.epoch,
        ))?;
        let Event::ConnectionStatusRead { outcome, .. } = event else {
            return Err("The command bridge returned an unexpected event".into());
        };
        Ok(outcome)
    }

    pub fn count_table_rows(
        &self,
        table: &TableInfo,
        where_sql: &str,
        cancellation: &CancellationToken,
    ) -> Result<u64, String> {
        if !self.supports_table_row_count() {
            return Err("Exact row counts are not available for this database type".into());
        }
        let (sql, params) = if where_sql.trim().is_empty() {
            (table_count_sql(&self.driver_id, table), Vec::new())
        } else {
            let columns = self.table_columns(table)?;
            let filter = where_filter(where_sql);
            let query = browse_target(&self.driver_id, table, &columns, &filter)
                .count()
                .map_err(|error| error.to_string())?;
            (query.sql, query.params)
        };
        let epoch = Epoch(self.epoch.load(Ordering::Relaxed));
        let control = OperationControl::new(
            cancellation.clone(),
            Some(tokio::time::Instant::now() + Duration::from_secs(60)),
        );
        let result = self.runtime.block_on(bridge_query(
            &self.sessions,
            self.session_id,
            (&sql, &params),
            &control,
            epoch,
            &self.epoch,
        ))?;
        let Some(row) = result.rows.first().filter(|row| row.len() == 1) else {
            return Err("The database did not return one exact row count".into());
        };
        if result.truncated || result.rows.len() != 1 {
            return Err("The database did not return one exact row count".into());
        }
        match &row[0] {
            bookie_core::Value::Int(count) => u64::try_from(*count).map_err(|_| "The row count was negative".into()),
            bookie_core::Value::Decimal(count) if count.scale() == 0 => {
                u64::try_from(count.mantissa()).map_err(|_| "The row count is outside the supported range".into())
            }
            bookie_core::Value::Text(count) => count
                .parse::<u64>()
                .map_err(|_| "The database returned an invalid row count".into()),
            _ => Err("The database returned an unsupported row count value".into()),
        }
    }

    pub fn table_columns(&self, table: &TableInfo) -> Result<Vec<ColumnInfo>, String> {
        self.read_columns(table, false, Duration::from_secs(10))
    }

    pub fn edit_columns(&self, table: &TableInfo) -> Result<(Vec<ColumnInfo>, ViewAccess), String> {
        let editability = self.view_editability(table);
        Ok(view_plan(self.table_columns(table)?, editability))
    }

    fn view_editability(&self, table: &TableInfo) -> Result<Option<bookie_core::ViewEditability>, String> {
        if self.writable.is_none() {
            return Ok(None);
        }
        let control = OperationControl::with_timeout(Duration::from_secs(10));
        self.runtime
            .block_on(self.sessions.read_view_editability(self.session_id, table, &control))
            .map_err(|failure| driver_failure_message(&failure))
    }

    pub fn enum_labels(&self, type_name: &bookie_core::QualifiedTypeName) -> Result<Vec<String>, String> {
        let control = OperationControl::with_timeout(Duration::from_secs(10));
        self.runtime
            .block_on(self.sessions.read_enum_labels(self.session_id, type_name, &control))
            .map_err(|failure| driver_failure_message(&failure))
    }

    pub fn completion_columns(&self, table: &TableInfo) -> Result<Vec<ColumnInfo>, String> {
        if let Some(columns) = self.cached_columns(table) {
            return Ok(columns);
        }
        self.read_columns(table, true, Duration::from_secs(1))
    }

    pub fn cached_columns(&self, table: &TableInfo) -> Option<Vec<ColumnInfo>> {
        let cache = self.column_cache.lock().ok()?;
        let (read_at, columns) = cache.get(&(table.schema.clone(), table.name.clone()))?;
        (read_at.elapsed() < COLUMN_CACHE_TTL).then(|| columns.clone())
    }

    pub fn forget_columns(&self) {
        if let Ok(mut cache) = self.column_cache.lock() {
            cache.forget();
        }
    }

    fn column_generation(&self) -> Option<u64> {
        self.column_cache.lock().ok().map(|cache| cache.generation)
    }

    fn remember_columns(&self, read_generation: Option<u64>, table: &TableInfo, columns: &[ColumnInfo]) {
        if let (Some(read_generation), Ok(mut cache)) = (read_generation, self.column_cache.lock()) {
            cache.remember(read_generation, (table.schema.clone(), table.name.clone()), columns);
        }
    }

    pub fn diagram_columns(&self, table: &TableInfo) -> Result<Vec<ColumnInfo>, String> {
        self.table_columns(table)
    }

    fn read_columns(
        &self,
        table: &TableInfo,
        drop_if_superseded: bool,
        timeout: Duration,
    ) -> Result<Vec<ColumnInfo>, String> {
        let control = OperationControl::with_timeout(timeout);
        let generation = self.column_generation();
        let epoch = Epoch(self.epoch.load(Ordering::Relaxed));
        let command = Command::ReadColumns {
            epoch,
            session_id: self.session_id,
            table: table.clone(),
        };
        let context = DispatchContext::Sessions(&self.sessions);
        let event = if drop_if_superseded {
            self.runtime
                .block_on(dispatch_read(context, command, &control, epoch, &self.epoch))?
        } else {
            self.runtime.block_on(dispatch_once(context, command, &control))?
        };
        let Event::ColumnsRead { outcome, .. } = event else {
            return Err("The command bridge returned an unexpected event".into());
        };
        let columns = outcome.map_err(|failure| driver_failure_message(&failure))?;
        self.remember_columns(generation, table, &columns);
        Ok(columns)
    }

    pub fn table_structure(
        &self,
        table: &TableInfo,
    ) -> Result<bookie_app_services::read_session::TableStructure, String> {
        let capabilities = drivers_registry::default_registry()
            .get(&self.driver_id)
            .map(|driver| driver.capabilities())
            .unwrap_or_default();
        let epoch = Epoch(self.epoch.load(Ordering::Relaxed));
        let control = OperationControl::with_timeout(Duration::from_secs(10));
        let event = self.runtime.block_on(dispatch_once(
            DispatchContext::Sessions(&self.sessions),
            Command::ReadStructure {
                epoch,
                session_id: self.session_id,
                table: table.clone(),
                capabilities,
            },
            &control,
        ))?;
        let Event::StructureRead { outcome, .. } = event else {
            return Err("The command bridge returned an unexpected event".into());
        };
        outcome.map_err(|failure| driver_failure_message(&failure))
    }

    pub fn catalog_kinds(&self) -> &'static [bookie_core::CatalogObjectKind] {
        drivers_registry::default_registry()
            .get(&self.driver_id)
            .map_or(&[][..], |driver| driver.capabilities().catalog_object_kinds)
    }

    pub fn catalog_objects(
        &self,
        kind: bookie_core::CatalogObjectKind,
    ) -> Result<Vec<bookie_core::CatalogObject>, String> {
        let epoch = Epoch(self.epoch.load(Ordering::Relaxed));
        let control = OperationControl::with_timeout(Duration::from_secs(10));
        let event = self.runtime.block_on(dispatch_once(
            DispatchContext::Sessions(&self.sessions),
            Command::ReadCatalog {
                epoch,
                session_id: self.session_id,
                kind,
            },
            &control,
        ))?;
        let Event::CatalogRead { outcome, .. } = event else {
            return Err("The command bridge returned an unexpected event".into());
        };
        outcome.map_err(|failure| driver_failure_message(&failure))
    }

    pub fn follow_pubsub(
        &self,
        channel: &str,
        cancellation: &CancellationToken,
        on_message: Box<dyn FnMut(bookie_app_services::read_session_store::SubscriptionMessage) + Send>,
    ) -> Result<u64, String> {
        let control = OperationControl::new(cancellation.clone(), None);
        self.runtime
            .block_on(self.sessions.follow_subscription(
                self.session_id,
                vec![channel.to_string()],
                &control,
                on_message,
            ))
            .map_err(|failure| driver_failure_message(&failure))
    }

    pub fn sample_pubsub(
        &self,
        patterns: &[String],
        max_messages: usize,
        window: Duration,
        cancellation: &CancellationToken,
    ) -> Result<SubscriptionSample, String> {
        let epoch = self.next_epoch();
        let deadline = tokio::time::Instant::now() + window + Duration::from_secs(15);
        let control = OperationControl::new(cancellation.clone(), Some(deadline));
        let event = self.runtime.block_on(dispatch_read(
            DispatchContext::Sessions(&self.sessions),
            Command::SampleSubscription {
                epoch,
                session_id: self.session_id,
                request: SubscriptionRequest {
                    patterns: patterns.to_vec(),
                    max_messages,
                    window,
                },
            },
            &control,
            epoch,
            &self.epoch,
        ))?;
        let Event::SubscriptionSampled { outcome, .. } = event else {
            return Err("The command bridge returned an unexpected event".into());
        };
        outcome.map_err(|failure| driver_failure_message(&failure))
    }

    pub fn load_table(self: &Arc<Self>, table: &TableInfo, max_rows: u64) -> Result<LiveTable, String> {
        self.load_table_controlled(table, max_rows, &CancellationToken::new())
    }

    pub fn load_table_controlled(
        self: &Arc<Self>,
        table: &TableInfo,
        max_rows: u64,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        self.load_table_sorted_controlled(table, max_rows, None, cancellation)
    }

    pub fn load_table_sorted_controlled(
        self: &Arc<Self>,
        table: &TableInfo,
        max_rows: u64,
        sort: Option<&bookie_app_services::read_session::TableSort>,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        if max_rows == 0 || max_rows > ROW_CAP {
            return Err(format!("Row limit must be between 1 and {ROW_CAP}"));
        }
        let table = find_table(&self.tables, table)?.clone();
        let editability = self.view_editability(&table);
        let (key, view_access) = match self.table_columns(&table) {
            Ok(columns) => {
                let (columns, access) = view_plan(columns, editability);
                let key: Vec<String> = columns
                    .into_iter()
                    .filter(|column| column.primary_key)
                    .map(|column| column.name)
                    .collect();
                (key, access)
            }
            Err(_) => (Vec::new(), view_plan(Vec::new(), editability).1),
        };
        if sort.is_some() && !self.supports_snapshot_cursor {
            return Err("Server-side table sorting is not available for this connection".into());
        }
        let epoch = self.next_epoch();
        let started = std::time::Instant::now();
        let reader = PageReader {
            sessions: &self.sessions,
            session_id: self.session_id,
            epoch,
            current_epoch: &self.epoch,
        };
        let result = self
            .runtime
            .block_on(read_pages(reader, &table, max_rows, sort, cancellation))?;
        let mut columns: Vec<_> = result.columns.iter().map(column_meta).collect();
        for column in &mut columns {
            column.primary_key = key.contains(&column.name);
        }
        Ok(LiveTable {
            session: self.clone(),
            columns,
            estimated_retained_bytes: result.estimated_row_bytes(),
            result,
            snapshot_consistent: self.supports_snapshot_cursor,
            messages: Default::default(),
            executed_sql: None,
            load_elapsed: started.elapsed(),
            table: Some(table.clone()),
            view_access,
        })
    }

    pub fn load_table_where_controlled(
        self: &Arc<Self>,
        table: &TableInfo,
        max_rows: u64,
        sort: Option<&bookie_app_services::read_session::TableSort>,
        where_sql: &str,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        if where_sql.trim().is_empty() {
            return self.load_table_sorted_controlled(table, max_rows, sort, cancellation);
        }
        if matches!(self.driver_id.as_str(), "mongodb" | "redis") {
            return Err("WHERE filters need a SQL database".into());
        }
        let table = find_table(&self.tables, table)?;
        let (columns, view_access) = self.edit_columns(&table)?;
        let sort = match sort {
            Some(sort) => Some((
                columns
                    .iter()
                    .position(|column| column.name == sort.column)
                    .ok_or("The requested sort column no longer exists")?,
                sort.ascending,
            )),
            None => None,
        };
        let filter = where_filter(where_sql);
        let page = browse_target(&self.driver_id, &table, &columns, &filter)
            .page(0, max_rows.saturating_add(1), sort, None)
            .map_err(|error| error.to_string())?;
        let bookie_app_services::browse_query::PageQuery::Sql(query) = page else {
            return Err("The WHERE filter could not be sent to the database".into());
        };
        let mut loaded = self.run_query_bound(&query.sql, &query.params, max_rows, cancellation)?;
        for column in &mut loaded.columns {
            column.primary_key = columns.iter().any(|info| info.primary_key && info.name == column.name);
        }
        loaded.executed_sql = None;
        loaded.table = Some(table);
        loaded.view_access = view_access;
        Ok(loaded)
    }

    pub fn export_all_tables(
        &self,
        directory: std::path::PathBuf,
        max_rows_per_table: Option<usize>,
        cancellation: &CancellationToken,
    ) -> Result<Vec<bookie_app_services::export_tables::TableResult>, String> {
        let job = bookie_app_services::export_tables::TablesExportJob {
            directory,
            max_rows_per_table,
            timeout_secs: 600,
        };
        self.runtime
            .block_on(
                self.sessions
                    .export_tables_to_folder(self.session_id, self.tables.clone(), job, cancellation),
            )
            .map_err(|error| error.to_string())
    }

    pub fn run_backup(
        &self,
        job: bookie_app_services::database_backup::BackupJob,
        cancellation: &CancellationToken,
    ) -> Result<(), String> {
        run_backup_on(&self.runtime, &self.sessions, self.session_id, job, cancellation)
    }

    pub fn diagram_layout_path(&self, scope: Option<&str>) -> Option<std::path::PathBuf> {
        let identity = self.cache_identity();
        let config = self
            .stamp
            .as_deref()
            .and_then(|stamp| serde_json::from_str::<serde_json::Value>(stamp).ok());
        let target = config.as_ref().map(|config| {
            [
                "driver_id",
                "host",
                "port",
                "database",
                "socket_dir",
                "socket_path",
                "ssh",
                "attached_databases",
            ]
            .map(|field| config.get(field))
        });
        let key = serde_json::to_vec(&(identity, target, scope)).ok()?;
        let id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &key);
        bookie_app_services::config_io::xdg_config_path(&format!("diagram-layouts/{id}.json"))
    }

    pub fn schema_diagram(&self, scope: Option<&str>, cancellation: &CancellationToken) -> Result<SchemaRead, String> {
        let control = bookie_app_services::operation_control::bounded_with(30, cancellation.clone());
        self.runtime
            .block_on(self.sessions.read_schema_diagram(self.session_id, scope, &control))
            .map_err(|error| driver_failure_message(&bookie_app_services::driver_error_presentation::classify(&error)))
    }

    pub fn compare_table_data(
        &self,
        source: &TableInfo,
        target: &TableInfo,
        cancellation: &CancellationToken,
    ) -> Result<bookie_app_services::data_compare::DataCompare, bookie_app_services::data_compare::DataCompareError>
    {
        let control = bookie_app_services::operation_control::bounded_with(30, cancellation.clone());
        self.runtime.block_on(
            self.sessions
                .compare_table_data(self.session_id, source, target, &control),
        )
    }

    pub fn search_values(
        &self,
        term: &str,
        options: SearchOptions,
        cancellation: &CancellationToken,
    ) -> Result<SearchReport, String> {
        let tables = self
            .tables
            .iter()
            .map(|table| TableTarget {
                schema: table.schema.clone(),
                name: table.name.clone(),
            })
            .collect();
        let request = SearchRequest {
            driver_id: &self.driver_id,
            tables,
            term,
            options,
            limits: SearchLimits::default(),
        };
        self.runtime
            .block_on(run_search(&BridgeSource(self), request, cancellation, |_| {}))
            .map_err(|error| format!("{error:?}"))
    }

    pub fn server_info(&self) -> ServerInfo {
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let (version, round_trip) = match self
            .runtime
            .block_on(self.sessions.server_version(self.session_id, &control))
        {
            Ok((version, round_trip)) => (version, Some(round_trip)),
            Err(_) => (None, None),
        };
        let details = self.saved.as_ref().and_then(|(id, _)| {
            self.runtime
                .block_on(bookie_app_services::connection_details::load_connection_details(*id))
                .ok()
                .flatten()
        });
        ServerInfo {
            version,
            round_trip,
            details,
        }
    }

    pub fn table_sizes(&self) -> bookie_app_services::table_sizes::SizeIndex {
        let control = OperationControl::with_timeout(Duration::from_secs(10));
        self.runtime
            .block_on(self.sessions.table_sizes(self.session_id, &control))
            .unwrap_or_default()
    }

    fn next_epoch(&self) -> Epoch {
        Epoch(self.epoch.fetch_add(1, Ordering::Relaxed) + 1)
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        let _inside = self.runtime.enter();
        self.sessions.close(self.session_id);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerInfo {
    pub version: Option<String>,
    pub round_trip: Option<Duration>,
    pub details: Option<bookie_app_services::connection_details::ConnectionDetails>,
}

fn view_plan(
    columns: Vec<ColumnInfo>,
    editability: Result<Option<bookie_core::ViewEditability>, String>,
) -> (Vec<ColumnInfo>, ViewAccess) {
    use bookie_app_services::view_edit::{ViewEditRestriction, apply_view_editability};
    let editability = match editability {
        Ok(Some(editability)) => editability,
        Ok(None) => return (columns, ViewAccess::Editable),
        Err(_) => {
            return (
                columns,
                ViewAccess::ReadOnly("its editability could not be read".into()),
            );
        }
    };
    let plan = apply_view_editability(columns, &editability);
    let access = match plan.restriction {
        None => ViewAccess::Editable,
        Some(restriction @ ViewEditRestriction::InsertUnavailable) => ViewAccess::NoInsert(restriction.insert_reason()),
        Some(ViewEditRestriction::NotUpdatable) => ViewAccess::ReadOnly("it is not automatically updatable".into()),
        Some(ViewEditRestriction::KeyNotExposed) => {
            ViewAccess::ReadOnly("it does not expose every primary-key column of its table".into())
        }
    };
    (plan.columns, access)
}

pub struct LiveTable {
    session: Arc<LiveSession>,
    columns: Vec<ColumnMeta>,
    result: QueryResult,
    estimated_retained_bytes: usize,
    snapshot_consistent: bool,
    pub messages: bookie_core::ServerMessages,
    executed_sql: Option<String>,
    load_elapsed: Duration,
    table: Option<TableInfo>,
    view_access: ViewAccess,
}

impl LiveTable {
    pub fn executed_sql(&self) -> Option<&str> {
        self.executed_sql.as_deref()
    }

    pub fn session(&self) -> &Arc<LiveSession> {
        &self.session
    }

    pub fn table_info(&self) -> Option<&TableInfo> {
        self.table.as_ref()
    }

    pub fn load(origin: &Origin, table: &TableInfo, max_rows: u64) -> Result<Self, String> {
        LiveSession::open(origin)?.load_table(table, max_rows)
    }
}

fn file_identity(path: &std::path::Path) -> (uuid::Uuid, String) {
    let text = path.display().to_string();
    (
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, format!("file://{text}").as_bytes()),
        text,
    )
}

type OpenedParts = (
    ReadSessionStore,
    OpenedSession,
    Option<(uuid::Uuid, String)>,
    bool,
    Option<String>,
);

async fn open_session(origin: &Origin, cancellation: CancellationToken) -> Result<OpenedParts, String> {
    let registry = Arc::new(drivers_registry::default_registry());
    let sessions = ReadSessionStore::new(registry, crate::host_key::database_service())
        .with_subscription_sampler(Arc::new(RegistrySubscriptionSampler));
    let epoch = Epoch(1);
    let current_epoch = AtomicU64::new(epoch.0);
    let control = OperationControl::new(
        cancellation,
        Some(tokio::time::Instant::now() + Duration::from_secs(30)),
    );
    let (command, identity, writable, stamp) = match origin {
        Origin::Saved(connection) => {
            let saved = bridge_saved_connections(&control, epoch, &current_epoch).await?;
            let found = saved
                .iter()
                .find(|saved| saved.id.to_string() == *connection || saved.name == *connection)
                .ok_or_else(|| format!("no saved connection named or numbered {connection}"))?;
            let command = Command::OpenSavedConnection {
                epoch,
                connection_id: found.id,
            };
            let stamp = ReadSessionStore::saved_connection_stamp(found.id).await.ok();
            (command, (found.id, found.name.clone()), !found.read_only, stamp)
        }
        Origin::File(path) => {
            let command = Command::OpenConnectionFile {
                epoch,
                path: path.clone(),
            };
            (command, file_identity(path), false, None)
        }
    };
    let event = dispatch_read(
        DispatchContext::Sessions(&sessions),
        command,
        &control,
        epoch,
        &current_epoch,
    )
    .await?;
    let Event::SessionOpened { outcome, .. } = event else {
        return Err("The command bridge returned an unexpected event".into());
    };
    Ok((sessions, outcome?, Some(identity), writable, stamp))
}

pub struct SaveFailure {
    pub message: String,
    pub outcome_unknown: bool,
}

pub struct WriteSession {
    runtime: tokio::runtime::Runtime,
    sessions: ReadSessionStore,
    session_id: SessionId,
    profile: Option<(uuid::Uuid, String)>,
}

fn refuse_edited_connection(opened_with: Option<&str>, now: &str) -> Result<(), String> {
    match opened_with {
        Some(expected) if expected != now => {
            Err("The connection was edited after this table was opened; reopen the table before saving".into())
        }
        _ => Ok(()),
    }
}

impl WriteSession {
    pub fn open_saved(
        connection_id: uuid::Uuid,
        approvals: tokio::sync::mpsc::UnboundedSender<bookie_app_services::approval_prompt::PendingApproval>,
        opened_with: Option<&str>,
    ) -> Result<WriteSession, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let database = crate::host_key::database_service();
        database.set_approval_sink(bookie_app_services::approval_prompt::reviewable_prompt_sink(approvals));
        let sessions = ReadSessionStore::new(Arc::new(drivers_registry::default_registry()), database);
        let control = OperationControl::with_timeout(Duration::from_secs(30));
        let stamp = runtime
            .block_on(ReadSessionStore::saved_connection_stamp(connection_id))
            .map_err(|error| error.to_string())?;
        refuse_edited_connection(opened_with, &stamp)?;
        let opened = runtime
            .block_on(sessions.open_saved_writable(connection_id, &control))
            .map_err(|error| error.to_string())?;
        Ok(WriteSession {
            runtime,
            sessions,
            session_id: opened.session_id,
            profile: Some((connection_id, stamp)),
        })
    }

    pub fn begin_transaction(&self) -> Result<bookie_app_services::interactive_session::SharedSession, String> {
        let shared = self
            .runtime
            .block_on(self.sessions.open_interactive(self.session_id))
            .map_err(|failure| driver_failure_message(&failure))?;
        if let Err(error) = self.run_in_transaction(&shared, "BEGIN", &CancellationToken::new()) {
            let _ = self
                .runtime
                .block_on(bookie_app_services::interactive_session::close_for_teardown(shared));
            return Err(error);
        }
        Ok(shared)
    }

    pub fn run_in_transaction(
        &self,
        shared: &bookie_app_services::interactive_session::SharedSession,
        sql: &str,
        cancellation: &CancellationToken,
    ) -> Result<QueryResult, String> {
        use bookie_core::Session as _;
        let control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
        self.runtime.block_on(async {
            let mut guard = shared.lock().await;
            let session = guard.as_mut().ok_or("The transaction session is closed")?;
            session
                .query_params_controlled(sql, &[], &control)
                .await
                .map_err(|error| error.to_string())
        })
    }

    pub fn commit_transaction(
        &self,
        shared: &bookie_app_services::interactive_session::SharedSession,
    ) -> Result<(bookie_core::SessionCommitOutcome, Option<String>), bookie_core::DriverError> {
        self.runtime.block_on(async {
            let outcome = bookie_app_services::interactive_session::commit_session(shared).await?;
            let close_error = bookie_app_services::interactive_session::close_for_teardown(shared.clone())
                .await
                .err()
                .map(|error| error.to_string());
            Ok((outcome, close_error))
        })
    }

    pub fn transaction_is_open(&self, shared: &bookie_app_services::interactive_session::SharedSession) -> bool {
        use bookie_core::Session as _;
        self.runtime.block_on(async {
            shared
                .lock()
                .await
                .as_ref()
                .is_some_and(|session| session.is_usable() && session.transaction_open())
        })
    }

    pub fn close_transaction(
        &self,
        shared: &bookie_app_services::interactive_session::SharedSession,
    ) -> Option<String> {
        self.runtime
            .block_on(bookie_app_services::interactive_session::close_for_teardown(
                shared.clone(),
            ))
            .err()
            .map(|error| error.to_string())
    }

    pub fn rollback_transaction(
        &self,
        shared: &bookie_app_services::interactive_session::SharedSession,
    ) -> Result<(), String> {
        self.runtime
            .block_on(bookie_app_services::interactive_session::rollback_and_close(shared))
            .map_err(|error| error.to_string())
    }

    pub fn is_current(&self) -> Result<bool, String> {
        let Some((connection_id, stamp)) = &self.profile else {
            return Ok(true);
        };
        let now = self
            .runtime
            .block_on(ReadSessionStore::saved_connection_stamp(*connection_id))
            .map_err(|error| error.to_string())?;
        Ok(now == *stamp)
    }

    #[doc(hidden)]
    pub fn from_parts(
        runtime: tokio::runtime::Runtime,
        sessions: ReadSessionStore,
        session_id: SessionId,
    ) -> WriteSession {
        WriteSession {
            runtime,
            sessions,
            session_id,
            profile: None,
        }
    }

    pub fn run_backup(
        &self,
        job: bookie_app_services::database_backup::BackupJob,
        cancellation: &CancellationToken,
    ) -> Result<(), String> {
        run_backup_on(&self.runtime, &self.sessions, self.session_id, job, cancellation)
    }

    pub fn apply(
        &self,
        statements: &[(String, Vec<bookie_core::Value>)],
        expect_one: &[usize],
        cancellation: &CancellationToken,
    ) -> Result<Vec<u64>, SaveFailure> {
        let control = OperationControl::new(
            cancellation.clone(),
            Some(tokio::time::Instant::now() + Duration::from_secs(60)),
        );
        self.runtime
            .block_on(
                self.sessions
                    .apply_changes(self.session_id, statements, expect_one, &control),
            )
            .map_err(|failure| SaveFailure {
                message: driver_failure_message(&failure),
                outcome_unknown: matches!(
                    failure,
                    bookie_app_services::driver_error_presentation::DriverFailurePresentation::OperationOutcomeUnknown(
                        _
                    ) | bookie_app_services::driver_error_presentation::DriverFailurePresentation::TimedOut
                ),
            })
    }
}

struct RegistrySubscriptionSampler;

impl SubscriptionSampler for RegistrySubscriptionSampler {
    fn validate(&self, driver_id: &str, request: &SubscriptionRequest) -> Result<(), bookie_core::DriverError> {
        match driver_id {
            "redis" => {
                drivers_registry::validate_pubsub_request(&request.patterns, request.max_messages, request.window)
            }
            "postgres" => {
                drivers_registry::check_notification_request(&request.patterns, request.max_messages, request.window)
            }
            other => Err(bookie_core::DriverError::Unsupported(format!(
                "{other} has no live messages to listen to"
            ))),
        }
    }

    fn follow(
        &self,
        driver_id: &str,
        options: bookie_core::ConnectOptions,
        channels: Vec<String>,
        cancel: CancellationToken,
        mut on_message: Box<dyn FnMut(bookie_app_services::read_session_store::SubscriptionMessage) + Send>,
    ) -> Pin<Box<dyn Future<Output = Result<u64, bookie_core::DriverError>> + Send + '_>> {
        let driver_id = driver_id.to_string();
        Box::pin(async move {
            match driver_id.as_str() {
                "postgres" => {
                    drivers_registry::follow_notifications(options, &channels, &cancel, move |message| {
                        on_message(crate::pubsub_view::subscription_message(
                            message.channel,
                            None,
                            message.payload,
                            message.payload_bytes,
                        ));
                    })
                    .await
                }
                "redis" => {
                    drivers_registry::follow_pubsub(options, &channels, &cancel, move |message| {
                        on_message(crate::pubsub_view::subscription_message(
                            message.channel,
                            message.pattern,
                            message.payload,
                            message.payload_bytes,
                        ));
                    })
                    .await
                }
                other => Err(bookie_core::DriverError::Unsupported(format!(
                    "{other} has no live messages to follow"
                ))),
            }
        })
    }

    fn sample(
        &self,
        driver_id: &str,
        options: bookie_core::ConnectOptions,
        request: SubscriptionRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SubscriptionSample, bookie_core::DriverError>> + Send + '_>> {
        let postgres = driver_id == "postgres";
        Box::pin(async move {
            if postgres {
                let sample = drivers_registry::sample_notifications(
                    options,
                    &request.patterns,
                    request.max_messages,
                    request.window,
                )
                .await?;
                return Ok(SubscriptionSample {
                    messages: sample
                        .messages
                        .into_iter()
                        .map(|message| {
                            crate::pubsub_view::subscription_message(
                                message.channel,
                                None,
                                message.payload,
                                message.payload_bytes,
                            )
                        })
                        .collect(),
                    reached_limit: sample.reached_limit,
                });
            }
            let sample =
                drivers_registry::sample_pubsub(options, &request.patterns, request.max_messages, request.window)
                    .await?;
            Ok(SubscriptionSample {
                messages: sample
                    .messages
                    .into_iter()
                    .map(|message| {
                        crate::pubsub_view::subscription_message(
                            message.channel,
                            message.pattern,
                            message.payload,
                            message.payload_bytes,
                        )
                    })
                    .collect(),
                reached_limit: sample.reached_limit,
            })
        })
    }
}

#[derive(Clone, Copy)]
struct PageReader<'a> {
    sessions: &'a ReadSessionStore,
    session_id: SessionId,
    epoch: Epoch,
    current_epoch: &'a AtomicU64,
}

async fn read_pages(
    reader: PageReader<'_>,
    table: &TableInfo,
    max_rows: u64,
    sort: Option<&bookie_app_services::read_session::TableSort>,
    cancellation: &CancellationToken,
) -> Result<QueryResult, String> {
    let open_control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
    let mut cursor = reader
        .sessions
        .open_table_cursor_sorted(reader.session_id, table, sort, &open_control)
        .await
        .map_err(|failure| driver_failure_message(&failure))?;
    let result = collect_pages(reader, table, max_rows, cancellation, &mut cursor).await;
    let close_result = if let Some(cursor) = cursor {
        cursor.close().await.map_err(|failure| driver_failure_message(&failure))
    } else {
        Ok(())
    };
    match result {
        Ok(pages) => {
            close_result?;
            Ok(pages)
        }
        Err(error) => {
            let _ = close_result;
            Err(error)
        }
    }
}

async fn collect_pages(
    reader: PageReader<'_>,
    table: &TableInfo,
    max_rows: u64,
    cancellation: &CancellationToken,
    cursor: &mut Option<bookie_app_services::read_session::PostgresTableCursor>,
) -> Result<QueryResult, String> {
    let mut infos: Vec<ColumnInfo> = Vec::new();
    let mut rows = QueryRows::new();
    let mut truncated = false;
    let mut budget = QueryResultBudget::default();
    while (rows.len() as u64) < max_rows {
        let limit = PAGE_ROWS.min(max_rows - rows.len() as u64);
        let (mut page, has_next_page) =
            next_result_page(reader, table, rows.len() as u64, limit, cancellation, cursor).await?;
        cap_page_rows(&mut page, limit);
        if infos.is_empty() {
            infos.clone_from(&page.columns);
        }
        if cancellation.is_cancelled() {
            return Err("Canceled the operation".into());
        }
        let incomplete = bound_page(&mut page, &mut budget);
        let count = page.rows.len() as u64;
        for row in page.rows.into_rows() {
            rows.push(row);
        }
        if cancellation.is_cancelled() {
            return Err("Canceled the operation".into());
        }
        if incomplete {
            truncated = true;
            break;
        }
        if count == 0 || (cursor.is_some() && !has_next_page) || (cursor.is_none() && count < limit) {
            break;
        }
        truncated = (rows.len() as u64) >= max_rows;
    }
    Ok(QueryResult {
        columns: infos,
        rows,
        truncated,
    })
}

async fn next_result_page(
    reader: PageReader<'_>,
    table: &TableInfo,
    offset: u64,
    limit: u64,
    cancellation: &CancellationToken,
    cursor: &mut Option<bookie_app_services::read_session::PostgresTableCursor>,
) -> Result<(QueryResult, bool), String> {
    let control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
    match cursor {
        Some(cursor) => {
            let page = cursor
                .next_page(&control)
                .await
                .map_err(|failure| driver_failure_message(&failure))?;
            Ok((page.result, page.has_next_page))
        }
        None => Ok((bridge_read_page(reader, table, offset, limit, &control).await?, false)),
    }
}

async fn query_pages(
    sessions: &ReadSessionStore,
    session_id: SessionId,
    (sql, params): (&str, &[bookie_core::Value]),
    max_rows: u64,
    cancellation: &CancellationToken,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<(QueryResult, bookie_core::ServerMessages), QueryFailure> {
    let control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
    let mut batch = bridge_query_detailed(sessions, session_id, (sql, params), &control, epoch, current_epoch).await?;
    let messages = std::mem::take(&mut batch.messages);
    Ok((bounded_result(batch.into_first(), max_rows, cancellation)?, messages))
}

async fn dictionary_pages(
    sessions: &ReadSessionStore,
    session_id: SessionId,
    capabilities: bookie_core::DriverCapabilities,
    cancellation: &CancellationToken,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<QueryResult, String> {
    let control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
    let event = dispatch_read(
        DispatchContext::Sessions(sessions),
        Command::ReadDataDictionary {
            epoch,
            session_id,
            capabilities,
        },
        &control,
        epoch,
        current_epoch,
    )
    .await?;
    let Event::DataDictionaryRead { outcome, .. } = event else {
        return Err("The command bridge returned an unexpected event".into());
    };
    let result = outcome.map_err(|failure| driver_failure_message(&failure))?;
    bounded_result(result, ROW_CAP, cancellation)
}

fn bounded_result(
    mut result: QueryResult,
    max_rows: u64,
    cancellation: &CancellationToken,
) -> Result<QueryResult, String> {
    if cancellation.is_cancelled() {
        return Err("Canceled the operation".into());
    }
    let cap = usize::try_from(max_rows).unwrap_or(usize::MAX);
    result.truncated = bound_query_result(&mut result, cap, &mut QueryResultBudget::default());
    if cancellation.is_cancelled() {
        return Err("Canceled the operation".into());
    }
    Ok(result)
}

async fn bridge_query(
    sessions: &ReadSessionStore,
    session_id: SessionId,
    (sql, params): (&str, &[bookie_core::Value]),
    control: &OperationControl,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<QueryResult, String> {
    bridge_query_detailed(sessions, session_id, (sql, params), control, epoch, current_epoch)
        .await
        .map(|batch| batch.into_first())
        .map_err(|error| error.message)
}

async fn bridge_query_detailed(
    sessions: &ReadSessionStore,
    session_id: SessionId,
    (sql, params): (&str, &[bookie_core::Value]),
    control: &OperationControl,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<bookie_core::QueryResultBatch, QueryFailure> {
    let event = dispatch_read(
        DispatchContext::Sessions(sessions),
        Command::RunQuery {
            epoch,
            session_id,
            sql: sql.to_owned(),
            params: params.to_vec(),
        },
        control,
        epoch,
        current_epoch,
    )
    .await?;
    let Event::QueryCompleted { outcome, .. } = event else {
        return Err("The command bridge returned an unexpected event".into());
    };
    outcome.map_err(Into::into)
}

async fn bridge_read_page(
    reader: PageReader<'_>,
    table: &TableInfo,
    offset: u64,
    limit: u64,
    control: &OperationControl,
) -> Result<QueryResult, String> {
    let event = dispatch_read(
        DispatchContext::Sessions(reader.sessions),
        Command::ReadPage {
            epoch: reader.epoch,
            session_id: reader.session_id,
            table: table.clone(),
            offset,
            limit,
        },
        control,
        reader.epoch,
        reader.current_epoch,
    )
    .await?;
    let Event::PageRead { outcome, .. } = event else {
        return Err("The command bridge returned an unexpected event".into());
    };
    outcome.map_err(|failure| driver_failure_message(&failure))
}

async fn dispatch_read(
    context: DispatchContext<'_>,
    command: Command,
    control: &OperationControl,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<Event, String> {
    if current_epoch.load(Ordering::Relaxed) != epoch.0 {
        return Err("The request was superseded".into());
    }
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let sink = ChannelEventSink::new(sender);
    dispatch(context, command, control, &sink)
        .await
        .map_err(|error| format!("{error:?}"))?;
    let event = receiver.try_recv().map_err(|error| format!("{error:?}"))?;
    keep_current_event(context, event, Epoch(current_epoch.load(Ordering::Relaxed)))
}

async fn dispatch_once(
    context: DispatchContext<'_>,
    command: Command,
    control: &OperationControl,
) -> Result<Event, String> {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let sink = ChannelEventSink::new(sender);
    dispatch(context, command, control, &sink)
        .await
        .map_err(|error| format!("{error:?}"))?;
    receiver.try_recv().map_err(|error| format!("{error:?}"))
}

fn keep_current_event(context: DispatchContext<'_>, event: Event, current: Epoch) -> Result<Event, String> {
    if event.epoch() == current {
        return Ok(event);
    }
    if let (
        DispatchContext::Sessions(sessions),
        Event::SessionOpened {
            outcome: Ok(opened), ..
        },
    ) = (context, event)
    {
        sessions.close(opened.session_id);
    }
    Err("The request was superseded".into())
}

async fn bridge_saved_connections(
    control: &OperationControl,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<Vec<ConnectionSummary>, String> {
    let event = dispatch_read(
        DispatchContext::SavedConnections,
        Command::ListSavedConnections { epoch },
        control,
        epoch,
        current_epoch,
    )
    .await?;
    let Event::SavedConnectionsListed { outcome, .. } = event else {
        return Err("The command bridge returned an unexpected event".into());
    };
    outcome
}

fn bound_query_result(result: &mut QueryResult, cap: usize, budget: &mut QueryResultBudget) -> bool {
    let mut truncated = result.truncated || result.rows.len() > cap;
    result.rows.truncate(cap);
    let admitted = result.rows.iter().take_while(|row| budget.admit(row)).count();
    if admitted < result.rows.len() {
        result.rows.truncate(admitted);
        truncated = true;
    }
    truncated
}

fn cap_page_rows(page: &mut QueryResult, cap: u64) -> bool {
    if page.rows.len() as u64 <= cap {
        return false;
    }
    page.rows.truncate(usize::try_from(cap).unwrap_or(usize::MAX));
    page.truncated = true;
    true
}

fn bound_page(page: &mut QueryResult, budget: &mut QueryResultBudget) -> bool {
    let admitted = page.rows.iter().take_while(|row| budget.admit(row)).count();
    let incomplete = page.truncated || admitted < page.rows.len();
    page.rows.truncate(admitted);
    page.truncated = incomplete;
    incomplete
}

fn where_filter(where_sql: &str) -> bookie_core::FilterSet {
    bookie_core::FilterSet {
        extra_sql: Some(where_sql.to_owned()),
        ..bookie_core::FilterSet::default()
    }
}

fn browse_target<'a>(
    driver_id: &'a str,
    table: &'a TableInfo,
    columns: &'a [ColumnInfo],
    filter: &'a bookie_core::FilterSet,
) -> bookie_app_services::browse_query::BrowseTarget<'a> {
    bookie_app_services::browse_query::BrowseTarget {
        driver_id,
        schema: table.schema.as_deref(),
        table: &table.name,
        columns,
        filter,
        hidden_columns: None,
        long_text_preview: false,
        supports_reverse_keyset: false,
    }
}

fn find_table(tables: &[TableInfo], wanted: &TableInfo) -> Result<TableInfo, String> {
    tables
        .iter()
        .find(|table| *table == wanted)
        .cloned()
        .ok_or_else(|| format!("no table named {}", table_label(wanted)))
}

const MIN_COLUMN_WIDTH: f32 = 80.0;
const MAX_COLUMN_WIDTH: f32 = 360.0;
const TEXT_COLUMN_WIDTH: f32 = 160.0;

fn column_meta(info: &ColumnInfo) -> ColumnMeta {
    ColumnMeta {
        name: info.name.clone(),
        width: column_width(&info.name, &info.data_type),
        numeric: numeric_type(&info.data_type),
        data_type: info.data_type.clone(),
        primary_key: info.primary_key,
    }
}

fn column_width(name: &str, data_type: &str) -> f32 {
    let header = name.chars().count() as f32 * 9.0 + 48.0;
    let lower = data_type.to_ascii_lowercase();
    let value = if lower.contains("timestamp") {
        272.0
    } else if lower.contains("uuid") {
        300.0
    } else if lower.starts_with("numeric")
        || lower.starts_with("decimal")
        || lower.starts_with("number")
        || lower.contains("money")
    {
        200.0
    } else if lower.contains("bool") {
        88.0
    } else if lower.contains("json") {
        200.0
    } else if lower == "date" {
        120.0
    } else if lower.starts_with("time") {
        130.0
    } else if lower.contains("bigint") || lower.contains("int8") || lower.contains("bigserial") {
        200.0
    } else if lower.contains("smallint") || lower.contains("int2") || lower.contains("smallserial") {
        96.0
    } else if matches!(lower.as_str(), "int" | "integer" | "int4") || lower.contains("serial") {
        120.0
    } else {
        TEXT_COLUMN_WIDTH
    };
    header.max(value).clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH)
}

#[cfg(test)]
mod column_meta_tests {
    use super::*;

    #[test]
    fn numeric_columns_have_room_for_exact_integer_values() {
        let column = column_meta(&ColumnInfo {
            name: "value".into(),
            data_type: "BIGINT".into(),
            nullable: false,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        });

        assert!(column.width >= 200.0, "{}", column.width);
    }

    #[test]
    fn numeric_decimal_and_money_columns_keep_the_wide_default() {
        for data_type in ["numeric(20,6)", "decimal(20,6)", "money"] {
            assert!(column_width("amount", data_type) >= 200.0, "{data_type}");
        }
    }

    #[test]
    fn a_column_is_as_wide_as_its_type_needs_not_its_name() {
        assert_eq!(column_width("id", "integer"), 120.0);
        assert!(column_width("id", "smallint") < column_width("id", "integer"));
        assert!(column_width("id", "boolean") <= 100.0);
        assert!(column_width("created_at", "timestamp with time zone") >= 272.0);
        assert!(column_width("created_at", "timestamp without time zone") >= 272.0);
        assert!(column_width("ref", "uuid") >= 300.0);
        assert_eq!(column_width("name", "text"), TEXT_COLUMN_WIDTH);
        assert_eq!(column_width("name", ""), TEXT_COLUMN_WIDTH);
    }

    #[test]
    fn a_long_column_name_is_capped() {
        assert_eq!(column_width(&"x".repeat(80), "text"), MAX_COLUMN_WIDTH);
    }

    #[test]
    fn column_meta_carries_the_type_and_the_primary_key_flag() {
        let column = column_meta(&ColumnInfo {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            primary_key: true,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        });
        assert_eq!((column.data_type.as_str(), column.primary_key), ("integer", true));
    }
}

fn numeric_type(data_type: &str) -> bool {
    let lower = data_type.to_ascii_lowercase();
    [
        "int", "serial", "numeric", "decimal", "number", "float", "double", "real", "money",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

impl ResultSource for LiveTable {
    fn estimated_retained_bytes(&self) -> usize {
        self.estimated_retained_bytes
    }

    fn load_elapsed(&self) -> Option<Duration> {
        Some(self.load_elapsed)
    }

    fn executed_sql(&self) -> Option<&str> {
        self.executed_sql.as_deref()
    }

    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }

    fn editable(&self) -> bool {
        self.table.is_some() && self.session.writable.is_some() && self.view_access.edit_refusal().is_none()
    }

    fn view_access(&self) -> Option<&ViewAccess> {
        Some(&self.view_access)
    }

    fn is_partial(&self) -> bool {
        self.result.truncated
    }

    fn snapshot_consistent(&self) -> bool {
        self.snapshot_consistent
    }

    fn origin(&self) -> String {
        let cap = if self.result.truncated {
            format!(" · {} rows loaded; result may be incomplete", self.result.rows.len())
        } else {
            String::new()
        };
        let table = self
            .table
            .as_ref()
            .map(table_label)
            .unwrap_or_else(|| "query result".into());
        let consistency = self.table.as_ref().map_or("", |_| {
            if self.snapshot_consistent {
                " · snapshot for this load"
            } else {
                " · page order may change; rows may be skipped or repeated"
            }
        });
        let access = if self.editable() { "editable" } else { "read-only" };
        format!("{} · {} · {access}{consistency}{cap}", self.session.driver_id, table)
    }

    fn total(&self) -> u64 {
        self.result.rows.len() as u64
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.result.rows.get(index))
            .map(<[Cell]>::to_vec)
            .unwrap_or_default()
    }

    fn row_ref(&self, index: u64) -> Option<&[Cell]> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.result.rows.get(index))
    }

    fn find(&self, query: &str, cap: usize, cancel: &CancellationToken) -> Result<FindResult, &'static str> {
        find_buffered_rows(&self.result.rows, self.columns.len(), query, cap, cancel)
    }

    fn filter(&self, predicate: &Predicate, cancel: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        filter_buffered_rows(&self.result, predicate, cancel)
    }

    fn compare(&self, _keyed: bool, _cancel: &CancellationToken) -> Result<Diff, &'static str> {
        Err("compare is not available for a live table")
    }
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;

struct BridgeSource<'a>(&'a LiveSession);

impl SearchSource for BridgeSource<'_> {
    async fn columns(&self, table: &TableTarget, control: &OperationControl) -> Result<Vec<ColumnInfo>, DriverError> {
        let session = self.0;
        let epoch = session.next_epoch();
        let table = TableInfo {
            schema: table.schema.clone(),
            name: table.name.clone(),
        };
        let event = dispatch_read(
            DispatchContext::Sessions(&session.sessions),
            Command::ReadColumns {
                epoch,
                session_id: session.session_id,
                table,
            },
            control,
            epoch,
            &session.epoch,
        )
        .await
        .map_err(search_error)?;
        let Event::ColumnsRead { outcome, .. } = event else {
            return Err(search_error("The command bridge returned an unexpected event".into()));
        };
        outcome.map_err(|failure| search_error(driver_failure_message(&failure)))
    }

    async fn query(
        &self,
        sql: &str,
        params: &[bookie_core::Value],
        control: &OperationControl,
    ) -> Result<QueryResult, DriverError> {
        let session = self.0;
        let epoch = session.next_epoch();
        bridge_query(
            &session.sessions,
            session.session_id,
            (sql, params),
            control,
            epoch,
            &session.epoch,
        )
        .await
        .map_err(search_error)
    }
}

fn search_error(message: String) -> DriverError {
    DriverError::Query {
        message,
        sqlstate: None,
        position: None,
        hint: None,
    }
}

fn backup_failure_text(error: &DriverError) -> String {
    match error {
        DriverError::Cancelled => "Canceled; nothing was changed".into(),
        error => bookie_app_services::database_backup::backup_error_text(error),
    }
}

fn run_backup_on(
    runtime: &tokio::runtime::Runtime,
    sessions: &ReadSessionStore,
    session_id: SessionId,
    job: bookie_app_services::database_backup::BackupJob,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .ok_or("The private runtime directory is not available")?;
    let control = OperationControl::new(cancellation.clone(), None);
    runtime
        .block_on(sessions.run_backup_job(session_id, job, &runtime_dir, &control))
        .map_err(|error| backup_failure_text(&error))
}
