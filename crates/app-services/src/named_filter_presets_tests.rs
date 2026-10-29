use super::*;
use tablepro_core::{FilterOp, FilterRule, FilterSet, FilterValue};
use uuid::Uuid;

fn filter(value: &str) -> FilterSet {
    FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single(value.into())),
        }],
        ..FilterSet::default()
    }
}

#[test]
fn presets_round_trip_and_remain_scoped_to_connection_schema_and_table() {
    let directory = tempfile::tempdir().unwrap();
    let store = NamedFilterPresetsStore::load_at(directory.path().join("presets.json"));
    let connection = Uuid::new_v4();
    store
        .save(connection, Some("public"), "jobs", "Failed", filter("failed"))
        .unwrap();
    store.flush().unwrap();
    let reopened = NamedFilterPresetsStore::load_at(directory.path().join("presets.json"));

    assert_eq!(reopened.list(connection, Some("public"), "jobs"), vec!["Failed"]);
    assert_eq!(
        reopened.load(connection, Some("public"), "jobs", "Failed"),
        Some(filter("failed"))
    );
    assert!(reopened.load(connection, Some("private"), "jobs", "Failed").is_none());
    assert!(reopened.load(connection, Some("public"), "users", "Failed").is_none());
    assert!(
        reopened
            .load(Uuid::new_v4(), Some("public"), "jobs", "Failed")
            .is_none()
    );
}

#[test]
fn names_are_trimmed_and_case_insensitive_collisions_replace_the_existing_preset() {
    let directory = tempfile::tempdir().unwrap();
    let store = NamedFilterPresetsStore::load_at(directory.path().join("presets.json"));
    let connection = Uuid::new_v4();
    store
        .save(connection, None, "jobs", " Failed ", filter("failed"))
        .unwrap();
    store.save(connection, None, "jobs", "failed", filter("open")).unwrap();

    assert_eq!(store.list(connection, None, "jobs"), vec!["Failed"]);
    assert_eq!(store.load(connection, None, "jobs", "FAILED"), Some(filter("open")));
}

#[test]
fn invalid_names_and_empty_filters_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let store = NamedFilterPresetsStore::load_at(directory.path().join("presets.json"));
    let connection = Uuid::new_v4();

    assert!(store.save(connection, None, "jobs", "  ", filter("failed")).is_err());
    assert!(
        store
            .save(
                connection,
                None,
                "jobs",
                &"x".repeat(MAX_NAME_CHARS + 1),
                filter("failed")
            )
            .is_err()
    );
    assert!(
        store
            .save(connection, None, "jobs", "Empty", FilterSet::default())
            .is_err()
    );
}

#[test]
fn raw_sql_is_refused_and_hostile_saved_filters_are_not_offered() {
    let directory = tempfile::tempdir().unwrap();
    let store = NamedFilterPresetsStore::load_at(directory.path().join("presets.json"));
    let connection = Uuid::new_v4();
    let mut raw = filter("failed");
    raw.extra_sql = Some("1=1; DROP TABLE jobs".into());
    assert!(store.save(connection, None, "jobs", "Unsafe", raw.clone()).is_err());
    store
        .file
        .update(|map| {
            map.entry(connection.to_string())
                .or_default()
                .entry(String::new())
                .or_default()
                .entry("jobs".into())
                .or_default()
                .insert(
                    "unsafe".into(),
                    StoredPreset {
                        name: "Unsafe".into(),
                        filter: raw,
                    },
                );
        })
        .unwrap();

    assert!(store.list(connection, None, "jobs").is_empty());
    assert!(store.load(connection, None, "jobs", "Unsafe").is_none());
}

#[test]
fn preset_count_is_bounded_without_discarding_existing_entries() {
    let directory = tempfile::tempdir().unwrap();
    let store = NamedFilterPresetsStore::load_at(directory.path().join("presets.json"));
    let connection = Uuid::new_v4();
    for index in 0..MAX_PRESETS_PER_TABLE {
        store
            .save(connection, None, "jobs", &format!("preset {index}"), filter("failed"))
            .unwrap();
    }

    assert!(
        store
            .save(connection, None, "jobs", "one too many", filter("open"))
            .is_err()
    );
    assert_eq!(store.list(connection, None, "jobs").len(), MAX_PRESETS_PER_TABLE);
}

#[test]
fn delete_removes_only_the_requested_preset() {
    let directory = tempfile::tempdir().unwrap();
    let store = NamedFilterPresetsStore::load_at(directory.path().join("presets.json"));
    let connection = Uuid::new_v4();
    store
        .save(connection, None, "jobs", "Failed", filter("failed"))
        .unwrap();
    store.save(connection, None, "jobs", "Open", filter("open")).unwrap();

    assert!(store.delete(connection, None, "jobs", "FAILED").unwrap());
    assert!(!store.delete(connection, None, "jobs", "missing").unwrap());
    assert_eq!(store.list(connection, None, "jobs"), vec!["Open"]);
}

#[test]
fn rename_preserves_filter_and_refuses_to_overwrite_another_name() {
    let directory = tempfile::tempdir().unwrap();
    let store = NamedFilterPresetsStore::load_at(directory.path().join("presets.json"));
    let connection = Uuid::new_v4();
    store
        .save(connection, None, "jobs", "Failed", filter("failed"))
        .unwrap();
    store.save(connection, None, "jobs", "Open", filter("open")).unwrap();

    assert!(store.rename(connection, None, "jobs", "Failed", "Open").is_err());
    store.rename(connection, None, "jobs", "Failed", "Errors").unwrap();
    assert_eq!(store.load(connection, None, "jobs", "Errors"), Some(filter("failed")));
    assert!(store.load(connection, None, "jobs", "Failed").is_none());
}
