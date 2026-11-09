use super::database_service::{ConnectionIdentity, DatabaseService};
use bookie_core::TableInfo;
use bookie_policy::{PolicyGuard, Principal};
use std::sync::Arc;
use uuid::Uuid;

pub type Catalog = (Vec<TableInfo>, Vec<TableInfo>);
pub(crate) use bookie_app_services::catalog_reads::{
    list_objects_excluding, list_tables_excluding, list_views_excluding,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogOrigin {
    pub id: Uuid,
    pub identity: ConnectionIdentity,
}

pub use bookie_app_services::catalog_changes::CatalogChanges;

impl CatalogOrigin {
    pub fn capture(id: Option<Uuid>, database: &DatabaseService) -> Option<Self> {
        let id = id?;
        Some(Self {
            id,
            identity: database.identity(id)?,
        })
    }
    pub fn owns_window(&self, id: Option<Uuid>, database: &DatabaseService) -> bool {
        self.matches(id, database.identity(self.id).as_ref())
    }
    pub(crate) fn matches(&self, id: Option<Uuid>, identity: Option<&ConnectionIdentity>) -> bool {
        id == Some(self.id) && identity == Some(&self.identity)
    }
    pub fn connection(&self, database: &DatabaseService) -> Option<Arc<dyn Connection>> {
        let (connection, identity) = database.get_with_identity(self.id)?;
        (identity == self.identity).then_some(connection)
    }

    pub(crate) fn guard(&self, database: &DatabaseService) -> Option<Arc<PolicyGuard>> {
        let (guard, identity) = database.guard_with_identity(self.id, Principal::human_gui())?;
        (identity == self.identity).then_some(guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (Arc<dyn Connection>, ConnectionIdentity) {
        use bookie_core::DatabaseDriver;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let options = bookie_core::ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        };
        let connection: Arc<dyn Connection> =
            Arc::from(runtime.block_on(drivers_sqlite::SqliteDriver.connect(options)).unwrap());
        let identity = ConnectionIdentity::from_connection(&connection);
        (connection, identity)
    }

    #[test]
    fn a_result_from_an_older_session_of_the_same_connection_is_rejected() {
        let id = Uuid::new_v4();
        let (_old, old_identity) = session();
        let (_new, new_identity) = session();
        let origin = CatalogOrigin {
            id,
            identity: old_identity.clone(),
        };
        assert!(origin.matches(Some(id), Some(&old_identity)));
        assert!(
            !origin.matches(Some(id), Some(&new_identity)),
            "a database switch reuses the id"
        );
        assert!(!origin.matches(Some(Uuid::new_v4()), Some(&old_identity)));
        assert!(!origin.matches(None, None));
    }
    #[test]
    fn committed_ddl_refreshes_but_rollback_and_savepoint_do_not_publish() {
        let mut changes = CatalogChanges::default();
        assert!(!changes.succeeded("/* start */ BEGIN", "postgres"));
        assert!(!changes.succeeded("CREATE VIEW v AS SELECT 1", "postgres"));
        assert!(!changes.succeeded("ROLLBACK TO SAVEPOINT s", "postgres"));
        assert!(changes.succeeded("COMMIT AND CHAIN", "postgres"));
        assert!(!changes.succeeded("DROP VIEW v", "postgres"));
        assert!(!changes.succeeded("ROLLBACK", "postgres"));
        assert!(!changes.succeeded("COMMIT", "postgres"));
        assert!(changes.succeeded("CREATE VIEW v AS SELECT 1", "postgres"));
        assert!(!changes.succeeded("SELECT 'CREATE TABLE x'", "postgres"));
    }
}
