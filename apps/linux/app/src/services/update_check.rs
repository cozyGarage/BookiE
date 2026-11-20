pub use bookie_app_services::update_check::{UpdateCheckError, UpdateCheckResult};

pub fn check_for_updates(running_version: &str) -> UpdateCheckResult {
    bookie_app_services::update_check::check_for_updates(running_version)
}
