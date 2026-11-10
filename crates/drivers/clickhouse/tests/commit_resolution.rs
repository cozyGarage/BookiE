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
