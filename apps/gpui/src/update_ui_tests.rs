use bookie_app_services::update_check::{UpdateCheckError, check_with};

use super::update_status;

fn release(tag: &str) -> String {
    format!(r#"{{"tag_name":"{tag}","html_url":"https://example.invalid/{tag}","body":""}}"#)
}

#[test]
fn the_status_says_what_the_check_found() {
    let newer = check_with("0.2.7", || Ok(release("linux-v9.9.9")));
    assert!(update_status(&newer, "0.2.7").contains("9.9.9"));
    let same = check_with("0.2.7", || Ok(release("linux-v0.2.7")));
    assert!(update_status(&same, "0.2.7").contains("up to date"));
    assert!(update_status(&Err(UpdateCheckError::Network), "0.2.7").contains("could not reach"));
}
