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

use crate::{connect, start_mysql};

const STOP_REQUEST: &[u8] = b"KILL QUERY";
const STOP_AFTER: Duration = Duration::from_millis(500);
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

fn one_row_write() -> Vec<(String, Vec<Value>)> {
    vec![(
        "UPDATE stop_class_rows SET note = ? WHERE id = 1".into(),
        vec![Value::Text("stopped".into())],
    )]
}

fn hidden() -> Vec<String> {
    vec!["hidden".into()]
}

fn catalog_cases() -> Vec<StopCase> {
    vec![
        sends(
            "list_collations_controlled",
            b"information_schema.collations",
            |c, control| async move { c.list_collations_controlled(&control).await },
        ),
        sends(
            "list_databases_controlled",
            b"information_schema.schemata",
            |c, control| async move { c.list_databases_controlled(&control).await },
        ),
        sends(
            "list_tables_controlled",
            b"information_schema.tables",
            |c, control| async move { c.list_tables_controlled(&control).await },
        ),
        sends(
            "list_tables_in",
            b"information_schema.tables",
            |c, control| async move { c.list_tables_in(&hidden(), &control).await },
        ),
        sends(
            "list_tables_excluding_controlled",
            b"information_schema.tables",
            |c, control| async move { c.list_tables_excluding_controlled(&hidden(), &control).await },
        ),
        sends(
            "list_views_controlled",
            b"information_schema.tables",
            |c, control| async move { c.list_views_controlled(&control).await },
        ),
        sends(
            "list_views_excluding_controlled",
            b"information_schema.tables",
            |c, control| async move { c.list_views_excluding_controlled(&hidden(), &control).await },
        ),
        sends(
            "lexical_settings_controlled",
            b"SELECT @@SESSION.sql_mode",
            |c, control| async move { c.lexical_settings_controlled(&control).await },
        ),
        sends("server_version_controlled", b"VERSION()", |c, control| async move {
            c.server_version_controlled(&control).await
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
                schema: "stop".into(),
                name: "stop_class_routine".into(),
                identity_arguments: None,
            };
            c.routine_definition_controlled(&routine, &control).await
        }),
    ]
}

fn table_metadata_cases() -> Vec<StopCase> {
    vec![
        sends(
            "fetch_columns_controlled",
            b"generation_expression",
            |c, control| async move { c.fetch_columns_controlled(None, "stop_class_rows", &control).await },
        ),
        sends("fetch_indexes_controlled", b"seq_in_index", |c, control| async move {
            c.fetch_indexes_controlled(None, "stop_class_rows", &control).await
        }),
        sends(
            "fetch_foreign_keys_controlled",
            b"delete_rule",
            |c, control| async move { c.fetch_foreign_keys_controlled(None, "stop_class_rows", &control).await },
        ),
        sends(
            "fetch_inbound_foreign_keys_controlled",
            b"referenced_table_name = ?",
            |c, control| async move {
                c.fetch_inbound_foreign_keys_controlled(None, "stop_class_rows", &control)
                    .await
            },
        ),
        sends("fetch_rows_controlled", b"stop_class_rows", |c, control| async move {
            c.fetch_rows_controlled(None, "stop_class_rows", 0, 10, &control).await
        }),
        sends(
            "fetch_view_editability_controlled",
            b"information_schema.views",
            |c, control| async move {
                c.fetch_view_editability_controlled(None, "stop_class_rows", &control)
                    .await
            },
        ),
        sends_nothing("fetch_enum_labels_controlled", |c, control| async move {
            c.fetch_enum_labels_controlled("stop", "mood", &control).await
        }),
    ]
}

fn statement_cases() -> Vec<StopCase> {
    const READ: &str = "SELECT 1 AS stop_class_marker";
    const WRITE: &str = "UPDATE stop_class_rows SET note = note";
    vec![
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
        sends("execute_controlled", b"stop_class_rows", |c, control| async move {
            c.execute_controlled(WRITE, &control).await
        }),
        sends(
            "execute_params_controlled",
            b"stop_class_rows",
            |c, control| async move {
                c.execute_params_controlled(
                    "UPDATE stop_class_rows SET note = ?",
                    &[Value::Text("a".into())],
                    &control,
                )
                .await
            },
        ),
        sends(
            "execute_controlled_with_token",
            b"stop_class_rows",
            |c, control| async move {
                c.execute_controlled_with_token(WRITE, &control)
                    .await
                    .map_err(|(error, _)| error)
            },
        ),
        sends(
            "execute_in_transaction_controlled",
            b"stop_class_rows",
            |c, control| async move { c.execute_in_transaction_controlled(&one_row_write(), &control).await },
        ),
        sends(
            "execute_in_transaction_checked_controlled",
            b"stop_class_rows",
            |c, control| async move {
                c.execute_in_transaction_checked_controlled(&one_row_write(), &[0], &control)
                    .await
            },
        ),
        sends("begin_controlled", b"BEGIN", |c, control| async move {
            c.begin_controlled(&control).await
        }),
        sends_nothing(
            "synchronize_auto_increment_sequences_controlled",
            |c, control| async move {
                c.synchronize_auto_increment_sequences_controlled(None, "stop_class_rows", &["id".into()], &control)
                    .await
            },
        ),
    ]
}

fn transaction_cases() -> Vec<StopCase> {
    const READ: &str = "SELECT 1 AS stop_class_marker";
    const WRITE: &str = "UPDATE stop_class_rows SET note = note";
    vec![
        sends(
            "Transaction::query_controlled",
            b"stop_class_marker",
            |c, control| async move { c.begin().await?.query_controlled(READ, &control).await },
        ),
        sends(
            "Transaction::query_params_controlled",
            b"stop_class_marker",
            |c, control| async move {
                let mut transaction = c.begin().await?;
                transaction
                    .query_params_controlled("SELECT ? AS stop_class_marker", &[Value::Int(1)], &control)
                    .await
            },
        ),
        sends(
            "Transaction::execute_controlled",
            b"stop_class_rows",
            |c, control| async move { c.begin().await?.execute_controlled(WRITE, &control).await },
        ),
        sends(
            "Transaction::execute_params_controlled",
            b"stop_class_rows",
            |c, control| async move {
                let mut transaction = c.begin().await?;
                transaction
                    .execute_params_controlled(
                        "UPDATE stop_class_rows SET note = ?",
                        &[Value::Text("a".into())],
                        &control,
                    )
                    .await
            },
        ),
        sends(
            "Transaction::rollback_controlled",
            b"ROLLBACK",
            |c, control| async move { c.begin().await?.rollback_controlled(&control).await },
        ),
    ]
}

fn stop_cases() -> Vec<StopCase> {
    let mut cases = catalog_cases();
    cases.extend(table_metadata_cases());
    cases.extend(statement_cases());
    cases.extend(transaction_cases());
    cases
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
fn every_controlled_call_of_the_contract_has_a_mysql_stop_case() {
    let mut declared = controlled_calls(
        include_str!("../../../../core/src/connection.rs"),
        "pub trait Connection",
        "",
    );
    declared.extend(controlled_calls(
        include_str!("../../../../core/src/transaction.rs"),
        "pub trait Transaction",
        "Transaction::",
    ));
    let cases = stop_cases();
    let covered: BTreeSet<String> = cases.iter().map(|case| case.method.to_string()).collect();
    assert_eq!(covered, declared);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_stopped_call_asks_mysql_to_stop_and_retires_the_uncertain_connection() {
    let (_container, options) = start_mysql().await;
    let observer = connect(options.clone()).await;
    create_fixture(observer.as_ref()).await;
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
            Some(marker) => stop_once(&case, marker, &proxy, &connection, observer.as_ref()).await,
            None => sends_no_statement(&case, &proxy, &connection).await,
        };
        if let Err(failure) = outcome {
            failures.push(format!("{}: {failure}", case.method));
        }
        proxy.release();
        tokio::time::timeout(OBSERVE_FOR, connection.query("SELECT 1"))
            .await
            .unwrap_or_else(|_| panic!("the pool serves no statement after {}", case.method))
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
    observer: &dyn Connection,
) -> Result<(), String> {
    let cancel_requests = proxy.cancel_requests();
    proxy.withhold_reply_to(marker);
    let call = tokio::spawn((case.call)(
        connection.clone(),
        OperationControl::with_timeout(STOP_AFTER),
    ));
    proxy.wait_for_cancel_request(cancel_requests).await;
    let reached = proxy.withheld_replies() > 0;
    let while_withheld = connections(observer).await;
    proxy.release();
    let outcome = tokio::time::timeout(Duration::from_secs(20), call)
        .await
        .map_err(|_| "the call never returned".to_string())?
        .unwrap();
    let asked = proxy.cancel_requests() > cancel_requests;
    if !reached {
        return Err("the statement never reached the server, so the case proves nothing".into());
    }
    if !asked {
        return Err(format!("no KILL QUERY reached MySQL; the call returned {outcome:?}"));
    }
    if !matches!(outcome, Err(DriverError::OperationOutcomeUnknown { .. })) {
        return Err(format!("a withheld reply was reported as {outcome:?}"));
    }
    if !a_connection_retires(observer, &while_withheld).await {
        return Err("the connection with the uncertain statement went back to the pool".into());
    }
    Ok(())
}

async fn create_fixture(admin: &dyn Connection) {
    for sql in [
        "DROP TABLE IF EXISTS stop_class_rows",
        "CREATE TABLE stop_class_rows (id int AUTO_INCREMENT PRIMARY KEY, note text) ENGINE=InnoDB",
        "INSERT INTO stop_class_rows (note) VALUES ('kept')",
    ] {
        admin.execute(sql).await.unwrap();
    }
}

async fn connections(observer: &dyn Connection) -> BTreeSet<i64> {
    let result = observer
        .query("SELECT CAST(id AS SIGNED) FROM information_schema.processlist")
        .await
        .unwrap();
    result
        .rows
        .iter()
        .filter_map(|row| match row.first() {
            Some(Value::Int(id)) => Some(*id),
            _ => None,
        })
        .collect()
}

async fn a_connection_retires(observer: &dyn Connection, seen: &BTreeSet<i64>) -> bool {
    let deadline = tokio::time::Instant::now() + OBSERVE_FOR;
    loop {
        let now = connections(observer).await;
        if seen.difference(&now).next().is_some() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn statements_still_running(observer: &dyn Connection) -> i64 {
    let result = observer
        .query(
            "SELECT COUNT(*) FROM information_schema.processlist \
             WHERE info LIKE 'UPDATE stop_class_rows%' AND id <> CONNECTION_ID()",
        )
        .await
        .unwrap();
    match result.rows.first().and_then(|row| row.first()) {
        Some(Value::Int(count)) => *count,
        other => panic!("statement count is not an integer: {other:?}"),
    }
}

async fn open_transactions(observer: &dyn Connection) -> i64 {
    let result = observer
        .query("SELECT COUNT(*) FROM information_schema.innodb_trx")
        .await
        .unwrap();
    match result.rows.first().and_then(|row| row.first()) {
        Some(Value::Int(count)) => *count,
        other => panic!("transaction count is not an integer: {other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_mysql_save_stopped_while_it_waits_for_a_lock_is_rolled_back_on_the_server() {
    let (_container, options) = start_mysql().await;
    let observer = connect(options.clone()).await;
    create_fixture(observer.as_ref()).await;
    let connection = connect(options).await;
    let no_limit = OperationControl::new(tokio_util::sync::CancellationToken::new(), None);

    let mut lock = observer.begin().await.unwrap();
    lock.execute_controlled("UPDATE stop_class_rows SET note = 'locked' WHERE id = 1", &no_limit)
        .await
        .unwrap();
    let write = one_row_write();
    let error = connection
        .execute_in_transaction_checked_controlled(&write, &[0], &OperationControl::with_timeout(STOP_AFTER))
        .await
        .expect_err("a save behind a row lock cannot finish");
    assert!(
        matches!(&error, DriverError::Transaction { source, .. } if matches!(source.as_ref(), DriverError::TimedOut)),
        "MySQL confirmed the stop and the batch was rolled back, so the save is known not to have run: {error:?}"
    );
    assert_eq!(statements_still_running(observer.as_ref()).await, 0);
    assert_eq!(
        open_transactions(observer.as_ref()).await,
        1,
        "only the lock holder stays open"
    );

    let cancelled = tokio_util::sync::CancellationToken::new();
    let by_caller = OperationControl::new(cancelled.clone(), None);
    let (error, ()) = tokio::join!(
        connection.execute_in_transaction_checked_controlled(&write, &[0], &by_caller),
        async {
            tokio::time::sleep(STOP_AFTER).await;
            cancelled.cancel();
        }
    );
    let error = error.expect_err("a save behind a row lock cannot finish");
    assert!(
        matches!(&error, DriverError::Transaction { source, .. } if matches!(source.as_ref(), DriverError::Cancelled)),
        "{error:?}"
    );
    assert_eq!(statements_still_running(observer.as_ref()).await, 0);
    assert_eq!(
        open_transactions(observer.as_ref()).await,
        1,
        "only the lock holder stays open"
    );

    lock.rollback().await.unwrap();
    let note = connection
        .query("SELECT note FROM stop_class_rows WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(note.rows, vec![vec![Value::Text("kept".into())]]);
}

struct ProxyState {
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
        while let Ok(count) = client_read.read(&mut bytes).await {
            if count == 0 {
                break;
            }
            let chunk = &bytes[..count];
            outbound_state.forwarded.fetch_add(1, Ordering::SeqCst);
            if contains_marker(&mut unmatched_stop, chunk, STOP_REQUEST) {
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

#[tokio::test]
#[ignore = "requires docker"]
async fn a_controlled_mysql_transaction_begins_writes_and_ends() {
    let (_container, options) = start_mysql().await;
    let connection = connect(options).await;
    create_fixture(connection.as_ref()).await;
    let control = OperationControl::with_timeout(OBSERVE_FOR);

    let mut transaction = connection.begin_controlled(&control).await.unwrap();
    transaction
        .execute_controlled("UPDATE stop_class_rows SET note = 'discarded' WHERE id = 1", &control)
        .await
        .unwrap();
    transaction.rollback().await.unwrap();
    let mut transaction = connection.begin_controlled(&control).await.unwrap();
    transaction
        .execute_controlled("INSERT INTO stop_class_rows (note) VALUES ('committed')", &control)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    let notes = connection
        .query("SELECT note FROM stop_class_rows ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        notes.rows,
        vec![vec![Value::Text("kept".into())], vec![Value::Text("committed".into())]]
    );
    assert_eq!(open_transactions(connection.as_ref()).await, 0);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_uncertain_write_in_a_mysql_transaction_is_never_committed_later() {
    let (_container, options) = start_mysql().await;
    let observer = connect(options.clone()).await;
    create_fixture(observer.as_ref()).await;
    let connection = connect(options).await;
    let no_limit = OperationControl::new(tokio_util::sync::CancellationToken::new(), None);

    let mut lock = observer.begin().await.unwrap();
    lock.execute_controlled("UPDATE stop_class_rows SET note = 'locked' WHERE id = 1", &no_limit)
        .await
        .unwrap();
    let mut transaction = connection.begin().await.unwrap();
    let abandoned_write = tokio::time::timeout(
        STOP_AFTER,
        transaction.execute("UPDATE stop_class_rows SET note = 'uncertain' WHERE id = 1"),
    )
    .await;
    assert!(abandoned_write.is_err(), "the write behind a row lock cannot finish");
    let abandoned_rollback = tokio::time::timeout(STOP_AFTER, transaction.rollback()).await;
    assert!(
        abandoned_rollback.is_err(),
        "the rollback waits behind the pending write"
    );
    lock.rollback().await.unwrap();

    for _ in 0..8 {
        connection.begin().await.unwrap().commit().await.unwrap();
    }
    assert_eq!(open_transactions(observer.as_ref()).await, 0);
    let note = observer
        .query("SELECT note FROM stop_class_rows WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(note.rows, vec![vec![Value::Text("kept".into())]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_write_in_a_mysql_transaction_stopped_behind_a_lock_leaves_the_transaction_usable() {
    let (_container, options) = start_mysql().await;
    let observer = connect(options.clone()).await;
    create_fixture(observer.as_ref()).await;
    let connection = connect(options).await;
    let no_limit = OperationControl::new(tokio_util::sync::CancellationToken::new(), None);

    let mut lock = observer.begin().await.unwrap();
    lock.execute_controlled("UPDATE stop_class_rows SET note = 'locked' WHERE id = 1", &no_limit)
        .await
        .unwrap();
    let mut transaction = connection.begin().await.unwrap();
    let error = transaction
        .execute_controlled(
            "UPDATE stop_class_rows SET note = 'stopped' WHERE id = 1",
            &OperationControl::with_timeout(STOP_AFTER),
        )
        .await
        .expect_err("a write behind a row lock cannot finish");
    assert!(matches!(error, DriverError::TimedOut), "{error:?}");
    assert_eq!(statements_still_running(observer.as_ref()).await, 0);

    transaction
        .execute_controlled("INSERT INTO stop_class_rows (note) VALUES ('second')", &no_limit)
        .await
        .unwrap();
    transaction
        .rollback_controlled(&OperationControl::with_timeout(OBSERVE_FOR))
        .await
        .unwrap();
    lock.rollback().await.unwrap();
    assert_eq!(open_transactions(observer.as_ref()).await, 0);
    let notes = observer
        .query("SELECT note FROM stop_class_rows ORDER BY id")
        .await
        .unwrap();
    assert_eq!(notes.rows, vec![vec![Value::Text("kept".into())]]);
}
