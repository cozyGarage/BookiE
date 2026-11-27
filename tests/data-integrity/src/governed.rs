use std::sync::Arc;
use std::time::Duration;

use bookie_core::{ConnectOptions, Connection, Environment, OperationControl};
use bookie_policy::{
    ApprovalSink, AuditState, AutoApproveSink, DenyApprovalSink, GuardContext, PolicyConfig, PolicyGuard, Principal,
};
use bookie_storage::AuditJournal;

pub struct Governed {
    pub guard: PolicyGuard,
    _journal: tempfile::TempDir,
}

pub fn control() -> OperationControl {
    OperationControl::with_timeout(Duration::from_secs(120))
}

pub async fn open(driver_id: &str, options: ConnectOptions, read_only: bool) -> Governed {
    open_with(driver_id, options, read_only, Arc::new(DenyApprovalSink)).await
}

pub async fn open_confirming(driver_id: &str, options: ConnectOptions) -> Governed {
    open_with(driver_id, options, false, Arc::new(AutoApproveSink)).await
}

pub async fn open_writable(driver_id: &str, options: ConnectOptions) -> Governed {
    open_confirming(driver_id, options).await
}

async fn open_with(
    driver_id: &str,
    options: ConnectOptions,
    read_only: bool,
    approval: Arc<dyn ApprovalSink>,
) -> Governed {
    bookie_transport::install_crypto_provider();
    let registry = drivers_registry::default_registry();
    let driver = registry
        .get(driver_id)
        .unwrap_or_else(|| panic!("driver {driver_id} is not registered"));
    let options = ConnectOptions { read_only, ..options };
    let connection: Arc<dyn Connection> = Arc::from(
        driver
            .connect(options)
            .await
            .unwrap_or_else(|error| panic!("connect {driver_id}: {error}")),
    );
    let journal_dir = tempfile::TempDir::new().expect("audit journal directory");
    let journal = AuditJournal::open_validated(journal_dir.path().join("audit.jsonl")).expect("open audit journal");
    let guard = PolicyGuard::new(
        connection,
        GuardContext {
            connection_id: uuid::Uuid::new_v4(),
            connection_name: format!("data-integrity {driver_id}"),
            driver_id: driver_id.to_string(),
            environment: Environment::Local,
            read_only,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval,
            audit: Arc::new(journal),
            audit_state: Arc::new(AuditState::new()),
        },
    );
    Governed {
        guard,
        _journal: journal_dir,
    }
}

impl Governed {
    pub async fn close(self) {
        let _ = Box::new(self.guard).close().await;
    }
}
