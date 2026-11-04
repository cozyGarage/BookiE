use super::*;
use crate::openssh::OpenSshTimeouts;
use crate::openssh::session::ssh_config_text;

const MODERN: [&str; 13] = [
    "ssh-ed25519",
    "ecdsa-sha2-nistp256",
    "rsa-sha2-512",
    "rsa-sha2-256",
    "chacha20-poly1305@openssh.com",
    "aes256-gcm@openssh.com",
    "aes128-gcm@openssh.com",
    "aes256-ctr",
    "curve25519-sha256",
    "diffie-hellman-group16-sha512",
    "sntrup761x25519-sha512",
    "hmac-sha2-256-etm@openssh.com",
    "hmac-sha2-512",
];

#[test]
fn every_policy_value_only_removes_from_the_defaults() {
    for (keyword, value) in REMOVED_ALGORITHMS {
        assert!(value.starts_with('-'), "{keyword} must subtract from the defaults");
        assert!(!value.contains(' '), "{keyword}");
    }
}

#[test]
fn no_modern_algorithm_is_removed() {
    for (keyword, value) in REMOVED_ALGORITHMS {
        for removed in value.split(',') {
            let removed = removed.trim_start_matches('-');
            assert!(!MODERN.contains(&removed), "{keyword} removes {removed}");
        }
    }
}

#[test]
fn the_weak_classes_of_the_issue_are_all_removed() {
    let removed: String = REMOVED_ALGORITHMS
        .iter()
        .map(|(_, value)| *value)
        .collect::<Vec<_>>()
        .join(",");
    for weak in [
        "ssh-rsa",
        "3des-cbc",
        "blowfish-cbc",
        "arcfour",
        "hmac-sha1",
        "diffie-hellman-group1-sha1",
    ] {
        assert!(
            removed.split(',').any(|name| name.trim_start_matches('-') == weak),
            "{weak}"
        );
    }
}

#[test]
fn the_generated_config_applies_the_policy_before_any_user_file_is_included() {
    let text = ssh_config_text(&OpenSshTimeouts::default());
    let first_include = text.find("Include ").unwrap();
    for (keyword, value) in REMOVED_ALGORITHMS {
        let line = format!("{keyword}={value}\n");
        let position = text.find(&line).unwrap_or_else(|| panic!("missing {keyword}"));
        assert!(position < first_include, "{keyword} must come before the first Include");
    }
}
