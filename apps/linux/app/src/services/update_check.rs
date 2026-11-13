use std::time::Duration;

use bookie_app_services::release_updates::{self, GITHUB_LATEST_RELEASE_URL, LatestRelease, UpdateCheckOutcome};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckError {
    Network,
    InvalidResponse,
}

pub type UpdateCheckResult = Result<UpdateCheckOutcome, UpdateCheckError>;

pub fn check_for_updates(running_version: &str) -> UpdateCheckResult {
    let body = fetch_latest_release_body().map_err(|_| UpdateCheckError::Network)?;
    let latest = release_updates::parse_github_release_json(&body).ok_or(UpdateCheckError::InvalidResponse)?;
    Ok(compare_fetched(running_version, &latest))
}

pub fn compare_fetched(running_version: &str, latest: &LatestRelease) -> UpdateCheckOutcome {
    release_updates::compare_running_to_latest(running_version, latest)
}

fn fetch_latest_release_body() -> Result<String, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(true)
        .build()
        .new_agent();
    let response = agent
        .get(GITHUB_LATEST_RELEASE_URL)
        .header("User-Agent", &format!("BookiE/{}", crate::config::VERSION))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| error.to_string())?;
    response.into_body().read_to_string().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_app_services::release_updates::{LatestRelease, ReleaseVersion, UpdateCheckOutcome};

    #[test]
    fn fixture_newer_release_is_reported_as_available() {
        let latest = LatestRelease {
            version: ReleaseVersion::parse("9.9.9").expect("parse"),
            tag_name: "linux-v9.9.9".into(),
            html_url: "https://github.com/cozyGarage/BookiE/releases/tag/linux-v9.9.9".into(),
            body: String::new(),
        };
        assert!(matches!(
            compare_fetched("0.2.7", &latest),
            UpdateCheckOutcome::UpdateAvailable { .. }
        ));
    }

    #[test]
    fn fixture_same_release_is_up_to_date() {
        let latest = LatestRelease {
            version: ReleaseVersion::parse("0.2.7").expect("parse"),
            tag_name: "linux-v0.2.7".into(),
            html_url: "https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.7".into(),
            body: String::new(),
        };
        assert!(matches!(
            compare_fetched("0.2.7", &latest),
            UpdateCheckOutcome::UpToDate { .. }
        ));
    }
}
