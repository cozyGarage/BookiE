use super::*;

fn column(name: &str) -> DraftColumn {
    DraftColumn {
        original: None,
        name: name.into(),
        data_type: "varchar(20)".into(),
        nullable: true,
        primary_key: false,
        auto_increment: false,
        default_value: None,
        comment: None,
        collation: None,
    }
}

fn create(driver_id: &str, collation: Option<&str>) -> Result<Vec<String>, BuildDdlError> {
    build_create_table_with_default_collation(driver_id, None, "t", &[column("label")], &[], &[], collation)
}

#[test]
fn mysql_names_the_table_default_collation_after_the_column_list() {
    let statements = create("mysql", Some("utf8mb4_bin")).unwrap();
    assert!(
        statements[0].ends_with(")\nDEFAULT COLLATE utf8mb4_bin"),
        "{statements:?}"
    );
}

#[test]
fn no_default_collation_leaves_the_statement_unchanged() {
    for driver_id in ["mysql", "postgres", "sqlite", "mssql", "clickhouse"] {
        assert_eq!(
            create(driver_id, None).unwrap(),
            build_create_table(driver_id, None, "t", &[column("label")], &[], &[]).unwrap(),
            "{driver_id}"
        );
    }
}

#[test]
fn an_engine_without_table_level_collations_refuses_one_instead_of_inventing_syntax() {
    for driver_id in ["postgres", "sqlite", "mssql", "clickhouse", "duckdb"] {
        let error = create(driver_id, Some("utf8mb4_bin")).unwrap_err();
        assert!(
            matches!(error, BuildDdlError::TableCollationUnsupported),
            "{driver_id}: {error:?}"
        );
    }
}

#[test]
fn an_unsafe_or_empty_default_collation_is_refused() {
    for collation in ["", "   ", "utf8mb4_bin; DROP TABLE t", "a b", "x`y", "a\u{0}b"] {
        let error = create("mysql", Some(collation)).unwrap_err();
        assert!(
            matches!(error, BuildDdlError::UnsafeCollation(_)),
            "{collation:?}: {error:?}"
        );
    }
}

#[test]
fn the_new_table_op_carries_the_default_collation_into_the_statement() {
    let op = StructureOp::CreateTable {
        schema: None,
        table: "t".into(),
        columns: vec![column("label")],
        indexes: Vec::new(),
        fks: Vec::new(),
        default_collation: Some("utf8mb4_bin".into()),
    };
    let statements = materialize_ops(&[op], "mysql").unwrap();
    assert!(statements[0].contains("DEFAULT COLLATE utf8mb4_bin"), "{statements:?}");
}
