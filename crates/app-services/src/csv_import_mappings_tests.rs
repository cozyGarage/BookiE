use super::*;

fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "TEXT".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn scope(connection_id: Uuid, database: &str, table: &str) -> CsvMappingScope {
    CsvMappingScope {
        connection_id,
        database: database.into(),
        driver_id: "postgres".into(),
        capabilities: DriverCapabilities::default(),
        schema: Some("public".into()),
        table: table.into(),
    }
}

fn headers(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[test]
fn restore_resolves_header_names_after_reordering_and_skips_added_columns() {
    let directory = tempfile::tempdir().unwrap();
    let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    let scope = scope(Uuid::new_v4(), "appdb", "users");
    store
        .save(
            &scope,
            &headers(&["id", "full name"]),
            &[column("id"), column("name")],
            &[None, Some(1)],
            true,
        )
        .unwrap();
    let restored = store
        .restore(
            &scope,
            &headers(&["full name", "id", "new source"]),
            &[column("id"), column("name"), column("created_at")],
            true,
        )
        .unwrap()
        .unwrap();
    assert_eq!(restored, [None, Some(0), None]);
}

#[test]
fn explicit_auto_increment_values_can_be_saved_and_restored() {
    let headers = headers(&["id", "name"]);
    let columns = [
        ColumnInfo {
            is_auto_increment: true,
            ..column("id")
        },
        column("name"),
        ColumnInfo {
            is_generated: true,
            ..column("computed")
        },
    ];
    let mapping = [Some(0), Some(1), None];
    let options = tablepro_core::import::CsvImportOptions::default();
    let sheet = tablepro_core::import::read_csv(b"id,name\n901,alpha\n", &options, None).unwrap();

    for driver_id in ["postgres", "mysql", "sqlite"] {
        let directory = tempfile::tempdir().unwrap();
        let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
        let mut scope = scope(Uuid::new_v4(), "appdb", "users");
        scope.driver_id = driver_id.into();
        scope.capabilities = DriverCapabilities {
            supports_explicit_auto_increment_values: true,
            ..DriverCapabilities::default()
        };

        store.save(&scope, &headers, &columns, &mapping, true).unwrap();
        let restored = store.restore(&scope, &headers, &columns, true).unwrap().unwrap();
        assert_eq!(restored, mapping, "{driver_id}");

        let plan = tablepro_core::import::build_insert_plan(
            &tablepro_core::import::ImportTarget {
                driver_id,
                capabilities: scope.capabilities,
                schema: None,
                table: "users",
                columns: &columns,
                mapping: &restored,
            },
            &sheet,
            &options,
        )
        .unwrap();

        assert_eq!(
            plan.columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "name"],
            "{driver_id}"
        );
        assert_eq!(
            plan.rows,
            [[
                tablepro_core::Value::Text("901".into()),
                tablepro_core::Value::Text("alpha".into())
            ]],
            "{driver_id}"
        );
        if driver_id == "postgres" {
            assert!(plan.statement.contains("OVERRIDING SYSTEM VALUE"));
        }
    }
}

#[test]
fn generated_values_remain_excluded_from_saved_mappings() {
    let directory = tempfile::tempdir().unwrap();
    let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    let scope = scope(Uuid::new_v4(), "appdb", "users");
    let headers = headers(&["computed"]);
    let generated = [ColumnInfo {
        is_generated: true,
        ..column("computed")
    }];

    assert!(store.save(&scope, &headers, &generated, &[Some(0)], true).is_err());

    store
        .save(&scope, &headers, &[column("computed")], &[Some(0)], true)
        .unwrap();
    assert!(store.restore(&scope, &headers, &generated, true).is_err());
}

#[test]
fn mssql_identity_values_cannot_be_saved_as_explicit_mappings() {
    let directory = tempfile::tempdir().unwrap();
    let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    let mut scope = scope(Uuid::new_v4(), "appdb", "users");
    scope.driver_id = "mssql".into();
    let columns = [ColumnInfo {
        is_auto_increment: true,
        ..column("id")
    }];

    assert!(
        store
            .save(&scope, &headers(&["id"]), &columns, &[Some(0)], true)
            .is_err()
    );
}

#[test]
fn a_saved_identity_mapping_is_refused_when_current_capability_no_longer_allows_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    let mut scope = scope(Uuid::new_v4(), "appdb", "users");
    scope.capabilities = DriverCapabilities {
        supports_explicit_auto_increment_values: true,
        ..DriverCapabilities::default()
    };
    let headers = headers(&["id"]);
    let columns = [ColumnInfo {
        is_auto_increment: true,
        ..column("id")
    }];

    store.save(&scope, &headers, &columns, &[Some(0)], true).unwrap();
    scope.capabilities = DriverCapabilities::default();

    assert!(store.restore(&scope, &headers, &columns, true).is_err());
}

#[test]
fn duplicate_headers_restore_by_name_and_occurrence_not_old_position() {
    let directory = tempfile::tempdir().unwrap();
    let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    let scope = scope(Uuid::new_v4(), "appdb", "tags");
    store
        .save(
            &scope,
            &headers(&["tag", "tag", "email"]),
            &[column("label")],
            &[Some(1)],
            true,
        )
        .unwrap();
    assert_eq!(
        store
            .restore(&scope, &headers(&["email", "tag", "tag"]), &[column("label")], true)
            .unwrap()
            .unwrap(),
        [Some(2)]
    );
}

#[test]
fn missing_source_headers_and_renamed_or_ambiguous_targets_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    let scope = scope(Uuid::new_v4(), "appdb", "users");
    store
        .save(&scope, &headers(&["full name"]), &[column("name")], &[Some(0)], true)
        .unwrap();
    assert!(
        store
            .restore(&scope, &headers(&["email"]), &[column("name")], true)
            .is_err()
    );
    assert!(
        store
            .restore(&scope, &headers(&["full name"]), &[column("display_name")], true)
            .is_err()
    );
    assert!(
        store
            .restore(
                &scope,
                &headers(&["full name"]),
                &[column("name"), column("name")],
                true
            )
            .is_err()
    );
}

#[test]
fn mappings_are_isolated_by_connection_and_database() {
    let directory = tempfile::tempdir().unwrap();
    let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    let connection_id = Uuid::new_v4();
    let original = scope(connection_id, "appdb", "users");
    store
        .save(
            &original,
            &headers(&["display name"]),
            &[column("name")],
            &[Some(0)],
            true,
        )
        .unwrap();
    assert!(
        store
            .restore(
                &scope(Uuid::new_v4(), "appdb", "users"),
                &headers(&["display name"]),
                &[column("name")],
                true
            )
            .unwrap()
            .is_none()
    );
    let mut other_schema = original.clone();
    other_schema.schema = Some("private".into());
    let mut other_table = original.clone();
    other_table.table = "accounts".into();
    let mut other_driver = original.clone();
    other_driver.driver_id = "mysql".into();
    for isolated in [other_schema, other_table, other_driver] {
        assert!(
            store
                .restore(&isolated, &headers(&["display name"]), &[column("name")], true)
                .unwrap()
                .is_none()
        );
    }
    assert!(
        store
            .restore(
                &scope(connection_id, "otherdb", "users"),
                &headers(&["display name"]),
                &[column("name")],
                true
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn mappings_without_headers_and_generated_values_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    let scope = scope(Uuid::new_v4(), "appdb", "users");
    assert!(
        store
            .save(&scope, &headers(&["Column 1"]), &[column("name")], &[Some(0)], false)
            .is_err()
    );
    assert!(
        store
            .restore(&scope, &headers(&["Column 1"]), &[column("name")], false)
            .is_err()
    );
    let generated = ColumnInfo {
        is_generated: true,
        ..column("id")
    };
    assert!(
        store
            .save(&scope, &headers(&["id"]), &[generated], &[Some(0)], true)
            .is_err()
    );
}

#[test]
fn forget_removes_only_the_scoped_mapping() {
    let directory = tempfile::tempdir().unwrap();
    let store = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    let scope = scope(Uuid::new_v4(), "appdb", "users");
    let mut other_scope = scope.clone();
    other_scope.table = "accounts".into();
    store
        .save(&scope, &headers(&["full name"]), &[column("name")], &[Some(0)], true)
        .unwrap();
    store
        .save(
            &other_scope,
            &headers(&["account name"]),
            &[column("name")],
            &[Some(0)],
            true,
        )
        .unwrap();
    assert!(store.forget(&scope).unwrap());
    assert!(!store.forget(&scope).unwrap());
    assert!(
        store
            .restore(&scope, &headers(&["full name"]), &[column("name")], true)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .restore(&other_scope, &headers(&["account name"]), &[column("name")], true)
            .unwrap()
            .is_some()
    );
}

#[test]
fn saved_mapping_survives_reopen_and_contains_names_not_row_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mappings.json");
    let scope = scope(Uuid::new_v4(), "appdb", "users");
    let store = CsvMappingStore::load_at(path.clone());
    store
        .save(&scope, &headers(&["full name"]), &[column("name")], &[Some(0)], true)
        .unwrap();
    store.flush().unwrap();
    let stored = std::fs::read_to_string(path).unwrap();
    assert!(stored.contains("full name"));
    assert!(stored.contains("name"));
    assert!(!stored.contains("private row value"));
    let reopened = CsvMappingStore::load_at(directory.path().join("mappings.json"));
    assert_eq!(
        reopened
            .restore(&scope, &headers(&["full name"]), &[column("name")], true)
            .unwrap()
            .unwrap(),
        [Some(0)]
    );
}
