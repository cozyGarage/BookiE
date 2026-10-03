use crate::query::{ColumnInfo, QualifiedTypeName, Value};
use crate::sql_dialect::{build_insert_from_draft, build_keyed_update};

#[test]
fn postgres_enum_write_casts_quote_catalog_schema_and_type_names() {
    let columns = [
        ColumnInfo {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            primary_key: true,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
        },
        ColumnInfo {
            name: "status".into(),
            data_type: "status_type".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: Some(QualifiedTypeName {
                schema: "odd schema".into(),
                name: "status\"type".into(),
            }),
        },
    ];

    let (sql, _) = build_keyed_update(
        "postgres",
        None,
        "t",
        &columns,
        &[(1, Value::Text("ready".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(
        sql,
        r#"UPDATE "t" SET "status" = $1::text::"odd schema"."status""type" WHERE "id" = $2"#
    );

    let (sql, _) = build_insert_from_draft("postgres", None, "t", &columns[1..], &[Value::Null]).unwrap();
    assert_eq!(
        sql,
        r#"INSERT INTO "t" ("status") VALUES ($1::text::"odd schema"."status""type")"#
    );
}
