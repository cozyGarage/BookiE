#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use bookie_app_services::window_sessions::GuardedSessionMap;
use bookie_core::DriverError;
use bookie_policy::{GuardContext, NullAuditSink, PolicyConfig, PolicyGuard, Principal};
use uuid::Uuid;

struct IdleConnection;

#[async_trait::async_trait]
impl bookie_core::Connection for IdleConnection {
    async fn list_tables(&self) -> Result<Vec<bookie_core::TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_columns(
        &self,
        _schema: Option<&str>,
        _table: &str,
    ) -> Result<Vec<bookie_core::ColumnInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn fetch_rows(
        &self,
        _schema: Option<&str>,
        _table: &str,
        _offset: u64,
        _limit: u64,
    ) -> Result<bookie_core::QueryResult, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn query(&self, _sql: &str) -> Result<bookie_core::QueryResult, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn execute(&self, _sql: &str) -> Result<bookie_core::ExecResult, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn execute_params(
        &self,
        _sql: &str,
        _params: &[bookie_core::Value],
    ) -> Result<bookie_core::ExecResult, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn execute_in_transaction(
        &self,
        _statements: &[(String, Vec<bookie_core::Value>)],
    ) -> Result<Vec<u64>, DriverError> {
        Err(DriverError::Internal("unused".into()))
    }

    async fn ping(&self) -> Result<(), DriverError> {
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

fn guard_for(connection_id: Uuid) -> Arc<PolicyGuard> {
    Arc::new(PolicyGuard::new(
        Arc::new(IdleConnection),
        GuardContext {
            connection_id,
            connection_name: "unit".into(),
            driver_id: "sqlite".into(),
            environment: bookie_core::Environment::Local,
            read_only: false,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(bookie_policy::AutoApproveSink),
            audit: Arc::new(NullAuditSink),
            audit_state: Arc::new(bookie_policy::AuditState::new()),
        },
    ))
}

#[test]
fn each_live_connection_holds_its_own_policy_guard() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut map = GuardedSessionMap::default();
    map.insert(first, guard_for(first));
    map.insert(second, guard_for(second));
    assert!(map.contains(first));
    assert!(map.contains(second));
    assert_eq!(map.len(), 2);
    assert!(std::ptr::eq(
        Arc::as_ptr(map.get(first).unwrap()),
        Arc::as_ptr(map.get(first).unwrap())
    ));
    assert!(!std::ptr::eq(
        Arc::as_ptr(map.get(first).unwrap()),
        Arc::as_ptr(map.get(second).unwrap())
    ));
    assert!(map.remove(first).is_some());
    assert!(!map.contains(first));
    assert!(map.contains(second));
}
