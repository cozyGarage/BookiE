#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(dead_code)]
#[path = "support/clickhouse.rs"]
mod clickhouse;

use bookie_core::{CommitResolution, Connection};
use clickhouse::start_clickhouse;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_tagged_write_resolves_as_applied_refused_or_unknown() {
    let (_container, opts) = start_clickhouse().await;
    let conn = drivers_clickhouse::open(opts).await.expect("open");
    conn.execute("CREATE TABLE commit_probe (id UInt32) ENGINE = MergeTree ORDER BY id")
        .await
        .expect("create table");

    let applied = drivers_clickhouse::commit_token();
    conn.execute_with_token("INSERT INTO commit_probe VALUES (1)", &applied)
        .await
        .expect("insert");
    assert_eq!(
        conn.resolve_commit(&applied).await.expect("resolve"),
        CommitResolution::Applied
    );

    let refused = drivers_clickhouse::commit_token();
    assert!(
        conn.execute_with_token("INSERT INTO no_such_table VALUES (1)", &refused)
            .await
            .is_err()
    );
    assert_eq!(
        conn.resolve_commit(&refused).await.expect("resolve"),
        CommitResolution::NotApplied
    );

    let never_ran = drivers_clickhouse::commit_token();
    assert_eq!(
        conn.resolve_commit(&never_ran).await.expect("resolve"),
        CommitResolution::Unresolvable
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_controlled_write_reports_the_token_that_resolves_it() {
    let (_container, opts) = start_clickhouse().await;
    let conn = drivers_clickhouse::open(opts).await.expect("open");
    conn.execute("CREATE TABLE IF NOT EXISTS commit_probe_ctl (id UInt32) ENGINE = MergeTree ORDER BY id")
        .await
        .expect("create table");
    let control = bookie_core::OperationControl::new(tokio_util::sync::CancellationToken::new(), None);

    let (rows, token) = conn
        .execute_controlled_with_token("INSERT INTO commit_probe_ctl VALUES (7)", &control)
        .await
        .expect("insert");
    assert_eq!(rows.rows_affected, 1);
    let token = token.expect("a ClickHouse write reports its token");
    assert_eq!(conn.resolve_commit(&token).await.expect("resolve"), CommitResolution::Applied);

    let (error, token) = conn
        .execute_controlled_with_token("INSERT INTO no_such_table_ctl VALUES (1)", &control)
        .await
        .expect_err("refused");
    assert!(!matches!(error, bookie_core::DriverError::Cancelled));
    let token = token.expect("a refused write still reports its token");
    assert_eq!(conn.resolve_commit(&token).await.expect("resolve"), CommitResolution::NotApplied);
}
