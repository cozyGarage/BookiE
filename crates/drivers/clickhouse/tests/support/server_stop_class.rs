#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use bookie_core::{CatalogObjectKind, ConnectOptions, Connection, DriverError, OperationControl, RoutineRef, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::clickhouse::{connect, start_clickhouse};

const STOP_REQUEST: &[u8] = b"KILL QUERY";
const STOP_AFTER: Duration = Duration::from_millis(200);
const OBSERVE_FOR: Duration = Duration::from_secs(4);

type CallFuture = Pin<Box<dyn Future<Output = Result<(), DriverError>> + Send>>;
type Call = Box<dyn Fn(Arc<dyn Connection>, OperationControl) -> CallFuture + Send + Sync>;

struct StopCase {
    method: &'static str,
    statement_marker: Option<&'static [u8]>,
    call: Call,
}

fn sends<F, Fut, T>(method: &'static str, statement_marker: &'static [u8], call: F) -> StopCase
where
    F: Fn(Arc<dyn Connection>, OperationControl) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DriverError>> + Send + 'static,
    T: Send,
{
    StopCase {
        method,
        statement_marker: Some(statement_marker),
        call: Box::new(move |connection, control| {
            let call = call(connection, control);
            Box::pin(async move { call.await.map(|_| ()) })
        }),
    }
}

fn sends_nothing<F, Fut, T>(method: &'static str, call: F) -> StopCase
where
    F: Fn(Arc<dyn Connection>, OperationControl) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DriverError>> + Send + 'static,
    T: Send,
{
    StopCase {
        statement_marker: None,
        ..sends(method, b"", call)
    }
}

fn hidden() -> Vec<String> {
    vec!["hidden".into()]
}

fn insert_batch() -> Vec<(String, Vec<Value>)> {
    vec![(
        "INSERT INTO stop_class_rows (id, note) VALUES (7, 'batch')".into(),
        Vec::new(),
    )]
}

fn stop_cases() -> Vec<StopCase> {
    const READ: &str = "SELECT 1 AS stop_class_marker";
    const WRITE: &str = "INSERT INTO stop_class_rows (id, note) VALUES (2, 'written')";
    vec![
        sends(
            "list_databases_controlled",
            b"system.databases",
            |c, control| async move { c.list_databases_controlled(&control).await },
        ),
        sends("list_tables_controlled", b"system.tables", |c, control| async move {
            c.list_tables_controlled(&control).await
        }),
        sends("list_tables_in", b"system.tables", |c, control| async move {
            c.list_tables_in(&hidden(), &control).await
        }),
        sends(
            "list_tables_excluding_controlled",
            b"system.tables",
            |c, control| async move { c.list_tables_excluding_controlled(&hidden(), &control).await },
        ),
        sends("fetch_columns_controlled", b"system.columns", |c, control| async move {
            c.fetch_columns_controlled(None, "stop_class_rows", &control).await
        }),
        sends("fetch_indexes_controlled", b"primary_key", |c, control| async move {
            c.fetch_indexes_controlled(None, "stop_class_rows", &control).await
        }),
        sends("fetch_rows_controlled", b"stop_class_rows", |c, control| async move {
            c.fetch_rows_controlled(None, "stop_class_rows", 0, 10, &control).await
        }),
        sends("query_controlled", b"stop_class_marker", |c, control| async move {
            c.query_controlled(READ, &control).await
        }),
        sends(
            "query_params_controlled",
            b"stop_class_marker",
            |c, control| async move {
                c.query_params_controlled("SELECT ? AS stop_class_marker", &[Value::Int(1)], &control)
                    .await
            },
        ),
        sends(
            "query_result_sets_controlled",
            b"stop_class_marker",
            |c, control| async move { c.query_result_sets_controlled(READ, &[], &control).await },
        ),
        sends(
            "execute_controlled",
            b"INSERT INTO stop_class_rows",
            |c, control| async move { c.execute_controlled(WRITE, &control).await },
        ),
        sends(
            "execute_params_controlled",
            b"INSERT INTO stop_class_rows",
            |c, control| async move {
                c.execute_params_controlled(
                    "INSERT INTO stop_class_rows (id, note) VALUES (?, ?)",
                    &[Value::Int(3), Value::Text("param".into())],
                    &control,
                )
                .await
            },
        ),
        sends(
            "execute_controlled_with_token",
            b"INSERT INTO stop_class_rows",
            |c, control| async move {
                c.execute_controlled_with_token(WRITE, &control)
                    .await
                    .map_err(|(error, _)| error)
            },
        ),
        sends(
            "execute_in_transaction_controlled",
            b"INSERT INTO stop_class_rows",
            |c, control| async move { c.execute_in_transaction_controlled(&insert_batch(), &control).await },
        ),
        sends_nothing("list_collations_controlled", |c, control| async move {
            c.list_collations_controlled(&control).await
        }),
        sends_nothing("list_views_controlled", |c, control| async move {
            c.list_views_controlled(&control).await
        }),
        sends_nothing("list_views_excluding_controlled", |c, control| async move {
            c.list_views_excluding_controlled(&hidden(), &control).await
        }),
        sends_nothing("lexical_settings_controlled", |c, control| async move {
            c.lexical_settings_controlled(&control).await
        }),
        sends_nothing("list_objects_controlled", |c, control| async move {
            c.list_objects_controlled(CatalogObjectKind::Sequence, None, &control)
                .await
        }),
        sends_nothing("list_objects_excluding_controlled", |c, control| async move {
            c.list_objects_excluding_controlled(CatalogObjectKind::Sequence, None, &hidden(), &control)
                .await
        }),
        sends_nothing("routine_definition_controlled", |c, control| async move {
            let routine = RoutineRef {
                schema: "default".into(),
                name: "none".into(),
                identity_arguments: None,
            };
            c.routine_definition_controlled(&routine, &control).await
        }),
        sends_nothing("fetch_foreign_keys_controlled", |c, control| async move {
            c.fetch_foreign_keys_controlled(None, "stop_class_rows", &control).await
        }),
        sends_nothing("fetch_inbound_foreign_keys_controlled", |c, control| async move {
            c.fetch_inbound_foreign_keys_controlled(None, "stop_class_rows", &control)
                .await
        }),
        sends_nothing("fetch_view_editability_controlled", |c, control| async move {
            c.fetch_view_editability_controlled(None, "stop_class_rows", &control)
                .await
        }),
        sends_nothing("fetch_enum_labels_controlled", |c, control| async move {
            c.fetch_enum_labels_controlled("default", "mood", &control).await
        }),
        sends_nothing("begin_controlled", |c, control| async move {
            c.begin_controlled(&control).await
        }),
        sends_nothing("server_version_controlled", |c, control| async move {
            c.server_version_controlled(&control).await
        }),
        sends_nothing(
            "synchronize_auto_increment_sequences_controlled",
            |c, control| async move {
                c.synchronize_auto_increment_sequences_controlled(None, "stop_class_rows", &["id".into()], &control)
                    .await
            },
        ),
        sends_nothing("execute_in_transaction_checked_controlled", |c, control| async move {
            c.execute_in_transaction_checked_controlled(&insert_batch(), &[0], &control)
                .await
        }),
    ]
}

fn controlled_calls(source: &str, contract: &str, prefix: &str) -> BTreeSet<String> {
    source
        .split(contract)
        .nth(1)
        .expect("the contract trait")
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default()
        .split("async fn ")
        .skip(1)
        .filter(|rest| {
            let signature = rest.split(['{', ';']).next().unwrap_or_default();
            signature.contains("OperationControl")
        })
        .filter_map(|rest| rest.split('(').next())
        .map(|name| format!("{prefix}{}", name.trim()))
        .collect()
}

#[test]
fn every_controlled_call_of_the_contract_has_a_clickhouse_stop_case() {
    let declared = controlled_calls(
        include_str!("../../../../core/src/connection.rs"),
        "pub trait Connection",
        "",
    );
    let cases = stop_cases();
    let covered: BTreeSet<String> = cases.iter().map(|case| case.method.to_string()).collect();
    assert_eq!(covered, declared);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_stopped_call_asks_clickhouse_to_kill_the_query_it_started() {
    let (_container, options) = start_clickhouse().await;
    create_fixture(connect(options.clone()).await.as_ref()).await;
    let proxy = StallProxy::start(&options).await;
    let connection: Arc<dyn Connection> = Arc::from(
        connect(ConnectOptions {
            host: "127.0.0.1".into(),
            port: proxy.port,
            ..options
        })
        .await,
    );

    let mut failures = Vec::new();
    for case in stop_cases() {
        connection.query("SELECT 1").await.unwrap();
        let outcome = match case.statement_marker {
            Some(marker) => stop_once(&case, marker, &proxy, &connection).await,
            None => sends_no_statement(&case, &proxy, &connection).await,
        };
        if let Err(failure) = outcome {
            failures.push(format!("{}: {failure}", case.method));
        }
        proxy.release();
        tokio::time::timeout(OBSERVE_FOR, connection.query("SELECT 1"))
            .await
            .unwrap_or_else(|_| panic!("the client serves no statement after {}", case.method))
            .unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

async fn sends_no_statement(
    case: &StopCase,
    proxy: &StallProxy,
    connection: &Arc<dyn Connection>,
) -> Result<(), String> {
    let forwarded = proxy.forwarded_chunks();
    let outcome = tokio::time::timeout(
        OBSERVE_FOR,
        (case.call)(connection.clone(), OperationControl::with_timeout(OBSERVE_FOR)),
    )
    .await
    .map_err(|_| "the call never returned".to_string())?;
    if proxy.forwarded_chunks() != forwarded {
        return Err(format!(
            "the call is listed as sending nothing but reached the server; it returned {outcome:?}"
        ));
    }
    Ok(())
}

async fn stop_once(
    case: &StopCase,
    marker: &'static [u8],
    proxy: &StallProxy,
    connection: &Arc<dyn Connection>,
) -> Result<(), String> {
    let kills = proxy.cancel_requests();
    proxy.withhold_reply_to(marker);
    let call = tokio::spawn((case.call)(
        connection.clone(),
        OperationControl::with_timeout(STOP_AFTER),
    ));
    let asked = proxy.wait_for_cancel_request(kills).await;
    let reached = proxy.withheld_replies() > 0;
    let started = proxy.withheld_query_id();
    let kill_texts = proxy.kill_texts();
    proxy.release();
    let outcome = tokio::time::timeout(Duration::from_secs(20), call)
        .await
        .map_err(|_| "the call never returned".to_string())?
        .unwrap();
    if !reached {
        return Err("the statement never reached the server, so the case proves nothing".into());
    }
    if !asked {
        return Err(format!(
            "no KILL QUERY reached ClickHouse; the call returned {outcome:?}"
        ));
    }
    let Some(started) = started.filter(|id| !id.is_empty()) else {
        return Err("the statement carried no query id, so a kill could not name it".into());
    };
    if !kill_texts.iter().any(|text| text.contains(&started)) {
        return Err(format!(
            "the KILL QUERY did not name the statement's query id {started}"
        ));
    }
    if !is_unknown_outcome(&outcome) {
        return Err(format!("a withheld reply was reported as {outcome:?}"));
    }
    Ok(())
}

fn is_unknown_outcome(outcome: &Result<(), DriverError>) -> bool {
    match outcome {
        Err(DriverError::OperationOutcomeUnknown { .. }) => true,
        Err(DriverError::Transaction { source, .. } | DriverError::PartiallyApplied { source, .. }) => {
            matches!(source.as_ref(), DriverError::OperationOutcomeUnknown { .. })
        }
        _ => false,
    }
}

async fn create_fixture(admin: &dyn Connection) {
    for sql in [
        "DROP TABLE IF EXISTS stop_class_rows",
        "CREATE TABLE stop_class_rows (id UInt32, note String) ENGINE = MergeTree ORDER BY id",
        "INSERT INTO stop_class_rows (id, note) VALUES (1, 'kept')",
        "DROP VIEW IF EXISTS slow_rows",
        "CREATE VIEW slow_rows AS SELECT sleepEachRow(0.05) AS pause, number AS id FROM numbers(1000)",
    ] {
        admin.execute(sql).await.unwrap();
    }
}

async fn slow_queries_running(observer: &dyn Connection) -> i64 {
    let result = observer
        .query(
            "SELECT toInt64(count()) FROM system.processes \
             WHERE query LIKE '%slow_rows%' AND query NOT LIKE '%system.processes%'",
        )
        .await
        .unwrap();
    match result.rows.first().and_then(|row| row.first()) {
        Some(Value::Int(count)) => *count,
        other => panic!("process count is not an integer: {other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_page_read_stopped_by_its_deadline_is_killed_on_the_server() {
    let (_container, options) = start_clickhouse().await;
    let observer = connect(options.clone()).await;
    create_fixture(observer.as_ref()).await;
    let connection = connect(options).await;

    let started = std::time::Instant::now();
    let error = connection
        .fetch_rows_controlled(None, "slow_rows", 900, 10, &OperationControl::with_timeout(STOP_AFTER))
        .await
        .expect_err("a page that scans a slow view cannot finish");
    assert!(matches!(error, DriverError::TimedOut), "{error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "the call outlived its stop"
    );

    for _ in 0..40 {
        if slow_queries_running(observer.as_ref()).await == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(
        slow_queries_running(observer.as_ref()).await,
        0,
        "the page read kept running on the server after the stop"
    );
    connection.query("SELECT 1").await.unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_controlled_clickhouse_batch_and_metadata_still_return_their_data() {
    let (_container, options) = start_clickhouse().await;
    let connection = connect(options).await;
    create_fixture(connection.as_ref()).await;
    let control = OperationControl::with_timeout(Duration::from_secs(30));

    let databases = connection.list_databases_controlled(&control).await.unwrap();
    assert!(databases.iter().any(|name| name == "default"), "{databases:?}");
    let tables = connection.list_tables_controlled(&control).await.unwrap();
    assert!(tables.iter().any(|table| table.name == "stop_class_rows"), "{tables:?}");
    let columns = connection
        .fetch_columns_controlled(None, "stop_class_rows", &control)
        .await
        .unwrap();
    assert_eq!(columns.len(), 2);
    let indexes = connection
        .fetch_indexes_controlled(None, "stop_class_rows", &control)
        .await
        .unwrap();
    assert_eq!(indexes.len(), 1);
    let page = connection
        .fetch_rows_controlled(None, "stop_class_rows", 0, 10, &control)
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);

    let affected = connection
        .execute_in_transaction_controlled(&insert_batch(), &control)
        .await
        .unwrap();
    assert_eq!(affected.len(), 1);
    let count = connection
        .query("SELECT toInt64(count()) FROM stop_class_rows")
        .await
        .unwrap();
    assert_eq!(count.rows[0][0], Value::Int(2));
}

struct ProxyState {
    withheld_ids: std::sync::Mutex<Vec<String>>,
    kill_texts: std::sync::Mutex<Vec<String>>,
    marker: std::sync::Mutex<Option<&'static [u8]>>,
    withheld: AtomicUsize,
    cancel_requests: AtomicUsize,
    forwarded: AtomicUsize,
    hold: watch::Sender<bool>,
}

struct StallProxy {
    port: u16,
    state: Arc<ProxyState>,
}

impl StallProxy {
    async fn start(options: &ConnectOptions) -> Self {
        let upstream = tokio::net::lookup_host((options.host.as_str(), options.port))
            .await
            .unwrap()
            .next()
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (hold, _) = watch::channel(false);
        let state = Arc::new(ProxyState {
            withheld_ids: std::sync::Mutex::new(Vec::new()),
            kill_texts: std::sync::Mutex::new(Vec::new()),
            marker: std::sync::Mutex::new(None),
            withheld: AtomicUsize::new(0),
            cancel_requests: AtomicUsize::new(0),
            forwarded: AtomicUsize::new(0),
            hold,
        });
        let accept_state = state.clone();
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                tokio::spawn(relay(client, upstream, accept_state.clone()));
            }
        });
        Self { port, state }
    }

    fn withhold_reply_to(&self, marker: &'static [u8]) {
        self.state.withheld.store(0, Ordering::SeqCst);
        self.state.withheld_ids.lock().unwrap().clear();
        self.state.hold.send_replace(true);
        *self.state.marker.lock().unwrap() = Some(marker);
    }

    fn release(&self) {
        *self.state.marker.lock().unwrap() = None;
        self.state.hold.send_replace(false);
    }

    fn withheld_replies(&self) -> usize {
        self.state.withheld.load(Ordering::SeqCst)
    }

    fn forwarded_chunks(&self) -> usize {
        self.state.forwarded.load(Ordering::SeqCst)
    }

    fn withheld_query_id(&self) -> Option<String> {
        self.state.withheld_ids.lock().unwrap().first().cloned()
    }

    fn kill_texts(&self) -> Vec<String> {
        self.state.kill_texts.lock().unwrap().clone()
    }

    fn cancel_requests(&self) -> usize {
        self.state.cancel_requests.load(Ordering::SeqCst)
    }

    async fn wait_for_cancel_request(&self, seen_before: usize) -> bool {
        let deadline = tokio::time::Instant::now() + OBSERVE_FOR;
        while tokio::time::Instant::now() < deadline {
            if self.cancel_requests() > seen_before {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }
}

fn query_id_in(chunk: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(chunk);
    let start = text.find("query_id=")? + "query_id=".len();
    let id: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!id.is_empty()).then_some(id)
}

fn contains_marker(unmatched: &mut Vec<u8>, bytes: &[u8], marker: &[u8]) -> bool {
    unmatched.extend_from_slice(bytes);
    let found = unmatched.windows(marker.len()).any(|window| window == marker);
    let keep = marker.len().saturating_sub(1);
    let discard = unmatched.len().saturating_sub(keep);
    unmatched.drain(..discard);
    found
}

async fn relay(client: TcpStream, upstream: SocketAddr, state: Arc<ProxyState>) {
    let Ok(server) = TcpStream::connect(upstream).await else {
        return;
    };
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    let withheld = Arc::new(AtomicBool::new(false));
    let outbound_withheld = withheld.clone();
    let outbound_state = state.clone();
    let outbound = tokio::spawn(async move {
        let mut bytes = [0; 16384];
        let mut unmatched_stop = Vec::new();
        let mut unmatched = Vec::new();
        let mut latest_query_id = String::new();
        while let Ok(count) = client_read.read(&mut bytes).await {
            if count == 0 {
                break;
            }
            let chunk = &bytes[..count];
            outbound_state.forwarded.fetch_add(1, Ordering::SeqCst);
            if let Some(id) = query_id_in(chunk) {
                latest_query_id = id;
            }
            if contains_marker(&mut unmatched_stop, chunk, STOP_REQUEST) {
                outbound_state
                    .kill_texts
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(chunk).into_owned());
                outbound_state.cancel_requests.fetch_add(1, Ordering::SeqCst);
            }
            let marker = *outbound_state.marker.lock().unwrap();
            let found = match marker {
                Some(marker) => contains_marker(&mut unmatched, chunk, marker),
                None => {
                    unmatched.clear();
                    false
                }
            };
            if found {
                *outbound_state.marker.lock().unwrap() = None;
                outbound_withheld.store(true, Ordering::SeqCst);
                outbound_state
                    .withheld_ids
                    .lock()
                    .unwrap()
                    .push(latest_query_id.clone());
                outbound_state.withheld.fetch_add(1, Ordering::SeqCst);
            }
            if server_write.write_all(chunk).await.is_err() {
                break;
            }
        }
    });
    let mut hold = state.hold.subscribe();
    let mut bytes = [0; 16384];
    while let Ok(count) = server_read.read(&mut bytes).await {
        if count == 0 {
            break;
        }
        if withheld.swap(false, Ordering::SeqCst) {
            let _ = hold.wait_for(|held| !held).await;
        }
        if client_write.write_all(&bytes[..count]).await.is_err() {
            break;
        }
    }
    outbound.abort();
}
