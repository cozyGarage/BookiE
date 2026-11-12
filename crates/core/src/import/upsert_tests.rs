use super::*;
use crate::driver::DriverCapabilities;

fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.to_owned(),
        data_type: "text".to_owned(),
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

fn index(name: &str, columns: &[&str], unique: bool, primary: bool) -> IndexInfo {
    IndexInfo {
        name: name.to_owned(),
        columns: columns.iter().map(|name| (*name).to_owned()).collect(),
        unique,
        primary,
        predicate: None,
        constraint: None,
        has_expression_key: false,
    }
}

fn plan(driver_id: &str, columns: &[&str]) -> InsertPlan {
    let columns: Vec<ColumnInfo> = columns.iter().map(|name| column(name)).collect();
    InsertPlan {
        statement: format!("INSERT INTO {} VALUES", quote_ident(driver_id, "people")),
        columns,
        rows: Vec::new(),
    }
}

fn target(driver_id: &str, style: ImportUpsertStyle) -> ImportTarget<'_> {
    ImportTarget {
        driver_id,
        capabilities: DriverCapabilities {
            import_upsert: style,
            ..DriverCapabilities::default()
        },
        schema: None,
        table: "people",
        columns: &[],
        mapping: &[],
    }
}

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn on_conflict_engines_update_every_non_key_column_from_excluded() {
    for driver in ["postgres", "sqlite", "duckdb"] {
        let built = plan(driver, &["id", "name", "email"]);
        let target = target(driver, ImportUpsertStyle::OnConflict);
        let upserted = apply_upsert(built.clone(), &target, &keys(&["id"])).expect("upsert");
        assert_eq!(
            upserted.statement,
            format!(
                "{} ON CONFLICT (\"id\") DO UPDATE SET \"name\" = EXCLUDED.\"name\", \"email\" = EXCLUDED.\"email\"",
                built.statement
            ),
            "{driver}"
        );
    }
}

#[test]
fn composite_keys_are_listed_in_the_order_chosen() {
    let built = plan("postgres", &["tenant", "id", "name"]);
    let target = target("postgres", ImportUpsertStyle::OnConflict);
    let upserted = apply_upsert(built, &target, &keys(&["tenant", "id"])).expect("upsert");
    assert!(
        upserted
            .statement
            .ends_with("ON CONFLICT (\"tenant\", \"id\") DO UPDATE SET \"name\" = EXCLUDED.\"name\"")
    );
}

#[test]
fn mysql_uses_on_duplicate_key_update_with_backtick_quoting() {
    let built = plan("mysql", &["id", "name"]);
    let target = target("mysql", ImportUpsertStyle::OnDuplicateKey);
    let upserted = apply_upsert(built, &target, &keys(&["id"])).expect("upsert");
    assert!(
        upserted
            .statement
            .ends_with(" ON DUPLICATE KEY UPDATE `name` = VALUES(`name`)")
    );
}

#[test]
fn hostile_column_names_stay_inside_their_quotes() {
    let hostile = "x\"; DROP TABLE people; --";
    let built = plan("postgres", &["id", hostile]);
    let target = target("postgres", ImportUpsertStyle::OnConflict);
    let upserted = apply_upsert(built, &target, &keys(&["id"])).expect("upsert");
    assert!(
        upserted
            .statement
            .contains("\"x\"\"; DROP TABLE people; --\" = EXCLUDED.\"x\"\"; DROP TABLE people; --\"")
    );
    let mysql = plan("mysql", &["id", "a`b"]);
    let target = self::target("mysql", ImportUpsertStyle::OnDuplicateKey);
    let upserted = apply_upsert(mysql, &target, &keys(&["id"])).expect("upsert");
    assert!(upserted.statement.ends_with("`a``b` = VALUES(`a``b`)"));
}

#[test]
fn a_key_that_is_not_a_valid_identifier_is_refused() {
    let built = plan("postgres", &["id", "name"]);
    let target = target("postgres", ImportUpsertStyle::OnConflict);
    assert!(matches!(
        apply_upsert(built.clone(), &target, &keys(&["bad\nname"])),
        Err(PlanError::Ident(_))
    ));
    assert!(matches!(
        apply_upsert(built, &target, &keys(&[""])),
        Err(PlanError::Ident(_))
    ));
}

#[test]
fn a_key_outside_the_import_is_refused() {
    let built = plan("postgres", &["name"]);
    let target = target("postgres", ImportUpsertStyle::OnConflict);
    assert_eq!(
        apply_upsert(built, &target, &keys(&["id"])),
        Err(PlanError::ConflictColumnNotImported("id".to_owned()))
    );
}

#[test]
fn missing_duplicate_and_all_key_columns_are_refused() {
    let built = plan("postgres", &["id", "name"]);
    let target = target("postgres", ImportUpsertStyle::OnConflict);
    assert_eq!(apply_upsert(built.clone(), &target, &[]), Err(PlanError::NoConflictKey));
    assert_eq!(
        apply_upsert(built.clone(), &target, &keys(&["id", "id"])),
        Err(PlanError::DuplicateConflictColumn("id".to_owned()))
    );
    assert_eq!(
        apply_upsert(built, &target, &keys(&["id", "name"])),
        Err(PlanError::NothingToUpdate)
    );
}

#[test]
fn engines_without_the_capability_refuse_the_mode() {
    let built = plan("mssql", &["id", "name"]);
    let target = target("mssql", ImportUpsertStyle::Unsupported);
    assert_eq!(
        apply_upsert(built, &target, &keys(&["id"])),
        Err(PlanError::UpsertUnsupported)
    );
}

#[test]
fn the_primary_key_leads_and_only_plain_unique_indexes_follow() {
    let mut partial = index("partial_email", &["email"], true, false);
    partial.predicate = Some("deleted = false".to_owned());
    let mut expression = index("lower_email", &["email"], true, false);
    expression.has_expression_key = true;
    let indexes = vec![
        index("people_name_key", &["name"], true, false),
        index("people_pkey", &["id"], true, true),
        index("plain", &["name", "email"], false, false),
        partial,
        expression,
        index("people_name_key_again", &["name"], true, false),
    ];
    let found = conflict_key_candidates(&[column("id"), column("name")], &indexes);
    let summary: Vec<(String, Vec<String>, bool)> = found
        .into_iter()
        .map(|key| (key.label, key.columns, key.primary))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("Primary key".to_owned(), keys(&["id"]), true),
            ("people_name_key".to_owned(), keys(&["name"]), false),
        ]
    );
}

#[test]
fn a_table_without_any_key_offers_none() {
    assert!(conflict_key_candidates(&[column("id")], &[]).is_empty());
}

#[test]
fn the_primary_key_falls_back_to_the_column_flags() {
    let mut id = column("id");
    id.primary_key = true;
    let found = conflict_key_candidates(&[id, column("name")], &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].columns, keys(&["id"]));
}

#[test]
fn the_insert_mode_leaves_the_plan_alone_and_the_update_mode_extends_it() {
    let built = plan("postgres", &["id", "name"]);
    let target = target("postgres", ImportUpsertStyle::OnConflict);
    let plain = WriteMode::Insert.apply(built.clone(), &target).expect("insert");
    assert_eq!(plain, built);
    assert!(!WriteMode::Insert.updates_existing_rows());
    let mode = WriteMode::UpdateOnConflict(keys(&["id"]));
    assert!(mode.updates_existing_rows());
    assert!(
        mode.apply(built, &target)
            .expect("upsert")
            .statement
            .contains("ON CONFLICT")
    );
}
