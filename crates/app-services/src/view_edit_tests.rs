use super::*;

fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "integer".into(),
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

fn editable(insertable: bool) -> ViewEditability {
    ViewEditability::Editable {
        key_columns: vec!["id".into()],
        read_only_columns: vec!["computed".into()],
        insertable,
    }
}

#[test]
fn an_updatable_view_marks_its_key_columns_and_locks_computed_columns() {
    let columns = vec![column("id"), column("label"), column("computed")];
    let plan = apply_view_editability(columns, &editable(true));
    assert_eq!(plan.restriction, None);
    assert!(plan.columns[0].primary_key);
    assert!(!plan.columns[1].primary_key);
    assert!(plan.columns[2].is_generated);
    assert!(!plan.columns[1].is_generated);
}

#[test]
fn a_view_that_refuses_inserts_stays_editable_with_a_distinct_reason() {
    let plan = apply_view_editability(vec![column("id")], &editable(false));
    assert!(plan.columns[0].primary_key);
    assert_eq!(plan.restriction, Some(ViewEditRestriction::InsertUnavailable));
    assert!(plan.restriction.unwrap().banner_text().is_none());
}

#[test]
fn a_keyless_view_and_a_non_updatable_view_are_refused_for_different_reasons() {
    let keyless = apply_view_editability(vec![column("id")], &ViewEditability::KeyNotExposed);
    let aggregate = apply_view_editability(vec![column("id")], &ViewEditability::NotUpdatable);
    assert_eq!(keyless.restriction, Some(ViewEditRestriction::KeyNotExposed));
    assert_eq!(aggregate.restriction, Some(ViewEditRestriction::NotUpdatable));
    assert_ne!(
        keyless.restriction.unwrap().banner_text(),
        aggregate.restriction.unwrap().banner_text()
    );
    assert!(!keyless.columns[0].primary_key && !aggregate.columns[0].primary_key);
}

#[test]
fn a_table_is_left_alone() {
    let plan = apply_view_editability(vec![column("id")], &ViewEditability::NotApplicable);
    assert_eq!(plan.restriction, None);
    assert!(!plan.columns[0].primary_key);
}
