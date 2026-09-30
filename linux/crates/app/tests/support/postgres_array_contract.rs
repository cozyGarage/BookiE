use super::parse_input_for_driver;

#[test]
fn value_contract_postgres_scalar_named_array_types_stay_text_in_the_grid_parser() {
    for (data_type, literal) in [
        ("uuid[]", "{550e8400-e29b-41d4-a716-446655440000,NULL}"),
        ("date[]", "{2026-09-30,NULL}"),
        ("time[]", "{12:34:56.123456,NULL}"),
        ("numeric[]", "{1.2300,NULL}"),
        ("boolean[]", "{true,NULL}"),
        ("timestamp with time zone[]", "{2026-09-30 12:34:56+00,NULL}"),
    ] {
        let column = ColumnInfo {
            name: "value".into(),
            data_type: data_type.into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        };
        let parsed = parse_input_for_driver(literal, Some(&column), "postgres")
            .unwrap_or_else(|error| panic!("{data_type}: {error}"));
        assert_eq!(parsed, Value::Text(literal.into()), "{data_type}");
    }
}
use tablepro_core::{ColumnInfo, Value};

#[test]
fn value_contract_postgres_float8_array_grid_literal_keeps_subnormal_and_signed_zero_text() {
    let literal = "{1.0000000000000002,-0,5e-324,NaN,Infinity,-Infinity,NULL}";
    let columns = vec![
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
        },
        ColumnInfo {
            name: "value".into(),
            data_type: "float8[]".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        },
    ];
    let parsed = parse_input_for_driver(literal, Some(&columns[1]), "postgres").unwrap();
    assert_eq!(parsed, Value::Text(literal.into()));
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "float8_array_grid",
        &columns,
        &[(1, parsed)],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(sql.contains("$1::text::pg_catalog.float8[]"));
    assert_eq!(params[0], Value::Text(literal.into()));
}

#[test]
fn value_contract_postgres_boolean_array_grid_literal_stays_text_through_the_keyed_update_builder() {
    let literal = "{true,false,NULL}";
    let columns = vec![
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
        },
        ColumnInfo {
            name: "value".into(),
            data_type: "boolean[]".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        },
    ];
    let parsed = parse_input_for_driver(literal, Some(&columns[1]), "postgres").unwrap();
    assert_eq!(parsed, Value::Text(literal.into()));
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "bool_array_grid",
        &columns,
        &[(1, parsed)],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(sql.contains("$1::text::pg_catalog.bool[]"));
    assert_eq!(params[0], Value::Text(literal.into()));
}

#[test]
fn value_contract_postgres_bytea_array_grid_literal_keeps_escaped_bytes_through_the_builder() {
    let literal = r#"{"\\x00ff","\\x5c5c","",NULL}"#;
    let columns = vec![
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
        },
        ColumnInfo {
            name: "value".into(),
            data_type: "bytea[]".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        },
    ];
    let parsed = parse_input_for_driver(literal, Some(&columns[1]), "postgres").unwrap();
    assert_eq!(parsed, Value::Text(literal.into()));
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "bytea_array_grid",
        &columns,
        &[(1, parsed)],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(sql.contains("$1::text::pg_catalog.bytea[]"));
    assert_eq!(params[0], Value::Text(literal.into()));
}

#[test]
fn value_contract_postgres_uuid_array_grid_literal_stays_text_through_the_keyed_update_builder() {
    let literal = "{550e8400-e29b-41d4-a716-446655440000,6ba7b810-9dad-11d1-80b4-00c04fd430c8,NULL}";
    let columns = vec![
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
        },
        ColumnInfo {
            name: "value".into(),
            data_type: "uuid[]".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        },
    ];
    let parsed = parse_input_for_driver(literal, Some(&columns[1]), "postgres").unwrap();
    assert_eq!(parsed, Value::Text(literal.into()));
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "uuid_array_grid",
        &columns,
        &[(1, parsed)],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(sql.contains("$1::text::pg_catalog.uuid[]"));
    assert_eq!(params[0], Value::Text(literal.into()));
}

#[test]
fn value_contract_postgres_timestamptz_array_grid_literal_stays_text_through_the_keyed_update_builder() {
    let literal = r#"{"2026-09-30 12:34:56.123456+05:30","1999-12-31 23:59:59.000001-07:00",NULL}"#;
    let columns = vec![
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
        },
        ColumnInfo {
            name: "value".into(),
            data_type: "timestamp with time zone[]".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        },
    ];
    let parsed = parse_input_for_driver(literal, Some(&columns[1]), "postgres").unwrap();
    assert_eq!(parsed, Value::Text(literal.into()));
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "timestamptz_array_grid",
        &columns,
        &[(1, parsed)],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(sql.contains("$1::text::pg_catalog.timestamptz[]"));
    assert_eq!(params[0], Value::Text(literal.into()));
}
