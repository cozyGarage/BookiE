pub const WEBSITE: &str = "https://github.com/cozyGarage/BookiE";
pub const ISSUES_URL: &str = "https://github.com/cozyGarage/BookiE/issues";
pub const SUPPORT_URL: &str = "https://github.com/cozyGarage/BookiE/discussions";
pub const RELEASES_URL: &str = "https://github.com/cozyGarage/BookiE/releases";

pub fn about_summary(version: &str) -> String {
    crate::tr!("BookiE {version} · GNU AGPL-3.0 · {website}")
        .replace("{version}", version)
        .replace("{website}", WEBSITE)
}

#[cfg(test)]
#[path = "about_tests.rs"]
mod tests;
