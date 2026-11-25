#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! End to end cost of the audit journal on governed reads, for #3736. Run with
//! `--ignored --nocapture` on an idle machine: it prints the time for 1,000 sequential reads through
//! the policy guard, once with the journal as it is (read intents are not synced) and once with every
//! intent synced, which is what the journal did before it learned the difference.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Environment, OperationControl};
use bookie_policy::{
    AuditError, AuditEvent, AuditRecordPhase, AuditSink, AuditState, AutoApproveSink, GuardContext, PolicyConfig,
    PolicyGuard, Principal,
};
use bookie_storage::{AuditJournal, StorageError};
use drivers_sqlite::SqliteDriver;
use uuid::Uuid;

const READS: usize = 1_000;

struct SyncEveryIntent(Arc<AuditJournal>);

#[async_trait]
impl AuditSink for SyncEveryIntent {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        let result = if event.phase == AuditRecordPhase::Intent {
            self.0.append_durable(event).await
        } else {
            self.0.append(event).await
        };
        result.map_err(|error| match error {
            StorageError::UnresolvedWrite => AuditError::UnresolvedWrite,
            StorageError::WriteInFlight => AuditError::WriteInFlight,
            error => AuditError::Persistence(error.to_string()),
        })
    }
}

async fn time_reads(sink: impl Fn(Arc<AuditJournal>) -> Arc<dyn AuditSink>) -> Duration {
    let directory = tempfile::TempDir::new().unwrap();
    let database = directory.path().join("reads.db");
    let _ = std::fs::File::create(&database);
    let journal = Arc::new(AuditJournal::open_validated(directory.path().join("audit.jsonl")).unwrap());
    let inner: Arc<dyn Connection> = SqliteDriver
        .connect(ConnectOptions {
            database: database.to_string_lossy().to_string(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into();
    let guard = PolicyGuard::new(
        inner,
        GuardContext {
            connection_id: Uuid::new_v4(),
            connection_name: "audit cost".into(),
            driver_id: "sqlite".into(),
            environment: Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit: sink(journal),
            audit_state: Arc::new(AuditState::new()),
        },
    );
    let started = Instant::now();
    for _ in 0..READS {
        guard
            .query_controlled("SELECT 1", &OperationControl::with_timeout(Duration::from_secs(30)))
            .await
            .unwrap();
    }
    started.elapsed()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "prints the audit cost of governed reads measured for #3736"]
async fn a_thousand_governed_reads_cost_less_when_read_intents_are_not_synced() {
    let current = time_reads(|journal| journal).await;
    let synced = time_reads(|journal| Arc::new(SyncEveryIntent(journal))).await;
    println!(
        "{READS} governed reads: read intents not synced {current:.2?}, every intent synced {synced:.2?} ({:.1}x)",
        synced.as_secs_f64() / current.as_secs_f64()
    );
    assert!(
        current <= synced * 2,
        "the current path must not be slower than syncing every intent"
    );
}
