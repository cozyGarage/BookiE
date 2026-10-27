#[derive(Default)]
pub struct CatalogChanges {
    transaction: bool,
    pending: bool,
}

impl CatalogChanges {
    pub fn succeeded(&mut self, sql: &str, driver: &str) -> bool {
        use tablepro_policy::{CatalogEffect, catalog_effect};
        match catalog_effect(sql, driver) {
            CatalogEffect::Begin => {
                self.transaction = true;
                false
            }
            CatalogEffect::Rollback { chain } => {
                self.transaction = chain;
                self.pending = false;
                false
            }
            CatalogEffect::Commit { chain } => {
                self.transaction = chain;
                std::mem::take(&mut self.pending)
            }
            CatalogEffect::Ddl if self.transaction && !matches!(driver, "mysql" | "clickhouse") => {
                self.pending = true;
                false
            }
            CatalogEffect::Ddl => {
                self.transaction = false;
                self.pending = false;
                true
            }
            CatalogEffect::None => false,
        }
    }
    pub fn transaction_finished<E>(&mut self, outcome: &Result<tablepro_core::SessionCommitOutcome, E>) -> bool {
        let Ok(outcome) = outcome else {
            return false;
        };
        self.transaction = false;
        let changed = std::mem::take(&mut self.pending);
        matches!(outcome, tablepro_core::SessionCommitOutcome::Committed) && changed
    }
}

#[cfg(test)]
mod tests {
    use super::CatalogChanges;
    use tablepro_core::{DriverError, SessionCommitOutcome};

    fn pending_ddl() -> CatalogChanges {
        let mut changes = CatalogChanges::default();
        assert!(!changes.succeeded("BEGIN", "sqlite"));
        assert!(!changes.succeeded("CREATE TABLE pending_table (id integer)", "sqlite"));
        changes
    }

    fn committed() -> Result<SessionCommitOutcome, DriverError> {
        Ok(SessionCommitOutcome::Committed)
    }

    #[test]
    fn acknowledged_commit_publishes_pending_ddl_once() {
        let mut changes = pending_ddl();
        assert!(changes.transaction_finished(&committed()));
        assert!(!changes.transaction_finished(&committed()));
        assert!(changes.succeeded("CREATE TABLE later_table (id integer)", "sqlite"));
    }

    #[test]
    fn acknowledged_rollback_discards_pending_ddl_and_restores_autocommit() {
        let mut changes = pending_ddl();
        let outcome = Ok::<_, DriverError>(SessionCommitOutcome::RolledBack);
        assert!(!changes.transaction_finished(&outcome));
        assert!(!changes.transaction_finished(&committed()));
        assert!(changes.succeeded("CREATE TABLE later_table (id integer)", "sqlite"));
    }

    #[test]
    fn failed_commit_preserves_pending_ddl_for_retry() {
        for additional_ddl in [false, true] {
            let mut changes = pending_ddl();
            assert!(!changes.transaction_finished(&Err(DriverError::query("commit failed"))));
            if additional_ddl {
                assert!(!changes.succeeded("CREATE TABLE retry_table (id integer)", "sqlite"));
            }
            assert!(changes.transaction_finished(&committed()));
        }
    }

    #[test]
    fn interrupted_commit_preserves_pending_ddl_for_retry() {
        for timed_out in [false, true] {
            for additional_ddl in [false, true] {
                let mut changes = pending_ddl();
                let interruption = if timed_out {
                    DriverError::TimedOut
                } else {
                    DriverError::Cancelled
                };
                assert!(!changes.transaction_finished(&Err(interruption)));
                if additional_ddl {
                    assert!(!changes.succeeded("CREATE TABLE retry_table (id integer)", "sqlite"));
                }
                assert!(changes.transaction_finished(&committed()));
            }
        }
    }

    #[test]
    fn a_session_without_pending_ddl_does_not_publish_a_catalog_change() {
        let mut changes = CatalogChanges::default();
        assert!(!changes.succeeded("BEGIN", "sqlite"));
        assert!(!changes.transaction_finished(&committed()));
        assert!(changes.succeeded("CREATE TABLE later_table (id integer)", "sqlite"));
    }

    #[test]
    fn sql_commit_and_chain_keeps_the_new_transaction_open() {
        let mut changes = pending_ddl();
        assert!(changes.succeeded("COMMIT AND CHAIN", "postgres"));
        assert!(!changes.succeeded("CREATE TABLE next_table (id integer)", "postgres"));
        assert!(changes.transaction_finished(&committed()));
    }

    #[test]
    fn nontransactional_ddl_is_not_published_again_when_the_session_ends() {
        for driver in ["mysql", "clickhouse"] {
            let mut changes = CatalogChanges::default();
            assert!(!changes.succeeded("BEGIN", driver));
            assert!(changes.succeeded("CREATE TABLE immediate_table (id integer)", driver));
            assert!(!changes.transaction_finished(&committed()));
        }
    }
}
