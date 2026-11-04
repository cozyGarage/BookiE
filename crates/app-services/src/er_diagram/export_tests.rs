use super::*;
use crate::er_diagram::{DiagramColumn, DiagramRelation, DiagramTable, SchemaDiagram};

fn column(name: &str, type_name: &str, primary_key: bool, foreign_key: bool, nullable: bool) -> DiagramColumn {
    DiagramColumn {
        name: name.into(),
        type_name: type_name.into(),
        primary_key,
        foreign_key,
        nullable,
    }
}

fn fixture() -> SchemaDiagram {
    SchemaDiagram {
        tables: vec![
            DiagramTable {
                schema: Some("public".into()),
                name: "orders".into(),
                columns: vec![
                    column("id", "integer", true, false, false),
                    column("user_id", "integer", false, true, false),
                    column("note", "character varying(255)", false, false, true),
                ],
            },
            DiagramTable {
                schema: Some("public".into()),
                name: "users".into(),
                columns: vec![column("id", "integer", true, false, false)],
            },
        ],
        relations: vec![DiagramRelation {
            name: "orders_user_fk".into(),
            from: 0,
            from_columns: vec![1],
            to: 1,
            to_columns: vec![0],
            optional: false,
            unique: false,
        }],
    }
}

#[test]
fn mermaid_lists_entities_attributes_keys_and_one_edge_per_relation() {
    assert_eq!(
        mermaid(&fixture()),
        "erDiagram\n\
         \x20   \"public.orders\" {\n\
         \x20       integer id PK\n\
         \x20       integer user_id FK\n\
         \x20       character_varying_255 note\n\
         \x20   }\n\
         \x20   \"public.users\" {\n\
         \x20       integer id PK\n\
         \x20   }\n\
         \x20   \"public.users\" ||--o{ \"public.orders\" : \"orders_user_fk\"\n"
    );
}

#[test]
fn mermaid_cardinality_follows_optional_and_unique_foreign_keys() {
    let mut diagram = fixture();
    diagram.relations[0].optional = true;
    assert!(mermaid(&diagram).contains("\"public.users\" |o--o{ \"public.orders\""));
    diagram.relations[0].unique = true;
    assert!(mermaid(&diagram).contains("\"public.users\" |o--o| \"public.orders\""));
    diagram.relations[0].optional = false;
    assert!(mermaid(&diagram).contains("\"public.users\" ||--o| \"public.orders\""));
}

#[test]
fn dbml_lists_tables_columns_and_refs() {
    assert_eq!(
        dbml(&fixture()),
        "Table public.orders {\n\
         \x20 id integer [pk]\n\
         \x20 user_id integer [not null]\n\
         \x20 note \"character varying(255)\"\n\
         }\n\
         \n\
         Table public.users {\n\
         \x20 id integer [pk]\n\
         }\n\
         \n\
         Ref orders_user_fk: public.orders.user_id > public.users.id\n"
    );
}

#[test]
fn dbml_writes_composite_and_one_to_one_refs() {
    let mut diagram = fixture();
    diagram.tables[0]
        .columns
        .push(column("tenant_id", "integer", false, true, false));
    diagram.tables[1]
        .columns
        .push(column("tenant_id", "integer", true, false, false));
    diagram.relations[0].from_columns = vec![1, 3];
    diagram.relations[0].to_columns = vec![0, 1];
    diagram.relations[0].unique = true;
    assert!(
        dbml(&diagram)
            .contains("Ref orders_user_fk: public.orders.(user_id, tenant_id) - public.users.(id, tenant_id)\n")
    );
}

#[test]
fn odd_names_are_quoted_and_cannot_break_out_of_the_text() {
    let mut diagram = fixture();
    diagram.tables[0].name = "weird \"name\"\n{".into();
    diagram.relations[0].name = "fk \"x\"\nRef".into();
    let mermaid = mermaid(&diagram);
    let dbml = dbml(&diagram);
    assert_eq!(dbml.lines().filter(|line| line.starts_with("Ref")).count(), 1);
    assert!(mermaid.lines().all(|line| !line.trim_start().starts_with("Ref")));
    assert_eq!(mermaid.lines().count(), 10);
    assert!(dbml.contains("Table public.\"weird \\\"name\\\" {\""));
}

#[test]
fn a_relation_that_cannot_be_drawn_is_left_out() {
    let mut diagram = fixture();
    diagram.relations[0].to_columns = vec![];
    assert!(!mermaid(&diagram).contains("orders_user_fk"));
    assert!(!dbml(&diagram).contains("Ref"));
    diagram.relations[0].to_columns = vec![0];
    diagram.relations[0].to = 9;
    assert!(!mermaid(&diagram).contains("orders_user_fk"));
}
