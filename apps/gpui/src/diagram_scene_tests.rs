use bookie_app_services::er_diagram::{SchemaDiagram, TableRead};
use bookie_core::{ColumnInfo, ForeignKeyInfo};

use super::{CARD_WIDTH, build_scene};

fn column(name: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "integer".into(),
        nullable: !primary_key,
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

fn read(name: &str, columns: &[&str], references: Option<&str>) -> TableRead {
    TableRead {
        schema: None,
        name: name.into(),
        columns: columns.iter().map(|c| column(c, *c == "id")).collect(),
        foreign_keys: references
            .map(|target| {
                vec![ForeignKeyInfo {
                    name: format!("{name}_{target}"),
                    columns: vec![format!("{target}_id")],
                    ref_schema: None,
                    ref_table: target.into(),
                    ref_columns: vec!["id".into()],
                    on_delete: None,
                    on_update: None,
                }]
            })
            .unwrap_or_default(),
    }
}

fn diagram() -> SchemaDiagram {
    SchemaDiagram::build(vec![
        read("customers", &["id", "name"], None),
        read("orders", &["id", "customers_id", "total"], Some("customers")),
        read("notes", &["id"], None),
    ])
}

#[test]
fn every_table_gets_a_card_and_every_relation_an_edge() {
    let scene = build_scene(&diagram()).expect("a scene");
    assert_eq!(scene.cards.len(), 3);
    assert_eq!(scene.edges.len(), 1);
    assert!(
        scene
            .cards
            .iter()
            .all(|card| (card.shape.width - CARD_WIDTH).abs() < f64::EPSILON)
    );
}

#[test]
fn a_card_is_as_tall_as_its_columns() {
    let scene = build_scene(&diagram()).unwrap();
    let rows: Vec<usize> = scene.cards.iter().map(|card| card.shape.rows).collect();
    assert_eq!(rows, [2, 1, 3], "customers, notes, orders in table order");
}

#[test]
fn cards_do_not_overlap_and_the_extent_holds_them_all() {
    let scene = build_scene(&diagram()).unwrap();
    for (index, card) in scene.cards.iter().enumerate() {
        let rect = card.rect();
        assert!(rect.x >= scene.extent.x && rect.right() <= scene.extent.right());
        assert!(rect.y >= scene.extent.y && rect.bottom() <= scene.extent.bottom());
        for other in &scene.cards[index + 1..] {
            assert!(!rect.intersects(&other.rect()), "{rect:?} overlaps {:?}", other.rect());
        }
    }
}

#[test]
fn nothing_to_draw_gives_no_scene() {
    assert!(build_scene(&SchemaDiagram::default()).is_none());
}
