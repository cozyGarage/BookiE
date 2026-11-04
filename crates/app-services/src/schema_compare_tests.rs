use super::*;
use bookie_core::ColumnInfo;

fn column(name: &str, data_type: &str, primary_key: bool, nullable: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn table(schema: &str, name: &str, columns: Vec<ColumnInfo>) -> TableRead {
    TableRead {
        schema: Some(schema.into()),
        name: name.into(),
        columns,
        foreign_keys: Vec::new(),
    }
}

fn people(schema: &str) -> TableRead {
    table(
        schema,
        "people",
        vec![
            column("id", "integer", true, false),
            column("name", "text", false, true),
        ],
    )
}

fn compare(current: &[TableRead], desired: &[TableRead]) -> SchemaCompare {
    compare_schemas("postgres", Some("a"), current, Some("b"), desired).unwrap()
}

#[test]
fn identical_schemas_give_an_empty_diff_and_a_script_with_no_statements() {
    let result = compare(&[people("a")], &[people("b")]);
    assert!(result.is_identical());
    assert!(
        result
            .script
            .lines()
            .all(|line| line.is_empty() || line.starts_with("--")),
        "{}",
        result.script
    );
    assert!(result.script.contains("nothing to change"));
}

#[test]
fn a_table_only_in_the_desired_schema_is_created_in_the_schema_being_changed() {
    let result = compare(&[], &[people("b")]);
    assert_eq!(result.added, ["people"]);
    assert!(
        result.script.contains("CREATE TABLE \"a\".\"people\""),
        "{}",
        result.script
    );
    assert!(
        !result.script.contains("\"b\"."),
        "the script must not touch the desired schema"
    );
}

#[test]
fn a_new_column_becomes_an_alter_and_an_unchanged_one_is_left_alone() {
    let mut desired = people("b");
    desired.columns.push(column("email", "text", false, true));
    let result = compare(&[people("a")], &[desired]);
    assert!(result.added.is_empty() && result.dropped.is_empty());
    assert_eq!(result.altered.len(), 1);
    assert_eq!(result.altered[0].table, "people");
    assert!(result.script.contains("ADD COLUMN \"email\""), "{}", result.script);
    assert!(
        !result.script.contains("\"name\""),
        "an unchanged column is not touched: {}",
        result.script
    );
}

#[test]
fn a_changed_column_type_is_altered() {
    let mut desired = people("b");
    desired.columns[1].data_type = "varchar(40)".into();
    let result = compare(&[people("a")], &[desired]);
    assert_eq!(result.altered.len(), 1);
    assert!(result.script.contains("ALTER COLUMN \"name\""), "{}", result.script);
}

#[test]
fn a_table_only_in_the_current_schema_is_dropped_last_and_marked_destructive() {
    let extra = table("a", "legacy", vec![column("id", "integer", true, false)]);
    let mut desired = people("b");
    desired.columns.push(column("email", "text", false, true));
    let result = compare(&[extra, people("a")], &[desired]);
    assert_eq!(result.dropped, ["legacy"]);
    let alter = result.script.find("ADD COLUMN").unwrap();
    let drop = result.script.find("DROP TABLE \"a\".\"legacy\"").unwrap();
    assert!(alter < drop, "drops come after changes: {}", result.script);
    assert!(result.script.contains("Dropped tables (destructive)"));
}

#[test]
fn a_foreign_key_into_the_desired_schema_points_into_the_changed_schema() {
    let mut orders = table(
        "b",
        "orders",
        vec![
            column("id", "integer", true, false),
            column("person", "integer", false, true),
        ],
    );
    orders.foreign_keys.push(ForeignKeyInfo {
        name: "orders_person_fk".into(),
        columns: vec!["person".into()],
        ref_schema: Some("b".into()),
        ref_table: "people".into(),
        ref_columns: vec!["id".into()],
        on_delete: None,
        on_update: None,
    });
    let result = compare(&[], &[people("b"), orders]);
    assert!(
        result.script.contains("REFERENCES \"a\".\"people\""),
        "{}",
        result.script
    );
}

#[test]
fn the_report_lists_what_is_not_compared_instead_of_inventing_ddl() {
    let result = compare(&[people("a")], &[people("b")]);
    assert!(
        result.skipped.iter().any(|line| line.contains("indexes")),
        "{:?}",
        result.skipped
    );
}

#[test]
fn a_table_with_no_columns_read_is_ignored_on_both_sides() {
    let empty = table("b", "unreadable", Vec::new());
    let result = compare(&[], &[empty]);
    assert!(result.is_identical());
}
