#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bookie_core::{Connection, DriverError, OperationControl};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::{connect, start_pg};

#[test]
fn recognizes_begin_split_across_socket_reads() {
    let mut buffered = Vec::new();
    assert!(!contains_begin(&mut buffered, b"BE"));
    assert!(contains_begin(&mut buffered, b"GIN\0"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_interrupted_interactive_begin_retires_the_uncertain_backend() {
    let (_container, options) = start_pg().await;
    let upstream = tokio::net::lookup_host((options.host.as_str(), options.port))
        .await
        .unwrap()
        .next()
        .unwrap();
    let (port, begin_reply, release_reply) = begin_reply_proxy(upstream).await;
    let connection: Arc<dyn Connection> = Arc::from(
        connect(bookie_core::ConnectOptions {
            host: "127.0.0.1".into(),
            port,
            ..options.clone()
        })
        .await,
    );
    let original_pid = backend_pid(connection.as_ref()).await;
    let begin_connection = connection.clone();
    let begin_task = tokio::spawn(async move {
        let control = OperationControl::with_timeout(Duration::from_secs(1));
        begin_connection.begin_controlled(&control).await
    });

    tokio::time::timeout(Duration::from_secs(5), begin_reply)
        .await
        .expect("BEGIN reaches PostgreSQL")
        .expect("proxy reports the BEGIN response");
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    release_reply.send(()).unwrap();
    let error = match begin_task.await.unwrap() {
        Ok(_) => panic!("the timed-out BEGIN cannot be reported as successful"),
        Err(error) => error,
    };
    assert!(
        matches!(error, DriverError::OperationOutcomeUnknown { .. }),
        "{error:?}"
    );

    let recovered_pid = backend_pid(connection.as_ref()).await;
    assert_ne!(recovered_pid, original_pid);
    let observer = connect(options).await;
    let state = observer
        .query_params(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid = $1)",
            &[bookie_core::Value::Int(i64::from(original_pid))],
        )
        .await
        .unwrap();
    assert_eq!(state.rows, vec![vec![bookie_core::Value::Bool(false)]]);
}

async fn backend_pid(connection: &dyn Connection) -> i32 {
    let result = connection.query("SELECT pg_backend_pid()::bigint").await.unwrap();
    let Some(bookie_core::Value::Int(pid)) = result.rows.first().and_then(|row| row.first()) else {
        panic!("PostgreSQL backend PID is not an integer")
    };
    i32::try_from(*pid).unwrap()
}

async fn begin_reply_proxy(upstream: SocketAddr) -> (u16, oneshot::Receiver<()>, oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (begin_tx, begin_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (client, _) = listener.accept().await.unwrap();
        tokio::spawn(relay_begin_connection(client, upstream, begin_tx, release_rx));
        while let Ok((client, _)) = listener.accept().await {
            tokio::spawn(relay_connection(client, upstream));
        }
    });
    (port, begin_rx, release_tx)
}

fn contains_begin(buffered: &mut Vec<u8>, bytes: &[u8]) -> bool {
    buffered.extend_from_slice(bytes);
    let found = buffered.windows(6).any(|window| window == b"BEGIN\0");
    let discard = buffered.len().saturating_sub(5);
    buffered.drain(..discard);
    found
}

async fn relay_connection(mut client: TcpStream, upstream: SocketAddr) {
    if let Ok(mut server) = TcpStream::connect(upstream).await {
        let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
    }
}

async fn relay_begin_connection(
    client: TcpStream,
    upstream: SocketAddr,
    begin_tx: oneshot::Sender<()>,
    release_rx: oneshot::Receiver<()>,
) {
    let Ok(server) = TcpStream::connect(upstream).await else {
        return;
    };
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    let begin_sent = Arc::new(AtomicBool::new(false));
    let outbound_flag = begin_sent.clone();
    let outbound = tokio::spawn(async move {
        let mut bytes = [0; 8192];
        let mut begin_bytes = Vec::with_capacity(8192 + 5);
        while let Ok(count) = client_read.read(&mut bytes).await {
            if count == 0 {
                break;
            }
            if contains_begin(&mut begin_bytes, &bytes[..count]) {
                outbound_flag.store(true, Ordering::SeqCst);
            }
            if server_write.write_all(&bytes[..count]).await.is_err() {
                break;
            }
        }
    });
    let mut release_rx = Some(release_rx);
    let mut begin_tx = Some(begin_tx);
    let mut bytes = [0; 8192];
    while let Ok(count) = server_read.read(&mut bytes).await {
        if count == 0 {
            break;
        }
        if begin_sent.swap(false, Ordering::SeqCst) {
            if let Some(sender) = begin_tx.take() {
                let _ = sender.send(());
            }
            if let Some(receiver) = release_rx.take() {
                let _ = receiver.await;
            }
        }
        if client_write.write_all(&bytes[..count]).await.is_err() {
            break;
        }
    }
    outbound.abort();
}
