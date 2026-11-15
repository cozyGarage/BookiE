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

use crate::{connect, start_pg};

pub(crate) const APPLICATION: &str = "bookie_stop_class";
const CANCEL_REQUEST_CODE: [u8; 4] = [0x04, 0xd2, 0x16, 0x2e];
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

fn sends_nothing(method: &'static str) -> StopCase {
    StopCase {
        method,
        statement_marker: None,
        call: Box::new(|_, _| Box::pin(async { Ok(()) })),
    }
}

fn one_row_write() -> Vec<(String, Vec<Value>)> {
    vec![(
        "UPDATE stop_class_rows SET note = $1 WHERE id = 1".into(),
        vec![Value::Text("stopped".into())],
    )]
}

fn catalog_cases() -> Vec<StopCase> {
    vec![
        sends("list_collations_controlled", b"pg_collation", |c, control| async move {
            c.list_collations_controlled(&control).await
        }),
        sends("list_databases_controlled", b"datallowconn", |c, control| async move {
            c.list_databases_controlled(&control).await
        }),
        sends("list_tables_controlled", b"relkind", |c, control| async move {
            c.list_tables_controlled(&control).await
        }),
        sends(
            "list_tables_excluding_controlled",
            b"relkind",
            |c, control| async move { c.list_tables_excluding_controlled(&["hidden".into()], &control).await },
        ),
        sends("list_tables_in", b"relkind", |c, control| async move {
            c.list_tables_in(&["public".into()], &control).await
        }),
        sends("list_views_controlled", b"relkind", |c, control| async move {
            c.list_views_controlled(&control).await
        }),
        sends("list_views_excluding_controlled", b"relkind", |c, control| async move {
            c.list_views_excluding_controlled(&["hidden".into()], &control).await
        }),
        sends_nothing("lexical_settings_controlled"),
        sends("list_objects_controlled", b"pg_sequence", |c, control| async move {
            c.list_objects_controlled(CatalogObjectKind::Sequence, None, &control)
                .await
        }),
        sends(
            "list_objects_excluding_controlled",
            b"pg_sequence",
            |c, control| async move {
                c.list_objects_excluding_controlled(CatalogObjectKind::Sequence, None, &["hidden".into()], &control)
                    .await
            },
        ),
        sends(
            "routine_definition_controlled",
            b"pg_get_functiondef",
            |c, control| async move {
                let routine = RoutineRef {
                    schema: "public".into(),
                    name: "stop_class_routine".into(),
                    identity_arguments: None,
                };
                c.routine_definition_controlled(&routine, &control).await
            },
        ),
        sends(
            "server_version_controlled",
            b"server_version",
            |c, control| async move { c.server_version_controlled(&control).await },
        ),
    ]
}

fn table_metadata_cases() -> Vec<StopCase> {
    vec![
        sends("fetch_columns_controlled", b"attgenerated", |c, control| async move {
            c.fetch_columns_controlled(None, "stop_class_rows", &control).await
        }),
        sends("fetch_indexes_controlled", b"indisunique", |c, control| async move {
            c.fetch_indexes_controlled(None, "stop_class_rows", &control).await
        }),
        sends(
            "fetch_foreign_keys_controlled",
            b"confdeltype",
            |c, control| async move { c.fetch_foreign_keys_controlled(None, "stop_class_rows", &control).await },
        ),
        sends(
            "fetch_inbound_foreign_keys_controlled",
            b"source_columns",
            |c, control| async move {
                c.fetch_inbound_foreign_keys_controlled(None, "stop_class_rows", &control)
                    .await
            },
        ),
        sends(
            "fetch_view_editability_controlled",
            b"stop_class_view",
            |c, control| async move {
                c.fetch_view_editability_controlled(None, "stop_class_view", &control)
                    .await
            },
        ),
        sends(
            "fetch_enum_labels_controlled",
            b"enumsortorder",
            |c, control| async move {
                c.fetch_enum_labels_controlled("public", "stop_class_mood", &control)
                    .await
            },
        ),
        sends("fetch_rows_controlled", b"stop_class_rows", |c, control| async move {
            c.fetch_rows_controlled(None, "stop_class_rows", 0, 10, &control).await
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
                c.query_params_controlled("SELECT $1::int AS stop_class_marker", &[Value::Int(1)], &control)
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
                    "UPDATE stop_class_rows SET note = $1",
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
        sends(
            "synchronize_auto_increment_sequences_controlled",
            b"pg_get_serial_sequence",
            |c, control| async move {
                c.synchronize_auto_increment_sequences_controlled(None, "stop_class_rows", &["id".into()], &control)
                    .await
            },
        ),
        sends("begin_controlled", b"BEGIN\0", |c, control| async move {
            c.begin_controlled(&control).await
        }),
    ]
}

fn stop_cases() -> Vec<StopCase> {
    let mut cases = catalog_cases();
    cases.extend(table_metadata_cases());
    cases.extend(statement_cases());
    cases
}

#[test]
fn every_controlled_call_of_the_contract_has_a_stop_case() {
    let source = include_str!("../../../../core/src/connection.rs");
    let contract = source
        .split("pub trait Connection")
        .nth(1)
        .expect("the Connection trait")
        .split("mod controlled_default_tests")
        .next()
        .unwrap();
    let declared: BTreeSet<&str> = contract
        .split("async fn ")
        .skip(1)
        .filter(|rest| {
            let signature = rest.split(['{', ';']).next().unwrap_or_default();
            signature.contains("OperationControl")
        })
        .filter_map(|rest| rest.split('(').next())
        .map(str::trim)
        .collect();
    let cases = stop_cases();
    let covered: BTreeSet<&str> = cases.iter().map(|case| case.method).collect();
    assert_eq!(covered, declared);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_stopped_call_asks_postgres_to_stop_and_retires_the_uncertain_backend() {
    let (_container, options) = start_pg().await;
    let options = ConnectOptions {
        application_name: Some(APPLICATION.into()),
        ..options
    };
    let observer = connect(ConnectOptions {
        application_name: Some("bookie_stop_class_observer".into()),
        ..options.clone()
    })
    .await;
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
        let Some(marker) = case.statement_marker else {
            continue;
        };
        connection.query("SELECT 1").await.unwrap();
        if let Err(failure) = stop_once(&case, marker, &proxy, &connection, observer.as_ref()).await {
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
    let asked = proxy.wait_for_cancel_request(cancel_requests).await;
    let reached = proxy.withheld_replies() > 0;
    let while_withheld = backends(observer).await;
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
            "no cancel request reached PostgreSQL; the call returned {outcome:?}"
        ));
    }
    if !matches!(outcome, Err(DriverError::OperationOutcomeUnknown { .. })) {
        return Err(format!("a withheld reply was reported as {outcome:?}"));
    }
    if !a_backend_retires(observer, &while_withheld).await {
        return Err("the backend with the uncertain statement went back to the pool".into());
    }
    Ok(())
}

async fn create_fixture(admin: &dyn Connection) {
    for sql in [
        "DROP VIEW IF EXISTS stop_class_view",
        "DROP TABLE IF EXISTS stop_class_rows",
        "DROP TYPE IF EXISTS stop_class_mood",
        "DROP FUNCTION IF EXISTS stop_class_routine()",
        "CREATE TABLE stop_class_rows (id serial PRIMARY KEY, note text)",
        "INSERT INTO stop_class_rows (note) VALUES ('kept')",
        "CREATE VIEW stop_class_view AS SELECT id, note FROM stop_class_rows",
        "CREATE TYPE stop_class_mood AS ENUM ('calm')",
        "CREATE FUNCTION stop_class_routine() RETURNS int LANGUAGE sql AS 'SELECT 1'",
    ] {
        admin.execute(sql).await.unwrap();
    }
}

pub(crate) async fn backends(observer: &dyn Connection) -> BTreeSet<i64> {
    let result = observer
        .query_params(
            "SELECT pid::bigint FROM pg_stat_activity WHERE application_name = $1",
            &[Value::Text(APPLICATION.into())],
        )
        .await
        .unwrap();
    result
        .rows
        .iter()
        .filter_map(|row| match row.first() {
            Some(Value::Int(pid)) => Some(*pid),
            _ => None,
        })
        .collect()
}

pub(crate) async fn a_backend_retires(observer: &dyn Connection, before: &BTreeSet<i64>) -> bool {
    let deadline = tokio::time::Instant::now() + OBSERVE_FOR;
    loop {
        let now = backends(observer).await;
        if before.difference(&now).next().is_some() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_save_stopped_while_it_waits_for_a_lock_is_rolled_back_on_the_server() {
    let (_container, options) = start_pg().await;
    let options = ConnectOptions {
        application_name: Some(APPLICATION.into()),
        ..options
    };
    let observer = connect(ConnectOptions {
        application_name: Some("bookie_stop_class_observer".into()),
        ..options.clone()
    })
    .await;
    create_fixture(observer.as_ref()).await;
    let connection = connect(options).await;
    let no_limit = OperationControl::new(tokio_util::sync::CancellationToken::new(), None);

    let mut lock = observer.begin().await.unwrap();
    lock.execute_controlled("UPDATE stop_class_rows SET note = 'locked' WHERE id = 1", &no_limit)
        .await
        .unwrap();
    let error = connection
        .execute_in_transaction_checked_controlled(&one_row_write(), &[0], &OperationControl::with_timeout(STOP_AFTER))
        .await
        .expect_err("a save behind a row lock cannot finish");
    assert!(
        matches!(
            &error,
            DriverError::Transaction {
                statement_index: 0,
                source
            } if matches!(source.as_ref(), DriverError::TimedOut)
        ),
        "PostgreSQL confirmed the stop, so the save is known not to have run: {error:?}"
    );
    assert_eq!(waiting_backends(observer.as_ref()).await, 0);
    assert_eq!(open_transactions(observer.as_ref()).await, 0);

    let cancelled = tokio_util::sync::CancellationToken::new();
    let by_caller = OperationControl::new(cancelled.clone(), None);
    let write = one_row_write();
    let (error, ()) = tokio::join!(
        connection.execute_in_transaction_checked_controlled(&write, &[0], &by_caller),
        async {
            tokio::time::sleep(STOP_AFTER).await;
            cancelled.cancel();
        }
    );
    let error = error.expect_err("a save behind a row lock cannot finish");
    assert!(
        matches!(
            &error,
            DriverError::Transaction {
                statement_index: 0,
                source
            } if matches!(source.as_ref(), DriverError::Cancelled)
        ),
        "{error:?}"
    );
    assert_eq!(waiting_backends(observer.as_ref()).await, 0);
    assert_eq!(open_transactions(observer.as_ref()).await, 0);

    lock.rollback().await.unwrap();
    let note = connection
        .query("SELECT note FROM stop_class_rows WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(note.rows, vec![vec![Value::Text("kept".into())]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_browse_page_stopped_behind_a_table_lock_leaves_no_waiting_backend() {
    let (_container, options) = start_pg().await;
    let options = ConnectOptions {
        application_name: Some(APPLICATION.into()),
        ..options
    };
    let observer = connect(ConnectOptions {
        application_name: Some("bookie_stop_class_observer".into()),
        ..options.clone()
    })
    .await;
    create_fixture(observer.as_ref()).await;
    let connection = connect(options).await;
    let no_limit = OperationControl::new(tokio_util::sync::CancellationToken::new(), None);

    let mut lock = observer.begin().await.unwrap();
    lock.execute_controlled("LOCK TABLE stop_class_rows IN ACCESS EXCLUSIVE MODE", &no_limit)
        .await
        .unwrap();
    let error = connection
        .fetch_rows_controlled(
            None,
            "stop_class_rows",
            0,
            10,
            &OperationControl::with_timeout(STOP_AFTER),
        )
        .await
        .expect_err("a page behind a table lock cannot finish");
    assert!(matches!(error, DriverError::TimedOut), "{error:?}");
    assert_eq!(waiting_backends(observer.as_ref()).await, 0);
    lock.rollback().await.unwrap();

    let page = connection
        .fetch_rows_controlled(
            None,
            "stop_class_rows",
            0,
            10,
            &OperationControl::with_timeout(OBSERVE_FOR),
        )
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
}

async fn activity_count(observer: &dyn Connection, condition: &str) -> i64 {
    let sql = format!("SELECT count(*)::bigint FROM pg_stat_activity WHERE application_name = $1 AND {condition}");
    let result = observer
        .query_params(&sql, &[Value::Text(APPLICATION.into())])
        .await
        .unwrap();
    match result.rows.first().and_then(|row| row.first()) {
        Some(Value::Int(count)) => *count,
        other => panic!("activity count is not an integer: {other:?}"),
    }
}

async fn waiting_backends(observer: &dyn Connection) -> i64 {
    activity_count(observer, "wait_event_type = 'Lock'").await
}

async fn open_transactions(observer: &dyn Connection) -> i64 {
    activity_count(observer, "state LIKE 'idle in transaction%'").await
}

struct ProxyState {
    marker: std::sync::Mutex<Option<&'static [u8]>>,
    withheld: AtomicUsize,
    cancel_requests: AtomicUsize,
    hold: watch::Sender<bool>,
}

pub(crate) struct StallProxy {
    pub(crate) port: u16,
    state: Arc<ProxyState>,
}

impl StallProxy {
    pub(crate) async fn start(options: &ConnectOptions) -> Self {
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

    pub(crate) fn withhold_reply_to(&self, marker: &'static [u8]) {
        self.state.withheld.store(0, Ordering::SeqCst);
        self.state.hold.send_replace(true);
        *self.state.marker.lock().unwrap() = Some(marker);
    }

    pub(crate) fn release(&self) {
        *self.state.marker.lock().unwrap() = None;
        self.state.hold.send_replace(false);
    }

    pub(crate) fn withheld_replies(&self) -> usize {
        self.state.withheld.load(Ordering::SeqCst)
    }

    fn cancel_requests(&self) -> usize {
        self.state.cancel_requests.load(Ordering::SeqCst)
    }

    pub(crate) async fn wait_for_withheld_reply(&self) -> bool {
        let deadline = tokio::time::Instant::now() + OBSERVE_FOR;
        while tokio::time::Instant::now() < deadline {
            if self.withheld_replies() > 0 {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
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

pub(crate) fn contains_marker(unmatched: &mut Vec<u8>, bytes: &[u8], marker: &[u8]) -> bool {
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
        let mut first = true;
        let mut unmatched = Vec::new();
        while let Ok(count) = client_read.read(&mut bytes).await {
            if count == 0 {
                break;
            }
            let chunk = &bytes[..count];
            if std::mem::take(&mut first) && count == 16 && chunk[4..8] == CANCEL_REQUEST_CODE {
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
