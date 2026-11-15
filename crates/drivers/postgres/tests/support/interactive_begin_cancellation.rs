#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use bookie_core::{ConnectOptions, Connection, DriverError, OperationControl};

use crate::server_stop_class::{APPLICATION, StallProxy, a_backend_retires, backends, contains_marker};
use crate::{connect, start_pg};

const BEGIN: &[u8] = b"BEGIN\0";

#[test]
fn recognizes_begin_split_across_socket_reads() {
    let mut unmatched = Vec::new();
    assert!(!contains_marker(&mut unmatched, b"BE", BEGIN));
    assert!(contains_marker(&mut unmatched, b"GIN\0", BEGIN));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_interrupted_interactive_begin_retires_the_uncertain_backend() {
    let (_container, options) = start_pg().await;
    let options = ConnectOptions {
        application_name: Some(APPLICATION.into()),
        ..options
    };
    let observer = connect(ConnectOptions {
        application_name: Some("bookie_begin_observer".into()),
        ..options.clone()
    })
    .await;
    let proxy = StallProxy::start(&options).await;
    let connection: Arc<dyn Connection> = Arc::from(
        connect(ConnectOptions {
            host: "127.0.0.1".into(),
            port: proxy.port,
            ..options
        })
        .await,
    );
    proxy.withhold_reply_to(BEGIN);
    let begin_connection = connection.clone();
    let begin = tokio::spawn(async move {
        let control = OperationControl::with_timeout(Duration::from_millis(300));
        begin_connection.begin_controlled(&control).await.map(|_| ())
    });
    assert!(proxy.wait_for_withheld_reply().await, "BEGIN never reached PostgreSQL");
    let while_withheld = backends(observer.as_ref()).await;
    let error = begin
        .await
        .unwrap()
        .expect_err("the timed-out BEGIN cannot be reported as successful");
    proxy.release();
    assert!(
        matches!(error, DriverError::OperationOutcomeUnknown { .. }),
        "{error:?}"
    );

    assert!(a_backend_retires(observer.as_ref(), &while_withheld).await);
    connection.query("SELECT 1").await.unwrap();
}
