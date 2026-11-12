#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bookie_app_services::value_search::{
    ConnectionSource, SearchLimits, SearchOptions, SearchReport, SearchRequest, StopReason, TableStatus, TableTarget,
    run_search,
};
use bookie_core::{ConnectOptions, DatabaseDriver, Environment, SearchMode};
use bookie_policy::{
    AuditError, AuditEvent, AuditRecordPhase, AuditSink, AuditState, AuditTerminalStatus, AutoApproveSink,
    GuardContext, PolicyConfig, PolicyGuard, Principal,
};
use drivers_postgres::PgDriver;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct CapturingAudit {
    events: Mutex<Vec<AuditEvent>>,
    fail: bool,
}

#[async_trait]
impl AuditSink for CapturingAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        if self.fail {
            return Err(AuditError::Unavailable("journal unavailable".into()));
        }
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

async fn start_pg() -> (ContainerAsync<Postgres>, ConnectOptions) {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("start postgres container");
    let opts = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(5432).await.unwrap(),
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    };
    (container, opts)
}

async fn seed(opts: &ConnectOptions) {
    let raw = PgDriver.connect(opts.clone()).await.unwrap();
    for statement in [
        "CREATE TABLE people (id integer PRIMARY KEY, name text, bio text, age integer)",
        "INSERT INTO people VALUES (1, 'Ada Lovelace', 'mathematician', 36), (2, 'Grace', 'admiral and ADA fan', 42), (3, 'Linus', NULL, 42)",
        "CREATE TABLE orders (id integer PRIMARY KEY, note text)",
        "INSERT INTO orders VALUES (1, 'ship to Ada'), (2, '100% cotton_shirt')",
        "CREATE TABLE blobs (id integer PRIMARY KEY, data bytea)",
        "INSERT INTO blobs VALUES (1, 'ada'::bytea)",
        "CREATE SCHEMA archive",
        "CREATE TABLE archive.people (id integer PRIMARY KEY, name text)",
        "INSERT INTO archive.people VALUES (1, 'old ada')",
    ] {
        raw.execute(statement).await.unwrap();
    }
}

fn guard_for(
    opts: &ConnectOptions,
    environment: Environment,
    audit: Arc<CapturingAudit>,
) -> impl std::future::Future<Output = PolicyGuard> {
    let opts = opts.clone();
    async move {
        let raw = PgDriver.connect(opts).await.unwrap();
        PolicyGuard::new(
            Arc::from(raw),
            GuardContext {
                connection_id: Uuid::nil(),
                connection_name: "value-search-test".into(),
                driver_id: "postgres".into(),
                environment,
                read_only: false,
                principal: Principal::human_gui(),
                policy: Arc::new(PolicyConfig::default()),
                approval: Arc::new(AutoApproveSink),
                audit,
                audit_state: Arc::new(AuditState::new()),
            },
        )
    }
}

fn audit(fail: bool) -> Arc<CapturingAudit> {
    Arc::new(CapturingAudit {
        events: Mutex::new(Vec::new()),
        fail,
    })
}

fn targets() -> Vec<TableTarget> {
    [
        (None, "people"),
        (None, "orders"),
        (None, "blobs"),
        (Some("archive"), "people"),
    ]
    .into_iter()
    .map(|(schema, name): (Option<&str>, &str)| TableTarget {
        schema: schema.map(str::to_owned),
        name: name.to_owned(),
    })
    .collect()
}

async fn search(
    guard: &PolicyGuard,
    term: &str,
    mode: SearchMode,
    include_non_text: bool,
    limits: SearchLimits,
    token: &CancellationToken,
) -> SearchReport {
    run_search(
        &ConnectionSource(guard),
        SearchRequest {
            driver_id: "postgres",
            tables: targets(),
            term,
            options: SearchOptions { mode, include_non_text },
            limits,
        },
        token,
        |_| {},
    )
    .await
    .unwrap()
}

fn outcomes(audit: &CapturingAudit) -> Vec<AuditEvent> {
    audit
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.phase == AuditRecordPhase::Outcome)
        .cloned()
        .collect()
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_search_finds_matches_by_column_and_audits_every_statement_as_a_read() {
    let (_container, opts) = start_pg().await;
    seed(&opts).await;
    let sink = audit(false);
    let guard = guard_for(&opts, Environment::Prod, sink.clone()).await;

    let report = search(
        &guard,
        "ada",
        SearchMode::Contains,
        false,
        SearchLimits::default(),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(report.stopped, None);
    let hit_tables: Vec<String> = report.with_hits().map(|table| table.target.label()).collect();
    assert_eq!(hit_tables, ["people", "orders", "archive.people"]);
    let people = report.with_hits().next().unwrap();
    assert_eq!(people.hits.len(), 2);
    assert_eq!(people.matched_columns, ["name", "bio"]);
    let first = &people.hits[0];
    assert_eq!(first.shown[0].0, "id");
    assert_eq!(first.matched, ["name"]);
    let blobs = report.tables.iter().find(|table| table.target.name == "blobs").unwrap();
    assert_eq!(blobs.status, TableStatus::NothingToSearch);

    let events = outcomes(&sink);
    assert!(!events.is_empty());
    assert!(
        events
            .iter()
            .all(|event| event.terminal_status == AuditTerminalStatus::Succeeded)
    );
    assert!(events.len() >= targets().len());
    assert!(events.iter().all(|event| event.redacted_sql == "[REDACTED]"));
    assert!(
        events
            .iter()
            .all(|event| !format!("{event:?}").to_lowercase().contains("%ada%"))
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn wildcards_in_the_term_match_literally_and_a_hostile_term_changes_nothing() {
    let (_container, opts) = start_pg().await;
    seed(&opts).await;
    let guard = guard_for(&opts, Environment::Local, audit(false)).await;
    let token = CancellationToken::new();

    let percent = search(
        &guard,
        "100%",
        SearchMode::Contains,
        false,
        SearchLimits::default(),
        &token,
    )
    .await;
    assert_eq!(
        percent
            .with_hits()
            .map(|table| table.target.label())
            .collect::<Vec<_>>(),
        ["orders"]
    );
    let underscore = search(
        &guard,
        "cotton_s",
        SearchMode::Contains,
        false,
        SearchLimits::default(),
        &token,
    )
    .await;
    assert_eq!(underscore.with_hits().count(), 1);
    let wildcard_only = search(
        &guard,
        "c_tton",
        SearchMode::Contains,
        false,
        SearchLimits::default(),
        &token,
    )
    .await;
    assert_eq!(wildcard_only.with_hits().count(), 0);

    let hostile = "x'; DROP TABLE people; --";
    let report = search(
        &guard,
        hostile,
        SearchMode::Contains,
        false,
        SearchLimits::default(),
        &token,
    )
    .await;
    assert_eq!(report.with_hits().count(), 0);
    let raw = PgDriver.connect(opts).await.unwrap();
    assert_eq!(raw.query("SELECT count(*) FROM people").await.unwrap().rows.len(), 1);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_exact_search_can_include_numeric_columns() {
    let (_container, opts) = start_pg().await;
    seed(&opts).await;
    let guard = guard_for(&opts, Environment::Local, audit(false)).await;
    let token = CancellationToken::new();

    let text_only = search(&guard, "42", SearchMode::Exact, false, SearchLimits::default(), &token).await;
    assert_eq!(text_only.with_hits().count(), 0);
    let numeric = search(&guard, "42", SearchMode::Exact, true, SearchLimits::default(), &token).await;
    let people = numeric.with_hits().next().unwrap();
    assert_eq!(people.matched_columns, ["age"]);
    assert_eq!(people.hits.len(), 2);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_policy_denial_stops_the_search_before_any_table_is_read() {
    let (_container, opts) = start_pg().await;
    seed(&opts).await;
    let sink = audit(true);
    let guard = guard_for(&opts, Environment::Prod, sink.clone()).await;

    let report = search(
        &guard,
        "ada",
        SearchMode::Contains,
        false,
        SearchLimits::default(),
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(report.stopped, Some(StopReason::Denied(_))));
    assert!(report.tables.is_empty());
    assert_eq!(report.tables_not_reached, 4);
    assert_eq!(report.with_hits().count(), 0);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn cancelling_a_blocked_search_ends_the_statement_as_cancelled_in_the_audit() {
    let (_container, opts) = start_pg().await;
    seed(&opts).await;
    let sink = audit(false);
    let guard = guard_for(&opts, Environment::Prod, sink.clone()).await;
    let locker = PgDriver.connect(opts.clone()).await.unwrap();
    let mut held = locker.begin().await.unwrap();
    held.execute("LOCK TABLE people IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();

    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(700)).await;
        canceller.cancel();
    });
    let report = search(
        &guard,
        "ada",
        SearchMode::Contains,
        false,
        SearchLimits::default(),
        &token,
    )
    .await;

    assert_eq!(report.stopped, Some(StopReason::Cancelled));
    assert_eq!(report.tables_not_reached, 4);
    let cancelled: Vec<AuditEvent> = outcomes(&sink)
        .into_iter()
        .filter(|event| event.terminal_status == AuditTerminalStatus::Cancelled)
        .collect();
    assert_eq!(cancelled.len(), 1);
    assert!(cancelled[0].targets.iter().any(|target| target.contains("people")));
    held.rollback().await.unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_table_that_exceeds_its_time_budget_is_audited_as_timed_out_and_the_rest_are_searched() {
    let (_container, opts) = start_pg().await;
    seed(&opts).await;
    let sink = audit(false);
    let guard = guard_for(&opts, Environment::Prod, sink.clone()).await;
    let locker = PgDriver.connect(opts.clone()).await.unwrap();
    let mut held = locker.begin().await.unwrap();
    held.execute("LOCK TABLE people IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();
    let limits = SearchLimits {
        table_timeout: Duration::from_millis(600),
        run_timeout: Duration::from_secs(60),
    };

    let report = search(
        &guard,
        "ada",
        SearchMode::Contains,
        false,
        limits,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(report.stopped, None);
    assert!(matches!(report.tables[0].status, TableStatus::Failed(_)));
    assert!(
        report
            .tables
            .iter()
            .any(|table| table.target.name == "orders" && !table.hits.is_empty())
    );
    assert!(
        outcomes(&sink)
            .iter()
            .any(|event| event.terminal_status == AuditTerminalStatus::TimedOut)
    );
    held.rollback().await.unwrap();
}
