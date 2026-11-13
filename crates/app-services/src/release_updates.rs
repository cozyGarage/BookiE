#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReleaseVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl ReleaseVersion {
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let without_tag = trimmed
            .strip_prefix("linux-v")
            .or_else(|| trimmed.strip_prefix('v'))
            .unwrap_or(trimmed);
        let mut parts = without_tag.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self { major, minor, patch })
    }

    pub fn display(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    pub fn is_newer_than(self, other: Self) -> bool {
        self > other
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhatsNewDecision {
    Show,
    SkipAndRemember,
    SkipKeepStored,
}

pub fn should_show_whats_new(stored: Option<&str>, running: &str) -> WhatsNewDecision {
    let Some(running_version) = ReleaseVersion::parse(running) else {
        return WhatsNewDecision::SkipKeepStored;
    };
    let Some(stored_raw) = stored.map(str::trim).filter(|value| !value.is_empty()) else {
        return WhatsNewDecision::Show;
    };
    let Some(stored_version) = ReleaseVersion::parse(stored_raw) else {
        return WhatsNewDecision::Show;
    };
    if running_version.is_newer_than(stored_version) {
        WhatsNewDecision::Show
    } else {
        WhatsNewDecision::SkipAndRemember
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestRelease {
    pub version: ReleaseVersion,
    pub tag_name: String,
    pub html_url: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckOutcome {
    UpToDate { latest: LatestRelease },
    UpdateAvailable { latest: LatestRelease },
    InvalidLatest,
}

pub fn compare_running_to_latest(running: &str, latest: &LatestRelease) -> UpdateCheckOutcome {
    let Some(running_version) = ReleaseVersion::parse(running) else {
        return UpdateCheckOutcome::InvalidLatest;
    };
    if latest.version.is_newer_than(running_version) {
        UpdateCheckOutcome::UpdateAvailable {
            latest: latest.clone(),
        }
    } else {
        UpdateCheckOutcome::UpToDate {
            latest: latest.clone(),
        }
    }
}

pub fn parse_github_release_json(body: &str) -> Option<LatestRelease> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("draft").and_then(|draft| draft.as_bool()) == Some(true) {
        return None;
    }
    if value.get("prerelease").and_then(|flag| flag.as_bool()) == Some(true) {
        return None;
    }
    let tag_name = value.get("tag_name")?.as_str()?.to_owned();
    let version = ReleaseVersion::parse(&tag_name)?;
    let html_url = value.get("html_url")?.as_str()?.to_owned();
    if html_url.is_empty() {
        return None;
    }
    let notes = value
        .get("body")
        .and_then(|body| body.as_str())
        .unwrap_or("")
        .to_owned();
    Some(LatestRelease {
        version,
        tag_name,
        html_url,
        body: notes,
    })
}

pub fn metainfo_release_notes(metainfo_xml: &str, version: &str) -> Option<String> {
    let target = ReleaseVersion::parse(version)?;
    let needle = format!("version=\"{}\"", target.display());
    let release_start = metainfo_xml.find(&needle)?;
    let after_attr = &metainfo_xml[release_start..];
    let block_start = after_attr.find("<description>")?;
    let block = &after_attr[block_start..];
    let inner_start = "<description>".len();
    let inner_end = block.find("</description>")?;
    let html = block[inner_start..inner_end].trim();
    if html.is_empty() {
        return None;
    }
    Some(html.to_owned())
}

pub const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/cozyGarage/BookiE/releases/latest";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_tags_and_plain_versions_parse() {
        assert_eq!(
            ReleaseVersion::parse("linux-v0.2.7"),
            Some(ReleaseVersion {
                major: 0,
                minor: 2,
                patch: 7
            })
        );
        assert_eq!(
            ReleaseVersion::parse("v0.2.7"),
            Some(ReleaseVersion {
                major: 0,
                minor: 2,
                patch: 7
            })
        );
        assert_eq!(
            ReleaseVersion::parse("0.2.7"),
            Some(ReleaseVersion {
                major: 0,
                minor: 2,
                patch: 7
            })
        );
        assert_eq!(ReleaseVersion::parse("0.2"), None);
        assert_eq!(ReleaseVersion::parse("linux-v0.2.7-rc1"), None);
    }

    #[test]
    fn newer_versions_compare_numerically() {
        let older = ReleaseVersion::parse("0.2.6").expect("parse");
        let newer = ReleaseVersion::parse("0.2.7").expect("parse");
        assert!(newer.is_newer_than(older));
        assert!(!older.is_newer_than(newer));
        assert!(!newer.is_newer_than(newer));
    }

    #[test]
    fn first_launch_and_upgrades_show_whats_new_once() {
        assert_eq!(
            should_show_whats_new(None, "0.2.7"),
            WhatsNewDecision::Show
        );
        assert_eq!(
            should_show_whats_new(Some(""), "0.2.7"),
            WhatsNewDecision::Show
        );
        assert_eq!(
            should_show_whats_new(Some("0.2.6"), "0.2.7"),
            WhatsNewDecision::Show
        );
        assert_eq!(
            should_show_whats_new(Some("0.2.7"), "0.2.7"),
            WhatsNewDecision::SkipAndRemember
        );
        assert_eq!(
            should_show_whats_new(Some("0.2.8"), "0.2.7"),
            WhatsNewDecision::SkipAndRemember
        );
    }

    #[test]
    fn github_latest_json_parses_stable_releases_only() {
        let body = concat!(
            "{",
            "\"tag_name\":\"linux-v0.2.8\",",
            "\"draft\":false,",
            "\"prerelease\":false,",
            "\"html_url\":\"https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.8\",",
            "\"body\":\"Fixed example.\"",
            "}"
        );
        let latest = parse_github_release_json(body).expect("parse");
        assert_eq!(latest.version.display(), "0.2.8");
        assert_eq!(
            latest.html_url,
            "https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.8"
        );
        assert!(latest.body.contains("example"));

        let draft = concat!(
            "{\"tag_name\":\"linux-v0.2.9\",\"draft\":true,\"prerelease\":false,",
            "\"html_url\":\"https://example.test\",\"body\":\"\"}"
        );
        assert_eq!(parse_github_release_json(draft), None);
        let pre = concat!(
            "{\"tag_name\":\"linux-v0.2.9\",\"draft\":false,\"prerelease\":true,",
            "\"html_url\":\"https://example.test\",\"body\":\"\"}"
        );
        assert_eq!(parse_github_release_json(pre), None);
    }

    #[test]
    fn update_check_distinguishes_available_and_current() {
        let latest = LatestRelease {
            version: ReleaseVersion::parse("0.2.8").expect("parse"),
            tag_name: "linux-v0.2.8".into(),
            html_url: "https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.8".into(),
            body: String::new(),
        };
        assert!(matches!(
            compare_running_to_latest("0.2.7", &latest),
            UpdateCheckOutcome::UpdateAvailable { .. }
        ));
        assert!(matches!(
            compare_running_to_latest("0.2.8", &latest),
            UpdateCheckOutcome::UpToDate { .. }
        ));
        assert!(matches!(
            compare_running_to_latest("0.2.9", &latest),
            UpdateCheckOutcome::UpToDate { .. }
        ));
    }

    #[test]
    fn metainfo_notes_extract_the_matching_release_html() {
        let xml = r#"
        <releases>
          <release version="0.2.7" type="stable" date="2026-11-12">
            <description><p>Release after 0.2.6: notes for seven.</p></description>
          </release>
          <release version="0.2.6" type="stable" date="2026-11-10">
            <description><p>Older.</p></description>
          </release>
        </releases>
        "#;
        assert_eq!(
            metainfo_release_notes(xml, "0.2.7").as_deref(),
            Some("<p>Release after 0.2.6: notes for seven.</p>")
        );
        assert_eq!(metainfo_release_notes(xml, "0.9.9"), None);
    }
}
