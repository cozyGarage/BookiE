#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::OperationControl;

use crate::{connect, start_pg};

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(60))
}

#[tokio::test]
#[ignore = "requires docker"]
async fn query_batches_keep_server_reported_affected_rows_for_connection_and_session() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    connection
        .execute("CREATE TEMP TABLE affected_count (id integer PRIMARY KEY)")
        .await
        .unwrap();

    let inserted = connection
        .query_result_sets_controlled(
            "INSERT INTO affected_count SELECT generate_series(1, 3) RETURNING id",
            &[],
            &control(),
        )
        .await
        .unwrap();
    assert_eq!(inserted.rows_affected, Some(3));
    assert_eq!(inserted.retained_rows(), 3);

    let mut session = connection.open_session().await.unwrap();
    session
        .query_result_sets_params_controlled(
            "CREATE TEMP TABLE session_affected_count (id integer PRIMARY KEY)",
            &[],
            &control(),
        )
        .await
        .unwrap();
    session
        .query_result_sets_params_controlled(
            "INSERT INTO session_affected_count SELECT generate_series(1, 3)",
            &[],
            &control(),
        )
        .await
        .unwrap();
    let updated = session
        .query_result_sets_params_controlled(
            "UPDATE session_affected_count SET id = id + 10 WHERE id <= 3",
            &[],
            &control(),
        )
        .await
        .unwrap();
    assert_eq!(updated.rows_affected, Some(3));
    assert_eq!(updated.retained_rows(), 0);
}
