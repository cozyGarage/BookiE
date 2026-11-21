use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bookie_app_services::command_bridge::{ChannelEventSink, Command, DispatchContext, Epoch, Event, dispatch};
use bookie_app_services::database_service::{ConnectionOverview, DatabaseService};
use bookie_app_services::driver_failure_message::driver_failure_message;
use bookie_app_services::read_session::ConnectionSummary;
use bookie_app_services::read_session_store::{
    OpenedSession, ReadSessionStore, SessionId, SubscriptionRequest, SubscriptionSample, SubscriptionSampler,
};
use bookie_app_services::er_diagram::{SchemaDiagram, TableRead, read::MAX_TABLES as MAX_DIAGRAM_TABLES};
use bookie_app_services::value_search::{
    SearchLimits, SearchOptions, SearchReport, SearchRequest, SearchSource, TableTarget, run_search,
};
use bookie_core::{ColumnInfo, DriverError, OperationControl, QueryResult, QueryResultBudget, QueryRows, TableInfo};
use bookie_results::result_scan::ScanSource;
use tokio_util::sync::CancellationToken;

use crate::model::{Cell, ColumnMeta, Diff, FindResult, PAGE_ROWS, Predicate};
use crate::shell::table_label;
use crate::source::{ResultSource, filter_in, find_in};

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
}

impl LiveSession {
    pub fn supports_table_row_count(&self) -> bool {
        matches!(
            self.driver_id.as_str(),
            "cassandra" | "clickhouse" | "duckdb" | "mssql" | "mysql" | "oracle" | "postgres" | "sqlite"
        )
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
        let (sessions, opened, saved, writable, stamp) = runtime.block_on(open_session(origin, cancellation.clone()))?;
        let writable = saved.as_ref().filter(|_| writable).map(|(id, _)| *id);
        Ok(Arc::new(LiveSession {
            driver_id: opened.driver_id.clone(),
            runtime,
            sessions,
            session_id: opened.session_id,
            epoch: AtomicU64::new(0),
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
        let sessions = ReadSessionStore::new(registry, Arc::new(DatabaseService::new()));
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().build() else {
            return self.clone();
        };
        Arc::new(LiveSession {
            runtime,
            sessions,
            session_id: self.session_id,
            epoch: AtomicU64::new(0),
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
        if max_rows == 0 || max_rows > ROW_CAP {
            return Err(format!("Row limit must be between 1 and {ROW_CAP}."));
        }
        let epoch = self.next_epoch();
        let started = std::time::Instant::now();
        let (columns, rows, scan, truncated) = self.runtime.block_on(query_pages(
            &self.sessions,
            self.session_id,
            (sql, params),
            max_rows,
            cancellation,
            epoch,
            &self.epoch,
        ))?;
        Ok(LiveTable {
            session: self.clone(),
            columns,
            rows,
            scan,
            truncated,
            snapshot_consistent: false,
            executed_sql: Some(sql.to_owned()),
            load_elapsed: started.elapsed(),
            table: None,
        })
    }

    pub fn data_dictionary_controlled(
        self: &Arc<Self>,
        capabilities: bookie_core::DriverCapabilities,
        cancellation: &CancellationToken,
    ) -> Result<LiveTable, String> {
        let epoch = self.next_epoch();
        let started = std::time::Instant::now();
        let (columns, rows, scan, truncated) = self.runtime.block_on(dictionary_pages(
            &self.sessions,
            self.session_id,
            capabilities,
            cancellation,
            epoch,
            &self.epoch,
        ))?;
        Ok(LiveTable {
            session: self.clone(),
            columns,
            rows,
            scan,
            truncated,
            snapshot_consistent: false,
            executed_sql: None,
            load_elapsed: started.elapsed(),
            table: None,
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
            return Err("The command bridge returned an unexpected event.".into());
        };
        Ok(outcome)
    }

    pub fn count_table_rows(&self, table: &TableInfo, cancellation: &CancellationToken) -> Result<u64, String> {
        if !self.supports_table_row_count() {
            return Err("Exact row counts are not available for this database type.".into());
        }
        let sql = table_count_sql(&self.driver_id, table);
        let epoch = Epoch(self.epoch.load(Ordering::Relaxed));
        let control = OperationControl::new(
            cancellation.clone(),
            Some(tokio::time::Instant::now() + Duration::from_secs(60)),
        );
        let result = self.runtime.block_on(bridge_query(
            &self.sessions,
            self.session_id,
            (&sql, &[]),
            &control,
            epoch,
            &self.epoch,
        ))?;
        let Some(row) = result.rows.first().filter(|row| row.len() == 1) else {
            return Err("The database did not return one exact row count.".into());
        };
        if result.truncated || result.rows.len() != 1 {
            return Err("The database did not return one exact row count.".into());
        }
        match &row[0] {
            bookie_core::Value::Int(count) => u64::try_from(*count).map_err(|_| "The row count was negative.".into()),
            bookie_core::Value::Decimal(count) if count.scale() == 0 => {
                u64::try_from(count.mantissa()).map_err(|_| "The row count is outside the supported range.".into())
            }
            bookie_core::Value::Text(count) => count
                .parse::<u64>()
                .map_err(|_| "The database returned an invalid row count.".into()),
            _ => Err("The database returned an unsupported row count value.".into()),
        }
    }

    pub fn table_columns(&self, table: &TableInfo) -> Result<Vec<ColumnInfo>, String> {
        let epoch = self.next_epoch();
        let control = OperationControl::with_timeout(Duration::from_secs(10));
        let event = self.runtime.block_on(dispatch_read(
            DispatchContext::Sessions(&self.sessions),
            Command::ReadColumns {
                epoch,
                session_id: self.session_id,
                table: table.clone(),
            },
            &control,
            epoch,
            &self.epoch,
        ))?;
        let Event::ColumnsRead { outcome, .. } = event else {
            return Err("The command bridge returned an unexpected event.".into());
        };
        outcome.map_err(|failure| driver_failure_message(&failure))
    }

    pub fn table_structure(
        &self,
        table: &TableInfo,
    ) -> Result<bookie_app_services::read_session::TableStructure, String> {
        let capabilities = drivers_registry::default_registry()
            .get(&self.driver_id)
            .map(|driver| driver.capabilities())
            .unwrap_or_default();
        let epoch = self.next_epoch();
        let control = OperationControl::with_timeout(Duration::from_secs(10));
        let event = self.runtime.block_on(dispatch_read(
            DispatchContext::Sessions(&self.sessions),
            Command::ReadStructure {
                epoch,
                session_id: self.session_id,
                table: table.clone(),
                capabilities,
            },
            &control,
            epoch,
            &self.epoch,
        ))?;
        let Event::StructureRead { outcome, .. } = event else {
            return Err("The command bridge returned an unexpected event.".into());
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
        let epoch = self.next_epoch();
        let control = OperationControl::with_timeout(Duration::from_secs(10));
        let event = self.runtime.block_on(dispatch_read(
            DispatchContext::Sessions(&self.sessions),
            Command::ReadCatalog {
                epoch,
                session_id: self.session_id,
                kind,
            },
            &control,
            epoch,
            &self.epoch,
        ))?;
        let Event::CatalogRead { outcome, .. } = event else {
            return Err("The command bridge returned an unexpected event.".into());
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
            .block_on(
                self.sessions
                    .follow_subscription(self.session_id, vec![channel.to_string()], &control, on_message),
            )
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
            return Err("The command bridge returned an unexpected event.".into());
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
            return Err(format!("Row limit must be between 1 and {ROW_CAP}."));
        }
        let table = find_table(&self.tables, table)?.clone();
        let key: Vec<String> = self
            .table_columns(&table)
            .map(|columns| {
                columns
                    .into_iter()
                    .filter(|column| column.primary_key)
                    .map(|column| column.name)
                    .collect()
            })
            .unwrap_or_default();
        if sort.is_some() && !self.supports_snapshot_cursor {
            return Err("Server-side table sorting is not available for this connection.".into());
        }
        let epoch = self.next_epoch();
        let started = std::time::Instant::now();
        let (mut columns, rows, scan, truncated) = self.runtime.block_on(read_pages(
            &self.sessions,
            self.session_id,
            &table,
            max_rows,
            sort,
            cancellation,
            epoch,
            &self.epoch,
        ))?;
        for column in &mut columns {
            column.primary_key = key.contains(&column.name);
        }
        Ok(LiveTable {
            session: self.clone(),
            columns,
            rows,
            scan,
            truncated,
            snapshot_consistent: self.supports_snapshot_cursor,
            executed_sql: None,
            load_elapsed: started.elapsed(),
            table: Some(table.clone()),
        })
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
        self.runtime.block_on(self.sessions.export_tables_to_folder(
            self.session_id,
            self.tables.clone(),
            job,
            cancellation,
        ))
    }

    pub fn run_backup(
        &self,
        job: bookie_app_services::database_backup::BackupJob,
        cancellation: &CancellationToken,
    ) -> Result<(), String> {
        run_backup_on(&self.runtime, &self.sessions, self.session_id, job, cancellation)
    }

    pub fn schema_diagram(&self, cancellation: &CancellationToken) -> Result<SchemaDiagram, String> {
        let supports_keys = drivers_registry::default_registry()
            .get(&self.driver_id)
            .is_some_and(|driver| driver.capabilities().supports_foreign_key_metadata);
        let mut tables: Vec<&TableInfo> = self.tables.iter().collect();
        tables.sort_unstable_by(|a, b| a.name.cmp(&b.name).then_with(|| a.schema.cmp(&b.schema)));
        tables.truncate(MAX_DIAGRAM_TABLES);
        let mut reads = Vec::new();
        for table in tables {
            if cancellation.is_cancelled() {
                return Err("The export was cancelled.".into());
            }
            let foreign_keys = if supports_keys {
                self.table_structure(table)?.foreign_keys
            } else {
                Vec::new()
            };
            reads.push(TableRead {
                schema: table.schema.clone(),
                name: table.name.clone(),
                columns: self.table_columns(table)?,
                foreign_keys,
            });
        }
        Ok(SchemaDiagram::build(reads))
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

pub struct LiveTable {
    session: Arc<LiveSession>,
    columns: Vec<ColumnMeta>,
    rows: QueryRows,
    scan: ScanSource,
    pub truncated: bool,
    snapshot_consistent: bool,
    executed_sql: Option<String>,
    load_elapsed: Duration,
    table: Option<TableInfo>,
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

type OpenedParts = (ReadSessionStore, OpenedSession, Option<(uuid::Uuid, String)>, bool, Option<String>);

async fn open_session(
    origin: &Origin,
    cancellation: CancellationToken,
) -> Result<OpenedParts, String> {
    let registry = Arc::new(drivers_registry::default_registry());
    let sessions = ReadSessionStore::new(registry, Arc::new(DatabaseService::new()))
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
        return Err("The command bridge returned an unexpected event.".into());
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
        let database = Arc::new(DatabaseService::new());
        database.set_approval_sink(bookie_app_services::approval_prompt::reviewable_prompt_sink(approvals));
        let sessions = ReadSessionStore::new(Arc::new(drivers_registry::default_registry()), database);
        let control = OperationControl::with_timeout(Duration::from_secs(30));
        let stamp = runtime.block_on(ReadSessionStore::saved_connection_stamp(connection_id))?;
        refuse_edited_connection(opened_with, &stamp)?;
        let opened = runtime.block_on(sessions.open_saved_writable(connection_id, &control))?;
        Ok(WriteSession {
            runtime,
            sessions,
            session_id: opened.session_id,
            profile: Some((connection_id, stamp)),
        })
    }

    pub fn is_current(&self) -> Result<bool, String> {
        let Some((connection_id, stamp)) = &self.profile else {
            return Ok(true);
        };
        let now = self
            .runtime
            .block_on(ReadSessionStore::saved_connection_stamp(*connection_id))?;
        Ok(now == *stamp)
    }

    #[doc(hidden)]
    pub fn from_parts(runtime: tokio::runtime::Runtime, sessions: ReadSessionStore, session_id: SessionId) -> WriteSession {
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
                    bookie_app_services::driver_error_presentation::DriverFailurePresentation::OperationOutcomeUnknown(_)
                        | bookie_app_services::driver_error_presentation::DriverFailurePresentation::TimedOut
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

type Pages = (Vec<ColumnMeta>, QueryRows, ScanSource, bool);

async fn read_pages(
    sessions: &ReadSessionStore,
    session_id: SessionId,
    table: &TableInfo,
    max_rows: u64,
    sort: Option<&bookie_app_services::read_session::TableSort>,
    cancellation: &CancellationToken,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<Pages, String> {
    let open_control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
    let mut cursor = sessions
        .open_table_cursor_sorted(session_id, table, sort, &open_control)
        .await
        .map_err(|failure| driver_failure_message(&failure))?;
    let result = collect_pages(
        sessions,
        session_id,
        table,
        max_rows,
        cancellation,
        &mut cursor,
        epoch,
        current_epoch,
    )
    .await;
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
    sessions: &ReadSessionStore,
    session_id: SessionId,
    table: &TableInfo,
    max_rows: u64,
    cancellation: &CancellationToken,
    cursor: &mut Option<bookie_app_services::read_session::PostgresTableCursor>,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<Pages, String> {
    let spool = crate::source::spool_manager()
        .create()
        .map_err(|error| error.to_string())?;
    let mut infos: Vec<ColumnInfo> = Vec::new();
    let mut rows = QueryRows::new();
    let mut truncated = false;
    let mut page_index = 0;
    let mut budget = QueryResultBudget::default();
    while (rows.len() as u64) < max_rows {
        let limit = PAGE_ROWS.min(max_rows - rows.len() as u64);
        let (mut page, has_next_page) = next_result_page(
            sessions,
            session_id,
            table,
            rows.len() as u64,
            limit,
            cancellation,
            cursor,
            epoch,
            current_epoch,
        )
        .await?;
        cap_page_rows(&mut page, limit);
        if infos.is_empty() {
            infos.clone_from(&page.columns);
        }
        if cancellation.is_cancelled() {
            return Err("The operation was cancelled.".into());
        }
        let incomplete = bound_page(&mut page, &mut budget);
        let count = page.rows.len() as u64;
        for row in page.rows.iter() {
            rows.push(row.to_vec());
        }
        spool
            .append_page(page_index, Arc::new(page))
            .await
            .map_err(|error| error.to_string())?;
        if cancellation.is_cancelled() {
            return Err("The operation was cancelled.".into());
        }
        page_index += 1;
        if incomplete {
            truncated = true;
            break;
        }
        if count == 0 || (cursor.is_some() && !has_next_page) || (cursor.is_none() && count < limit) {
            break;
        }
        truncated = (rows.len() as u64) >= max_rows;
    }
    let total = rows.len() as u64;
    let columns = infos.iter().map(column_meta).collect();
    Ok((columns, rows, ScanSource::new(spool, infos, total), truncated))
}

async fn next_result_page(
    sessions: &ReadSessionStore,
    session_id: SessionId,
    table: &TableInfo,
    offset: u64,
    limit: u64,
    cancellation: &CancellationToken,
    cursor: &mut Option<bookie_app_services::read_session::PostgresTableCursor>,
    epoch: Epoch,
    current_epoch: &AtomicU64,
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
        None => Ok((
            bridge_read_page(
                sessions,
                session_id,
                table,
                offset,
                limit,
                &control,
                epoch,
                current_epoch,
            )
            .await?,
            false,
        )),
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
) -> Result<Pages, String> {
    let control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
    let result = bridge_query(sessions, session_id, (sql, params), &control, epoch, current_epoch).await?;
    spooled_pages(result, max_rows, cancellation).await
}

async fn dictionary_pages(
    sessions: &ReadSessionStore,
    session_id: SessionId,
    capabilities: bookie_core::DriverCapabilities,
    cancellation: &CancellationToken,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<Pages, String> {
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
        return Err("The command bridge returned an unexpected event.".into());
    };
    let result = outcome.map_err(|failure| driver_failure_message(&failure))?;
    spooled_pages(result, ROW_CAP, cancellation).await
}

async fn spooled_pages(
    mut result: QueryResult,
    max_rows: u64,
    cancellation: &CancellationToken,
) -> Result<Pages, String> {
    if cancellation.is_cancelled() {
        return Err("The operation was cancelled.".into());
    }
    let spool = crate::source::spool_manager()
        .create()
        .map_err(|error| error.to_string())?;
    let infos: Vec<ColumnInfo> = result.columns.clone();
    let cap = usize::try_from(max_rows).unwrap_or(usize::MAX);
    let mut budget = QueryResultBudget::default();
    let truncated = bound_query_result(&mut result, cap, &mut budget);
    let page_size = PAGE_ROWS as usize;
    for page_index in 0..result.rows.len().div_ceil(page_size) {
        let start = page_index * page_size;
        let page_rows = result
            .rows
            .iter()
            .skip(start)
            .take(page_size)
            .map(|row| row.to_vec())
            .collect();
        let page = QueryResult {
            columns: infos.clone(),
            rows: QueryRows::from_rows(page_rows),
            truncated: false,
        };
        spool
            .append_page(page_index as u64, Arc::new(page))
            .await
            .map_err(|error| error.to_string())?;
    }
    let total = result.rows.len() as u64;
    let columns = infos.iter().map(column_meta).collect();
    Ok((columns, result.rows, ScanSource::new(spool, infos, total), truncated))
}

async fn bridge_query(
    sessions: &ReadSessionStore,
    session_id: SessionId,
    (sql, params): (&str, &[bookie_core::Value]),
    control: &OperationControl,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<QueryResult, String> {
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
        return Err("The command bridge returned an unexpected event.".into());
    };
    outcome.map_err(|failure| driver_failure_message(&failure))
}

async fn bridge_read_page(
    sessions: &ReadSessionStore,
    session_id: SessionId,
    table: &TableInfo,
    offset: u64,
    limit: u64,
    control: &OperationControl,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<QueryResult, String> {
    let event = dispatch_read(
        DispatchContext::Sessions(sessions),
        Command::ReadPage {
            epoch,
            session_id,
            table: table.clone(),
            offset,
            limit,
        },
        control,
        epoch,
        current_epoch,
    )
    .await?;
    let Event::PageRead { outcome, .. } = event else {
        return Err("The command bridge returned an unexpected event.".into());
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
        return Err("The request was superseded.".into());
    }
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let sink = ChannelEventSink::new(sender);
    dispatch(context, command, control, &sink)
        .await
        .map_err(|error| format!("{error:?}"))?;
    let event = receiver.try_recv().map_err(|error| format!("{error:?}"))?;
    keep_current_event(context, event, Epoch(current_epoch.load(Ordering::Relaxed)))
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
    Err("The request was superseded.".into())
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
        return Err("The command bridge returned an unexpected event.".into());
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
        self.table.is_some() && self.session.writable.is_some()
    }

    fn is_partial(&self) -> bool {
        self.truncated
    }

    fn origin(&self) -> String {
        let cap = if self.truncated {
            format!(" · {} rows loaded; result may be incomplete", self.rows.len())
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
        self.rows.len() as u64
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.rows.get(index))
            .map(<[Cell]>::to_vec)
            .unwrap_or_default()
    }

    fn row_ref(&self, index: u64) -> Option<&[Cell]> {
        usize::try_from(index).ok().and_then(|index| self.rows.get(index))
    }

    fn find(&self, query: &str, cap: usize, cancel: &CancellationToken) -> Result<FindResult, &'static str> {
        find_in(&self.scan, query, cap, cancel)
    }

    fn filter(&self, predicate: &Predicate, cancel: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        filter_in(&self.scan, predicate, cancel)
    }

    fn compare(&self, _keyed: bool, _cancel: &CancellationToken) -> Result<Diff, &'static str> {
        Err("compare is not available for a live table")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn table_count_sql_uses_a_wide_count_for_sql_server_and_quotes_schema_names() {
        let table = TableInfo {
            schema: Some("orders schema".into()),
            name: "order]items".into(),
        };
        assert_eq!(
            super::table_count_sql("mssql", &table),
            "SELECT COUNT_BIG(*) FROM [orders schema].[order]]items]"
        );
        assert_eq!(
            super::table_count_sql("postgres", &table),
            "SELECT COUNT(*) FROM \"orders schema\".\"order]items\""
        );
    }

    #[test]
    fn a_write_session_is_refused_when_the_saved_connection_changed_since_the_table_opened() {
        assert!(super::refuse_edited_connection(Some("a"), "a").is_ok());
        assert!(super::refuse_edited_connection(None, "b").is_ok());
        assert!(super::refuse_edited_connection(Some("a"), "b").is_err());
    }

    use super::*;

    #[test]
    fn a_stale_open_event_closes_its_session() {
        let Origin::File(source) = fixture() else {
            panic!("the fixture is a file")
        };
        let file = FixtureFile(std::env::temp_dir().join(format!("bookie-gpui-stale-{}.sqlite", uuid::Uuid::new_v4())));
        std::fs::copy(source, &file.0).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let database = Arc::new(DatabaseService::new_isolated());
        let sessions = ReadSessionStore::new(Arc::new(drivers_registry::default_registry()), database.clone());
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let opened = runtime.block_on(sessions.open_file(&file.0, &control)).unwrap();

        let result = keep_current_event(
            DispatchContext::Sessions(&sessions),
            Event::SessionOpened {
                epoch: Epoch(1),
                outcome: Ok(opened),
            },
            Epoch(2),
        );

        assert!(result.is_err());
        assert!(database.connection_overview().is_empty());
    }

    #[test]
    fn declared_number_types_are_numeric_and_text_types_are_not() {
        for numeric in [
            "INTEGER",
            "bigint",
            "NUMERIC(10,2)",
            "double precision",
            "BIGSERIAL",
            "REAL",
            "decimal",
        ] {
            assert!(numeric_type(numeric), "{numeric}");
        }
        for other in ["TEXT", "varchar(20)", "timestamp", "uuid", "BLOB", ""] {
            assert!(!numeric_type(other), "{other}");
        }
    }

    struct FixtureFile(std::path::PathBuf);

    impl Drop for FixtureFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn oversized_fixture() -> FixtureFile {
        let path = std::env::temp_dir().join(format!("bookie-gpui-budget-{}.sqlite", uuid::Uuid::new_v4()));
        std::fs::File::create(&path).unwrap();
        let file = FixtureFile(path);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let registry = drivers_registry::default_registry();
            let driver = registry.get("sqlite").unwrap();
            let connection = driver
                .connect(bookie_core::ConnectOptions {
                    database: file.0.to_string_lossy().into_owned(),
                    ..Default::default()
                })
                .await
                .unwrap();
            connection.execute("CREATE TABLE wide (value BLOB)").await.unwrap();
            connection
                .execute("INSERT INTO wide VALUES (zeroblob(67108865))")
                .await
                .unwrap();
            connection.close().await.unwrap();
        });
        file
    }

    #[test]
    fn a_driver_truncated_page_is_reported_as_incomplete() {
        let file = oversized_fixture();
        let table = LiveTable::load(
            &Origin::File(file.0.clone()),
            &TableInfo {
                schema: None,
                name: "wide".into(),
            },
            100,
        )
        .unwrap();
        assert_eq!(table.total(), 0);
        assert!(table.truncated);
    }

    #[test]
    fn an_empty_or_oversized_row_cap_is_refused() {
        let session = LiveSession::open(&fixture()).unwrap();
        assert!(
            session
                .load_table(
                    &TableInfo {
                        schema: None,
                        name: "orders".into()
                    },
                    0
                )
                .is_err()
        );
        assert!(
            session
                .load_table(
                    &TableInfo {
                        schema: None,
                        name: "orders".into()
                    },
                    ROW_CAP + 1
                )
                .is_err()
        );
    }

    #[test]
    fn unsupported_server_sort_is_refused_instead_of_sorting_a_partial_page() {
        let session = LiveSession::open(&fixture()).unwrap();
        let sort = bookie_app_services::read_session::TableSort {
            column: "customer".into(),
            ascending: true,
        };
        let result = session.load_table_sorted_controlled(
            &TableInfo { schema: None, name: "orders".into() },
            100,
            Some(&sort),
            &CancellationToken::new(),
        );
        assert!(matches!(result, Err(message) if message.contains("Server-side table sorting is not available")));
    }

    #[test]
    fn a_cancelled_live_load_is_not_returned_as_a_complete_table() {
        let session = LiveSession::open(&fixture()).unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(
            session
                .load_table_controlled(
                    &TableInfo {
                        schema: None,
                        name: "orders".into()
                    },
                    100,
                    &cancellation
                )
                .is_err()
        );
    }

    #[test]
    fn decoded_budget_is_shared_across_pages_and_keeps_a_whole_row_prefix() {
        let make_page = || QueryResult {
            columns: Vec::new(),
            rows: bookie_core::QueryRows::from_rows(
                (0..40)
                    .map(|index| {
                        vec![
                            bookie_core::Value::Int(index),
                            bookie_core::Value::Text("x".repeat(1024 * 1024)),
                        ]
                    })
                    .collect(),
            ),
            truncated: false,
        };
        let mut budget = QueryResultBudget::default();
        let mut first = make_page();
        assert!(!bound_page(&mut first, &mut budget));
        assert_eq!(first.rows.len(), 40);
        drop(first);

        let mut second = make_page();
        assert!(bound_page(&mut second, &mut budget));
        assert!(second.truncated);
        assert!((1..40).contains(&second.rows.len()));
        for (index, row) in second.rows.iter().enumerate() {
            assert_eq!(row[0], bookie_core::Value::Int(index as i64));
        }
    }

    #[test]
    fn a_cursor_page_larger_than_the_remaining_row_cap_is_truncated() {
        let mut page = QueryResult {
            columns: Vec::new(),
            rows: QueryRows::from_rows((0..3).map(|id| vec![bookie_core::Value::Int(id)]).collect()),
            truncated: false,
        };
        assert!(cap_page_rows(&mut page, 2));
        assert!(page.truncated);
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[1][0], bookie_core::Value::Int(1));
    }

    #[test]
    fn a_query_result_is_bounded_in_place_and_keeps_its_typed_prefix() {
        let row = |index| {
            vec![
                bookie_core::Value::Int(index),
                bookie_core::Value::Text("x".repeat(1024 * 1024)),
            ]
        };
        let mut result = QueryResult {
            columns: Vec::new(),
            rows: QueryRows::from_rows((0..70).map(row).collect()),
            truncated: false,
        };
        let mut budget = QueryResultBudget::default();
        assert!(bound_query_result(&mut result, 100, &mut budget));
        assert!((1..70).contains(&result.rows.len()));
        for (index, retained) in result.rows.iter().enumerate() {
            assert_eq!(retained[0], bookie_core::Value::Int(index as i64));
        }
    }

    #[test]
    fn a_query_result_row_cap_keeps_only_the_prefix_and_marks_it_incomplete() {
        let mut result = QueryResult {
            columns: Vec::new(),
            rows: QueryRows::from_rows((0..3).map(|index| vec![bookie_core::Value::Int(index)]).collect()),
            truncated: false,
        };
        assert!(bound_query_result(&mut result, 2, &mut QueryResultBudget::default()));
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows.last(), Some([bookie_core::Value::Int(1)].as_slice()));
    }

    fn fixture() -> Origin {
        let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        Origin::File(std::path::PathBuf::from(manifest_dir).join("tests/fixtures/orders.sqlite"))
    }

    #[test]
    fn a_database_file_cannot_write_from_the_query_editor() {
        let Origin::File(source) = fixture() else {
            panic!("the fixture is a file")
        };
        let path = std::env::temp_dir().join(format!("bookie-gpui-read-only-{}.sqlite", uuid::Uuid::new_v4()));
        std::fs::copy(source, &path).unwrap();
        let file = FixtureFile(path);
        let live = LiveSession::open(&Origin::File(file.0.clone())).unwrap();
        assert!(live.run_query("DELETE FROM orders", 100).is_err());
        assert_eq!(live.run_query("SELECT id FROM orders", 100).unwrap().total(), 6);
    }

    #[test]
    fn a_bound_query_returns_the_matching_row_and_a_bound_write_is_refused() {
        let Origin::File(source) = fixture() else {
            panic!("the fixture is a file")
        };
        let path = std::env::temp_dir().join(format!("bookie-gpui-bound-{}.sqlite", uuid::Uuid::new_v4()));
        std::fs::copy(source, &path).unwrap();
        let file = FixtureFile(path);
        let live = LiveSession::open(&Origin::File(file.0.clone())).unwrap();
        let cancel = CancellationToken::new();
        let params = [bookie_core::Value::Int(2)];
        let found = live
            .run_query_bound("SELECT id FROM orders WHERE id = ?", &params, 100, &cancel)
            .unwrap();
        assert_eq!(found.total(), 1);
        assert_eq!(found.executed_sql(), Some("SELECT id FROM orders WHERE id = ?"));
        assert!(ResultSource::load_elapsed(&found).is_some());
        assert!(
            live.run_query_bound("DELETE FROM orders WHERE id = ?", &params, 100, &cancel)
                .is_err()
        );
        let later = live.run_query("SELECT id FROM orders", 100).unwrap();
        assert_eq!(later.total(), 6);
        assert_eq!(later.executed_sql(), Some("SELECT id FROM orders"));
        assert_eq!(found.executed_sql(), Some("SELECT id FROM orders WHERE id = ?"));
    }

    #[test]
    fn a_database_file_loads_as_a_live_table_with_real_columns_and_nulls() {
        let table = LiveTable::load(
            &fixture(),
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load");
        let names: Vec<&str> = table.columns().iter().map(|column| column.name.as_str()).collect();
        assert_eq!(names, ["id", "customer", "total", "paid", "note"]);
        let keys: Vec<bool> = table.columns().iter().map(|column| column.primary_key).collect();
        assert_eq!(keys, [true, false, false, false, false]);
        assert_eq!(table.total(), 6);
        assert!(!table.truncated);
        assert_eq!(table.row(1)[1], Cell::Text("Grace".into()));
        assert_eq!(table.row(1)[4], Cell::Null);
        assert_eq!(table.row(2)[4], Cell::Text(String::new()));
    }

    #[test]
    fn connection_status_uses_the_read_command_bridge() {
        let session = LiveSession::open(&fixture()).expect("open fixture");

        let overview = session.overview().expect("connection status");

        assert_eq!(overview.len(), 1);
        assert_eq!(overview[0].driver_id, "sqlite");
        assert_eq!(
            overview[0].health,
            bookie_app_services::database_service::ConnectionHealth::Healthy
        );
        assert_eq!(
            overview[0].tunnel,
            bookie_app_services::database_service::TunnelState::Direct
        );
    }

    #[test]
    fn find_and_filter_answer_from_the_loaded_rows() {
        let table = LiveTable::load(
            &fixture(),
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load");
        let cancel = CancellationToken::new();
        let found = table.find("ada", 10, &cancel).expect("find");
        assert_eq!(found.matches, vec![0, 3]);
        let columns = table.columns().to_vec();
        let predicate = crate::model::parse_predicate("customer = Ada", &columns).expect("predicate");
        assert_eq!(table.filter(&predicate, &cancel).expect("filter"), vec![0, 3]);
    }

    #[test]
    fn one_session_loads_each_of_its_tables() {
        let session = LiveSession::open(&fixture()).expect("open");
        assert_eq!(
            session.tables,
            [
                TableInfo {
                    schema: None,
                    name: "customers".into()
                },
                TableInfo {
                    schema: None,
                    name: "orders".into()
                }
            ]
        );
        assert_eq!(session.label, "orders.sqlite");
        let customers = session
            .load_table(
                &TableInfo {
                    schema: None,
                    name: "customers".into(),
                },
                100,
            )
            .expect("customers");
        let orders = session
            .load_table(
                &TableInfo {
                    schema: None,
                    name: "orders".into(),
                },
                100,
            )
            .expect("orders");
        assert_eq!((customers.total(), orders.total()), (2, 6));
        assert_eq!(customers.columns().len(), 2);
        assert_eq!(orders.columns().len(), 5);
    }

    #[test]
    fn an_explain_statement_for_the_driver_runs_in_the_read_only_session() {
        let session = LiveSession::open(&fixture()).expect("open");
        let statement = bookie_core::sql_dialect::explain_statement(&session.driver_id, "SELECT id FROM orders")
            .expect("sqlite can explain");
        let plan = session.run_query(&statement, 100).expect("plan");
        assert!(plan.total() >= 1);
    }

    #[test]
    fn a_query_becomes_a_grid_source_with_its_own_columns_and_cap() {
        let session = LiveSession::open(&fixture()).expect("open");
        let result = session
            .run_query("SELECT customer, total FROM orders WHERE paid = 1 ORDER BY id", 100)
            .expect("query");
        let names: Vec<&str> = result.columns().iter().map(|column| column.name.as_str()).collect();
        assert_eq!(names, ["customer", "total"]);
        assert_eq!(result.total(), 3);
        assert_eq!(result.row(0)[0], Cell::Text("Ada".into()));
        let cancel = CancellationToken::new();
        assert_eq!(result.find("linus", 5, &cancel).expect("find").matches, vec![1]);
        let capped = session.run_query("SELECT id FROM orders", 2).expect("capped");
        assert_eq!((capped.total(), capped.truncated), (2, true));
        assert!(session.run_query("DELETE FROM orders", 100).is_err());
        assert!(session.run_query("SELECT nope FROM orders", 100).is_err());
    }

    #[test]
    fn the_row_cap_truncates_and_says_so() {
        let table = LiveTable::load(
            &fixture(),
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            4,
        )
        .expect("load");
        assert_eq!(table.total(), 4);
        assert!(table.truncated);
        assert_eq!(
            table.origin(),
            "sqlite · orders · read-only · page order may change; rows may be skipped or repeated · 4 rows loaded; result may be incomplete"
        );
        let whole = LiveTable::load(
            &fixture(),
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load");
        assert_eq!(
            whole.origin(),
            "sqlite · orders · read-only · page order may change; rows may be skipped or repeated"
        );
        assert!(whole.origin().contains("page order may change; rows may be skipped or repeated"));
    }

    #[test]
    fn a_missing_table_or_file_is_an_error_not_a_panic() {
        assert!(
            LiveTable::load(
                &fixture(),
                &TableInfo {
                    schema: None,
                    name: "nope".into()
                },
                10
            )
            .is_err()
        );
        let missing = Origin::File("/nonexistent/none.sqlite".into());
        assert!(
            LiveTable::load(
                &missing,
                &TableInfo {
                    schema: None,
                    name: "orders".into()
                },
                10
            )
            .is_err()
        );
    }
}

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
            return Err(search_error("The command bridge returned an unexpected event.".into()));
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
        bridge_query(&session.sessions, session.session_id, (sql, params), control, epoch, &session.epoch)
            .await
            .map_err(search_error)
    }
}

fn search_error(message: String) -> DriverError {
    DriverError::Query {
        message,
        sqlstate: None,
        position: None,
    }
}

fn backup_failure_text(error: &DriverError) -> String {
    match error {
        DriverError::Cancelled => "Cancelled. Nothing was changed.".into(),
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
