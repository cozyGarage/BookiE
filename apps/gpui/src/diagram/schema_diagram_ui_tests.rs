use std::path::Path;

use bookie_app_services::er_diagram::{SchemaDiagram, TableRead};
use bookie_core::{ColumnInfo, ForeignKeyInfo};

use super::{DiagramFormat, EXPORT_CANCELLED, EXPORT_RUNNING, diagram_text, format_for_path, write_new_file};

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

fn diagram() -> SchemaDiagram {
    SchemaDiagram::build(vec![
        TableRead {
            schema: None,
            name: "customers".into(),
            columns: vec![column("id", true)],
            foreign_keys: Vec::new(),
        },
        TableRead {
            schema: None,
            name: "orders".into(),
            columns: vec![column("id", true), column("customer_id", false)],
            foreign_keys: vec![ForeignKeyInfo {
                name: "orders_customer".into(),
                columns: vec!["customer_id".into()],
                ref_schema: None,
                ref_table: "customers".into(),
                ref_columns: vec!["id".into()],
                on_delete: None,
                on_update: None,
            }],
        },
    ])
}

#[test]
fn the_extension_picks_the_format() {
    assert_eq!(format_for_path(Path::new("a.mmd")), Some(DiagramFormat::Mermaid));
    assert_eq!(format_for_path(Path::new("a.MERMAID")), Some(DiagramFormat::Mermaid));
    assert_eq!(format_for_path(Path::new("a.dbml")), Some(DiagramFormat::Dbml));
    assert_eq!(format_for_path(Path::new("a.png")), None);
    assert_eq!(format_for_path(Path::new("noextension")), None);
}

#[test]
fn cancelled_diagram_output_is_not_published_and_existing_files_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("diagram.mmd");
    let cancelled = std::sync::atomic::AtomicU8::new(EXPORT_CANCELLED);
    assert!(write_new_file(&path, "partial", &cancelled).is_err());
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    std::fs::write(&path, "existing").unwrap();
    let running = std::sync::atomic::AtomicU8::new(EXPORT_RUNNING);
    assert!(write_new_file(&path, "replacement", &running).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing");
}

#[gpui::test]
fn cancellation_after_publication_starts_does_not_claim_success(cx: &mut gpui::TestAppContext) {
    let (view, cx, _orders) = crate::live_ui_tests::open_live(cx);
    view.update(cx, |view, cx| {
        view.open_schema_diagram_bar(cx);
        view.search.diagram.export_phase = Some(std::sync::Arc::new(std::sync::atomic::AtomicU8::new(
            super::EXPORT_PUBLISHING,
        )));
        view.close_schema_diagram_export();
        assert!(view.search.diagram.open);
        assert!(view.status.contains("can no longer be canceled"));
    });
}

#[test]
fn each_format_renders_its_tables_and_the_relation() {
    let mermaid = diagram_text(DiagramFormat::Mermaid, &diagram());
    assert!(mermaid.starts_with("erDiagram"), "{mermaid}");
    assert!(
        mermaid.contains("customers") && mermaid.contains("orders_customer"),
        "{mermaid}"
    );
    let dbml = diagram_text(DiagramFormat::Dbml, &diagram());
    assert!(
        dbml.contains("Table customers") || dbml.contains("Table \"customers\""),
        "{dbml}"
    );
    assert!(dbml.contains("Ref"), "{dbml}");
}
