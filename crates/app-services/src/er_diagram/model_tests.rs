use tablepro_core::{ColumnInfo, ForeignKeyInfo};

use super::*;

fn column(name: &str, primary_key: bool, nullable: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "integer".into(),
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

fn key(
    name: &str,
    columns: &[&str],
    ref_schema: Option<&str>,
    ref_table: &str,
    ref_columns: &[&str],
) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.into(),
        columns: columns.iter().map(|column| column.to_string()).collect(),
        ref_schema: ref_schema.map(str::to_string),
        ref_table: ref_table.into(),
        ref_columns: ref_columns.iter().map(|column| column.to_string()).collect(),
        on_delete: None,
        on_update: None,
    }
}

fn read(name: &str, columns: Vec<ColumnInfo>, foreign_keys: Vec<ForeignKeyInfo>) -> TableRead {
    TableRead {
        schema: Some("shop".into()),
        name: name.into(),
        columns,
        foreign_keys,
    }
}

fn shop() -> SchemaDiagram {
    SchemaDiagram::build(vec![
        read(
            "orders",
            vec![column("id", true, false), column("customer_id", false, true)],
            vec![key("orders_customer", &["customer_id"], None, "customers", &["id"])],
        ),
        read("customers", vec![column("id", true, false)], Vec::new()),
    ])
}

#[test]
fn tables_are_sorted_by_name_and_keys_become_relations_between_table_indexes() {
    let diagram = shop();

    let names: Vec<&str> = diagram.tables.iter().map(|table| table.name.as_str()).collect();
    assert_eq!(names, ["customers", "orders"]);
    assert_eq!(diagram.relations.len(), 1);
    let relation = &diagram.relations[0];
    assert_eq!((relation.from, relation.to), (1, 0));
    assert_eq!(relation.from_columns, [1]);
    assert_eq!(relation.to_columns, [0]);
    assert_eq!(relation.name, "orders_customer");
}

#[test]
fn a_key_is_optional_when_a_key_column_takes_null_and_unique_when_it_covers_the_primary_key() {
    let diagram = shop();

    assert!(diagram.relations[0].optional);
    assert!(!diagram.relations[0].unique);

    let one_to_one = SchemaDiagram::build(vec![
        read(
            "profile",
            vec![column("user_id", true, false)],
            vec![key("profile_user", &["user_id"], None, "users", &["id"])],
        ),
        read("users", vec![column("id", true, false)], Vec::new()),
    ]);
    assert!(!one_to_one.relations[0].optional);
    assert!(one_to_one.relations[0].unique);
}

#[test]
fn a_key_to_a_table_outside_the_read_is_left_out_and_a_table_without_columns_is_dropped() {
    let diagram = SchemaDiagram::build(vec![
        read(
            "orders",
            vec![column("id", true, false)],
            vec![key("elsewhere", &["id"], Some("other"), "customers", &["id"])],
        ),
        read("gone", Vec::new(), Vec::new()),
    ]);

    assert_eq!(diagram.tables.len(), 1);
    assert!(diagram.relations.is_empty());
}

#[test]
fn key_columns_are_marked_and_a_self_reference_is_one_relation() {
    let diagram = SchemaDiagram::build(vec![read(
        "employees",
        vec![column("id", true, false), column("manager_id", false, true)],
        vec![key("manager", &["manager_id"], Some("shop"), "employees", &["id"])],
    )]);

    assert!(diagram.tables[0].columns[1].foreign_key);
    assert!(!diagram.tables[0].columns[0].foreign_key);
    assert_eq!((diagram.relations[0].from, diagram.relations[0].to), (0, 0));
    assert_eq!(diagram.neighbours(0), (Vec::<&str>::new(), Vec::<&str>::new()));
}

#[test]
fn neighbours_list_the_tables_a_table_points_at_and_those_pointing_at_it() {
    let diagram = shop();

    assert_eq!(diagram.neighbours(1), (vec!["customers"], Vec::<&str>::new()));
    assert_eq!(diagram.neighbours(0), (Vec::<&str>::new(), vec!["orders"]));
    assert_eq!(diagram.position_of(Some("shop"), "orders"), Some(1));
    assert_eq!(diagram.position_of(None, "orders"), None);
}
