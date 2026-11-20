use crate::release_updates::UpdateCheckOutcome;

use super::{RUNNING_VERSION, UpdateCheckError, check_with, outcome_message};

fn release(tag: &str) -> String {
    format!(
        r#"{{"tag_name":"{tag}","html_url":"https://example.invalid/releases/{tag}","body":"Notes","draft":false,"prerelease":false}}"#
    )
}

#[test]
fn a_newer_release_is_reported_as_available() {
    let outcome = check_with("0.2.7", || Ok(release("linux-v9.9.9"))).unwrap();
    assert!(matches!(outcome, UpdateCheckOutcome::UpdateAvailable { .. }));
}

#[test]
fn the_same_release_is_up_to_date() {
    let outcome = check_with("0.2.7", || Ok(release("linux-v0.2.7"))).unwrap();
    assert!(matches!(outcome, UpdateCheckOutcome::UpToDate { .. }));
}

#[test]
fn a_failed_request_is_a_network_error_and_a_bad_body_an_invalid_response() {
    assert_eq!(
        check_with("0.2.7", || Err("timeout".into())),
        Err(UpdateCheckError::Network)
    );
    assert_eq!(
        check_with("0.2.7", || Ok("not json".into())),
        Err(UpdateCheckError::InvalidResponse)
    );
    let prerelease = r#"{"tag_name":"linux-v9.0.0","html_url":"https://x","prerelease":true}"#;
    assert_eq!(
        check_with("0.2.7", || Ok(prerelease.into())),
        Err(UpdateCheckError::InvalidResponse),
        "a prerelease is never offered"
    );
}

#[test]
fn the_message_names_the_version_and_where_to_get_it() {
    let available = check_with("0.2.7", || Ok(release("linux-v9.9.9")));
    let text = outcome_message(&available, "0.2.7");
    assert!(
        text.contains("9.9.9") && text.contains("https://example.invalid/releases/linux-v9.9.9"),
        "{text}"
    );
    let current = check_with("0.2.7", || Ok(release("linux-v0.2.7")));
    assert!(outcome_message(&current, "0.2.7").contains("up to date"));
    assert!(outcome_message(&Err(UpdateCheckError::Network), "0.2.7").contains("could not reach"));
    assert!(!outcome_message(&Err(UpdateCheckError::InvalidResponse), "0.2.7").contains('{'));
}

#[test]
fn the_running_version_is_a_release_version_the_comparison_can_read() {
    assert!(
        crate::release_updates::ReleaseVersion::parse(RUNNING_VERSION).is_some(),
        "{RUNNING_VERSION}"
    );
}
