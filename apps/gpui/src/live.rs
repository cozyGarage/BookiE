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
use bookie_core::{ColumnInfo, OperationControl, QueryResult, QueryResultBudget, QueryRows, TableInfo};
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

pub struct LiveSession {
    runtime: tokio::runtime::Runtime,
    sessions: ReadSessionStore,
    session_id: SessionId,
    epoch: AtomicU64,
    pub driver_id: String,
    pub label: String,
    pub tables: Vec<TableInfo>,
    supports_snapshot_cursor: bool,
}

impl LiveSession {
    pub fn open(origin: &Origin) -> Result<Arc<LiveSession>, String> {
        Self::open_controlled(origin, &CancellationToken::new())
    }

    pub fn open_controlled(origin: &Origin, cancellation: &CancellationToken) -> Result<Arc<LiveSession>, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let (sessions, opened) = runtime.block_on(open_session(origin, cancellation.clone()))?;
        Ok(Arc::new(LiveSession {
            driver_id: opened.driver_id.clone(),
            runtime,
            sessions,
            session_id: opened.session_id,
            epoch: AtomicU64::new(0),
            label: opened.label,
            tables: opened.tables,
            supports_snapshot_cursor: opened.supports_snapshot_cursor,
        }))
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
        if max_rows == 0 || max_rows > ROW_CAP {
            return Err(format!("Row limit must be between 1 and {ROW_CAP}."));
        }
        let epoch = self.next_epoch();
        let (columns, rows, scan, truncated) = self.runtime.block_on(query_pages(
            &self.sessions,
            self.session_id,
            sql,
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
        if max_rows == 0 || max_rows > ROW_CAP {
            return Err(format!("Row limit must be between 1 and {ROW_CAP}."));
        }
        let table = find_table(&self.tables, table)?.clone();
        let epoch = self.next_epoch();
        let (columns, rows, scan, truncated) = self.runtime.block_on(read_pages(
            &self.sessions,
            self.session_id,
            &table,
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
            snapshot_consistent: self.supports_snapshot_cursor,
            table: Some(table.clone()),
        })
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
    table: Option<TableInfo>,
}

impl LiveTable {
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

async fn open_session(
    origin: &Origin,
    cancellation: CancellationToken,
) -> Result<(ReadSessionStore, OpenedSession), String> {
    let registry = Arc::new(drivers_registry::default_registry());
    let sessions = ReadSessionStore::new(registry, Arc::new(DatabaseService::new()))
        .with_subscription_sampler(Arc::new(RegistrySubscriptionSampler));
    let epoch = Epoch(1);
    let current_epoch = AtomicU64::new(epoch.0);
    let control = OperationControl::new(
        cancellation,
        Some(tokio::time::Instant::now() + Duration::from_secs(30)),
    );
    let command = match origin {
        Origin::Saved(connection) => {
            let saved = bridge_saved_connections(&control, epoch, &current_epoch).await?;
            let connection_id = saved
                .iter()
                .find(|saved| saved.id.to_string() == *connection || saved.name == *connection)
                .map(|saved| saved.id)
                .ok_or_else(|| format!("no saved connection named or numbered {connection}"))?;
            Command::OpenSavedConnection { epoch, connection_id }
        }
        Origin::File(path) => Command::OpenConnectionFile {
            epoch,
            path: path.clone(),
        },
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
    Ok((sessions, outcome?))
}

struct RegistrySubscriptionSampler;

impl SubscriptionSampler for RegistrySubscriptionSampler {
    fn validate(&self, request: &SubscriptionRequest) -> Result<(), bookie_core::DriverError> {
        drivers_registry::validate_pubsub_request(&request.patterns, request.max_messages, request.window)
    }

    fn sample(
        &self,
        options: bookie_core::ConnectOptions,
        request: SubscriptionRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SubscriptionSample, bookie_core::DriverError>> + Send + '_>> {
        Box::pin(async move {
            let sample =
                drivers_registry::sample_pubsub(options, &request.patterns, request.max_messages, request.window)
                    .await?;
            Ok(SubscriptionSample {
                messages: sample
                    .messages
                    .into_iter()
                    .map(|message| bookie_app_services::read_session_store::SubscriptionMessage {
                        channel: message.channel,
                        pattern: message.pattern,
                        payload: message.payload,
                        payload_bytes: message.payload_bytes,
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
    cancellation: &CancellationToken,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<Pages, String> {
    let open_control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
    let mut cursor = sessions
        .open_table_cursor(session_id, table, &open_control)
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
    sql: &str,
    max_rows: u64,
    cancellation: &CancellationToken,
    epoch: Epoch,
    current_epoch: &AtomicU64,
) -> Result<Pages, String> {
    let control = OperationControl::new(cancellation.clone(), Some(tokio::time::Instant::now() + PAGE_TIMEOUT));
    let mut result = bridge_query(sessions, session_id, sql, &control, epoch, current_epoch).await?;
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
    sql: &str,
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

fn column_meta(info: &ColumnInfo) -> ColumnMeta {
    let measured = (info.name.chars().count() as f32 * 9.0 + 48.0).clamp(110.0, 320.0);
    let width = if numeric_type(&info.data_type) {
        measured.max(200.0)
    } else {
        measured
    };
    ColumnMeta {
        name: info.name.clone(),
        width,
        numeric: numeric_type(&info.data_type),
    }
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
    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
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
                " · concurrent changes may skip or repeat rows"
            }
        });
        format!("{} · {} · read-only{consistency}{cap}", self.session.driver_id, table)
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
    use super::*;

    #[test]
    fn a_stale_open_event_closes_its_session() {
        let Origin::File(source) = fixture() else {
            unreachable!()
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
            unreachable!()
        };
        let path = std::env::temp_dir().join(format!("bookie-gpui-read-only-{}.sqlite", uuid::Uuid::new_v4()));
        std::fs::copy(source, &path).unwrap();
        let file = FixtureFile(path);
        let live = LiveSession::open(&Origin::File(file.0.clone())).unwrap();
        assert!(live.run_query("DELETE FROM orders", 100).is_err());
        assert_eq!(live.run_query("SELECT id FROM orders", 100).unwrap().total(), 6);
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
            "sqlite · orders · read-only · concurrent changes may skip or repeat rows · 4 rows loaded; result may be incomplete"
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
            "sqlite · orders · read-only · concurrent changes may skip or repeat rows"
        );
        assert!(whole.origin().contains("concurrent changes may skip or repeat rows"));
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
