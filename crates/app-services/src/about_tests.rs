use super::{ISSUES_URL, RELEASES_URL, SUPPORT_URL, WEBSITE, about_summary};

#[test]
fn the_summary_names_the_app_version_licence_and_home() {
    let text = about_summary("0.2.7");
    assert!(text.starts_with("BookiE 0.2.7"), "{text}");
    assert!(text.contains("AGPL-3.0"), "{text}");
    assert!(text.contains(WEBSITE), "{text}");
    assert!(!text.contains('{'), "{text}");
}

#[test]
fn every_link_points_at_the_project_repository() {
    for url in [WEBSITE, ISSUES_URL, SUPPORT_URL, RELEASES_URL] {
        assert!(url.starts_with("https://github.com/cozyGarage/BookiE"), "{url}");
    }
}
