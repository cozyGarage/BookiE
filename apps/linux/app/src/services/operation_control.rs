use crate::services::preferences::PreferencesStore;

pub use bookie_app_services::operation_control::{
    MAX_QUERY_TIMEOUT_SECS, bounded, bounded_with, longer_timeout_secs,
};

pub fn configured_timeout_secs(preferences: &PreferencesStore) -> u32 {
    preferences.load().query_timeout_secs
}

pub fn timeout_for(
    preferences: &PreferencesStore,
    database: &crate::services::database_service::DatabaseService,
    connection: Option<uuid::Uuid>,
) -> u32 {
    let own = connection
        .and_then(|id| database.metadata(id))
        .and_then(|metadata| metadata.query_timeout_secs);
    bookie_app_services::operation_control::effective_timeout(own, configured_timeout_secs(preferences))
}
