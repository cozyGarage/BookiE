use std::time::Duration;

use crate::release_updates::{self, GITHUB_LATEST_RELEASE_URL, UpdateCheckOutcome};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckError {
    Network,
    InvalidResponse,
}

pub const RUNNING_VERSION: &str = match option_env!("BOOKIE_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

pub type UpdateCheckResult = Result<UpdateCheckOutcome, UpdateCheckError>;

pub fn check_with(running_version: &str, fetch: impl FnOnce() -> Result<String, String>) -> UpdateCheckResult {
    let body = fetch().map_err(|_| UpdateCheckError::Network)?;
    let latest = release_updates::parse_github_release_json(&body).ok_or(UpdateCheckError::InvalidResponse)?;
    Ok(release_updates::compare_running_to_latest(running_version, &latest))
}

pub fn check_for_updates(running_version: &str) -> UpdateCheckResult {
    check_with(running_version, || fetch_latest_release_body(running_version))
}

fn fetch_latest_release_body(running_version: &str) -> Result<String, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(true)
        .build()
        .new_agent();
    let response = agent
        .get(GITHUB_LATEST_RELEASE_URL)
        .header("User-Agent", &format!("BookiE/{running_version}"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| error.to_string())?;
    response.into_body().read_to_string().map_err(|error| error.to_string())
}

pub fn outcome_message(result: &UpdateCheckResult, running_version: &str) -> String {
    match result {
        Ok(UpdateCheckOutcome::UpdateAvailable { latest }) => crate::tr!("Version {version} is available: {url}")
            .replace("{version}", &latest.version.display())
            .replace("{url}", &latest.html_url),
        Ok(UpdateCheckOutcome::UpToDate { .. }) => {
            crate::tr!("BookiE {version} is up to date").replace("{version}", running_version)
        }
        Ok(UpdateCheckOutcome::InvalidLatest) | Err(UpdateCheckError::InvalidResponse) => {
            crate::tr!("The update check got an answer it could not read")
        }
        Err(UpdateCheckError::Network) => crate::tr!("The update check could not reach the release server"),
    }
}

#[cfg(test)]
#[path = "update_check_tests.rs"]
mod tests;
