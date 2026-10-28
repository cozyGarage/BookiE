use super::*;

#[test]
fn default_prod_denies_agent_writes() {
    let p = PolicyConfig::default();
    let env = p.for_environment(Environment::Prod);
    assert_eq!(env.agent_writes, WritePolicy::Deny);
    assert!(env.human_approve_writes);
}

#[test]
fn round_trip_toml() {
    let p = PolicyConfig::default();
    let text = toml::to_string_pretty(&p).unwrap();
    let back: PolicyConfig = toml::from_str(&text).unwrap();
    assert_eq!(back.for_environment(Environment::Prod).agent_writes, WritePolicy::Deny);
}

#[test]
fn partial_prod_policy_inherits_secure_defaults() {
    let policy: PolicyConfig = toml::from_str(
        r#"
                [environments.prod]
                human_approve_ddl = false
            "#,
    )
    .unwrap();
    let prod = policy.for_environment(Environment::Prod);
    assert_eq!(prod.agent_writes, WritePolicy::Deny);
    assert!(prod.human_approve_writes);
    assert!(!prod.human_allow_unaudited_writes);
    assert!(!prod.human_approve_ddl);
    assert_eq!(prod.blast_radius_max_rows, Some(1_000));
    assert!(prod.mask_agent_results);
}

#[test]
fn partial_connection_override_inherits_environment_policy() {
    let id = "9ab424f0-76ae-4fec-931f-2346549b6ca3";
    let policy: PolicyConfig = toml::from_str(&format!(
        r#"
                [connection_overrides."{id}"]
                human_approve_ddl = false
            "#
    ))
    .unwrap();
    let prod = policy.for_connection(id, Environment::Prod);
    assert_eq!(prod.agent_writes, WritePolicy::Deny);
    assert!(prod.human_approve_writes);
    assert!(!prod.human_approve_ddl);
}

#[test]
fn local_unaudited_writes_require_explicit_opt_in() {
    let default_policy = PolicyConfig::default();
    assert!(
        !default_policy
            .for_environment(Environment::Local)
            .human_allow_unaudited_writes
    );

    let policy: PolicyConfig = toml::from_str(
        r#"
                [environments.local]
                human_allow_unaudited_writes = true
            "#,
    )
    .unwrap();
    assert!(policy.for_environment(Environment::Local).human_allow_unaudited_writes);
    assert!(!policy.for_environment(Environment::Prod).human_allow_unaudited_writes);
}

#[test]
fn omitted_mask_patterns_keep_sensitive_defaults() {
    let policy: PolicyConfig = toml::from_str("").unwrap();
    assert!(!policy.effective_mask_patterns().is_empty());
}

#[test]
fn a_corrupt_policy_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::write(&path, "this is not toml [[[").unwrap();
    assert!(load_from_path(&path).is_err());
}

#[test]
fn an_explicit_empty_mask_list_keeps_sensitive_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::write(&path, "mask_patterns = []").unwrap();
    let policy = load_from_path(&path).unwrap();
    assert!(!policy.effective_mask_patterns().is_empty());
}

#[test]
fn a_custom_mask_pattern_adds_to_the_defaults_instead_of_replacing_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::write(&path, r#"mask_patterns = [{ pattern = "*internal*" }]"#).unwrap();
    let policy = load_from_path(&path).unwrap();
    let patterns = policy.effective_mask_patterns();
    assert!(patterns.contains(&"*internal*".to_string()));
    for default_pattern in crate::mask::DEFAULT_SENSITIVE_PATTERNS {
        assert!(
            patterns.iter().any(|p| p == default_pattern),
            "custom pattern discarded default {default_pattern}"
        );
    }
}

#[test]
fn an_invalid_glob_mask_pattern_is_refused_at_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::write(&path, r#"mask_patterns = [{ pattern = "*pan[*" }]"#).unwrap();
    assert!(load_from_path(&path).is_err());
}

#[test]
fn a_misspelled_environment_name_is_refused_at_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::write(
        &path,
        r#"
                [environments.production]
                human_approve_ddl = false
            "#,
    )
    .unwrap();
    let error = load_from_path(&path).unwrap_err();
    assert!(error.contains("production"), "{error}");
}

#[test]
fn an_unknown_field_in_an_environment_override_is_refused_at_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::write(
        &path,
        r#"
                [environments.prod]
                human_approve_ddll = false
            "#,
    )
    .unwrap();
    assert!(load_from_path(&path).is_err());
}

#[test]
fn a_connection_override_key_is_case_normalised_so_it_still_matches_at_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    let id = uuid::Uuid::new_v4();
    std::fs::write(
        &path,
        format!(
            r#"
                [connection_overrides."{}"]
                human_approve_ddl = false
                "#,
            id.to_string().to_uppercase()
        ),
    )
    .unwrap();
    let policy = load_from_path(&path).unwrap();
    let prod = policy.for_connection(&id.to_string(), Environment::Prod);
    assert!(
        !prod.human_approve_ddl,
        "an uppercase connection id in the file must still match a lowercase lookup"
    );
}

#[test]
fn a_connection_override_key_that_is_not_a_uuid_is_refused_at_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::write(
        &path,
        r#"
                [connection_overrides.not-a-uuid]
                human_approve_ddl = false
            "#,
    )
    .unwrap();
    assert!(load_from_path(&path).is_err());
}
